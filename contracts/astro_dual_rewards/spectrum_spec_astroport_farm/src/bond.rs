use cosmwasm_std::{
    attr, to_binary, CanonicalAddr, CosmosMsg, Decimal, Deps, DepsMut, MessageInfo, Order,
    QueryRequest, Response, StdError, StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use crate::state::{
    pool_info_read, pool_info_store, read_config, read_state, rewards_read, rewards_store,
    state_store, Config, PoolInfo, RewardInfo, State,
};

use cw20::Cw20ExecuteMsg;
use spectrum_protocol::gov::{BalanceResponse, ExecuteMsg, QueryMsg};
use spectrum_protocol::math::UDec128;
use spectrum_protocol::spec_astroport_farm::{RewardInfoResponse, RewardInfoResponseItem};

pub fn bond(
    deps: DepsMut,
    info: MessageInfo,
    staker_addr: String,
    asset_token: String,
    amount: Uint128,
    sender: String,
) -> StdResult<Response> {
    let staker_addr_raw = deps.api.addr_canonicalize(&staker_addr)?;
    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    let config = read_config(deps.storage)?;
    if config.generator_proxy.is_none() {
        return Err(StdError::generic_err(
            "generator_proxy must be set first",
        ));
    }

    let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;

    // only staking token contract and astroport generator proxy can execute this message
    if pool_info.staking_token != deps.api.addr_canonicalize(info.sender.as_str())? || sender != deps.api.addr_humanize(&config.generator_proxy.unwrap())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut state = read_state(deps.storage)?;
    // Withdraw reward to pending reward; before changing share
    let config = read_config(deps.storage)?;
    if !pool_info.total_bond_amount.is_zero() {
        deposit_reward(deps.as_ref(), &mut state, &config, false)?;
        reward_to_pool(&state, &mut pool_info)?;
    }

    let mut reward_info = rewards_read(deps.storage, &staker_addr_raw)
        .may_load(asset_token_raw.as_slice())?
        .unwrap_or_else(|| RewardInfo {
            spec_share_index: pool_info.spec_share_index,
            bond_amount: Uint128::zero(),
            spec_share: Uint128::zero(),
        });
    before_share_change(&pool_info, &mut reward_info)?;

    pool_info.total_bond_amount += amount;
    reward_info.bond_amount += amount;
    rewards_store(deps.storage, &staker_addr_raw)
        .save(asset_token_raw.as_slice(), &reward_info)?;
    pool_info_store(deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    state_store(deps.storage).save(&state)?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "bond"),
        attr("staker_addr", staker_addr),
        attr("asset_token", asset_token),
        attr("amount", amount),
    ]))
}

pub fn deposit_reward(
    deps: Deps,
    state: &mut State,
    config: &Config,
    query: bool,
) -> StdResult<BalanceResponse> {
    if state.total_weight == 0 {
        return Ok(BalanceResponse {
            share: Uint128::zero(),
            balance: Uint128::zero(),
            locked_balance: vec![],
            pools: vec![],
        });
    }

    let staked: BalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        msg: to_binary(&QueryMsg::balance {
            address: deps.api.addr_humanize(&state.contract_addr)?.to_string(),
        })?,
    }))?;
    let diff = staked.share.checked_sub(state.previous_spec_share);
    let deposit_share = if query {
        diff.unwrap_or_else(|_| Uint128::zero())
    } else {
        diff?
    };
    let share_per_weight = Decimal::from_ratio(deposit_share, state.total_weight);
    state.spec_share_index = state.spec_share_index + share_per_weight;
    state.previous_spec_share = staked.share;

    Ok(staked)
}

fn reward_to_pool(state: &State, pool_info: &mut PoolInfo) -> StdResult<()> {
    if pool_info.total_bond_amount.is_zero() {
        return Ok(());
    }

    let share = (UDec128::from(state.spec_share_index) - pool_info.state_spec_share_index.into())
        * Uint128::from(pool_info.weight as u128);
    let share_per_bond = share / pool_info.total_bond_amount;
    pool_info.spec_share_index = pool_info.spec_share_index + share_per_bond.into();
    pool_info.state_spec_share_index = state.spec_share_index;

    Ok(())
}

fn before_share_change(pool_info: &PoolInfo, reward_info: &mut RewardInfo) -> StdResult<()> {
    let share =
        reward_info.bond_amount * (pool_info.spec_share_index - reward_info.spec_share_index);
    reward_info.spec_share += share;
    reward_info.spec_share_index = pool_info.spec_share_index;
    Ok(())
}

pub fn unbond(
    deps: DepsMut,
    info: MessageInfo,
    asset_token: String,
    amount: Uint128,
) -> StdResult<Response> {
    let staker_addr_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    let config = read_config(deps.storage)?;
    if config.generator_proxy.is_none() {
        return Err(StdError::generic_err(
            "generator_proxy must be set first",
        ));
    }

    // only astroport generator proxy can execute this message
    if info.sender != deps.api.addr_humanize(&config.generator_proxy.unwrap())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut reward_info =
        rewards_read(deps.storage, &staker_addr_raw).load(asset_token_raw.as_slice())?;

    if reward_info.bond_amount < amount {
        return Err(StdError::generic_err("Cannot unbond more than bond amount"));
    }

    let mut state = read_state(deps.storage)?;
    let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;

    // Distribute reward to pending reward; before changing share
    let config = read_config(deps.storage)?;
    deposit_reward(deps.as_ref(), &mut state, &config, false)?;
    reward_to_pool(&state, &mut pool_info)?;
    before_share_change(&pool_info, &mut reward_info)?;

    // Decrease bond amount
    pool_info.total_bond_amount = pool_info.total_bond_amount.checked_sub(amount)?;
    reward_info.bond_amount = reward_info.bond_amount.checked_sub(amount)?;

    // Update rewards info
    if reward_info.spec_share.is_zero() && reward_info.bond_amount.is_zero() {
        rewards_store(deps.storage, &staker_addr_raw).remove(asset_token_raw.as_slice());
    } else {
        rewards_store(deps.storage, &staker_addr_raw)
            .save(asset_token_raw.as_slice(), &reward_info)?;
    }

    // Update pool info
    pool_info_store(deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps
                .api
                .addr_humanize(&pool_info.staking_token)?
                .to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount,
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            attr("action", "unbond"),
            attr("staker_addr", info.sender),
            attr("asset_token", asset_token),
            attr("amount", amount),
        ]))
}

pub fn withdraw(
    deps: DepsMut,
    info: MessageInfo,
    asset_token: Option<String>,
    spec_amount: Option<Uint128>,
) -> StdResult<Response> {
    let staker_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let asset_token = asset_token.map(|a| deps.api.addr_canonicalize(&a).unwrap());
    let mut state = read_state(deps.storage)?;

    let config = read_config(deps.storage)?;
    if config.generator_proxy.is_none() {
        return Err(StdError::generic_err(
            "generator_proxy must be set first",
        ));
    }
    
    // only astroport generator proxy can execute this message
    if info.sender != deps.api.addr_humanize(&config.generator_proxy.clone().unwrap())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let staked = deposit_reward(deps.as_ref(), &mut state, &config, false)?;
    let (amount, share) = withdraw_reward(
        deps.storage,
        &state,
        &staker_addr,
        &asset_token,
        &staked,
        spec_amount,
    )?;
    state.previous_spec_share = state.previous_spec_share.checked_sub(share)?;
    state_store(deps.storage).save(&state)?;
    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
                msg: to_binary(&ExecuteMsg::withdraw {
                    amount: Some(amount),
                    days: None,
                })?,
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount,
                })?,
                funds: vec![],
            }),
        ])
        .add_attributes(vec![attr("action", "withdraw"), attr("amount", amount)]))
}

#[allow(clippy::too_many_arguments)]
fn withdraw_reward(
    storage: &mut dyn Storage,
    state: &State,
    staker_addr: &CanonicalAddr,
    asset_token: &Option<CanonicalAddr>,
    staked: &BalanceResponse,
    mut request_spec_amount: Option<Uint128>,
) -> StdResult<(Uint128, Uint128)> {
    let rewards_bucket = rewards_read(storage, staker_addr);

    // single reward withdraw
    let reward_pairs: Vec<(CanonicalAddr, RewardInfo)>;
    if let Some(asset_token) = asset_token {
        let key = asset_token.as_slice();
        let reward_info = rewards_bucket.may_load(key)?;
        reward_pairs = if let Some(reward_info) = reward_info {
            vec![(asset_token.clone(), reward_info)]
        } else {
            vec![]
        };
    } else {
        reward_pairs = rewards_bucket
            .range(None, None, Order::Ascending)
            .map(|item| {
                let (k, v) = item?;
                Ok((CanonicalAddr::from(k), v))
            })
            .collect::<StdResult<Vec<(CanonicalAddr, RewardInfo)>>>()?;
    }

    let mut amount = Uint128::zero();
    let mut share = Uint128::zero();
    for reward_pair in reward_pairs {
        let (asset_token_raw, mut reward_info) = reward_pair;

        // Withdraw reward to pending reward
        let key = asset_token_raw.as_slice();
        let mut pool_info = pool_info_read(storage).load(key)?;
        reward_to_pool(state, &mut pool_info)?;
        before_share_change(&pool_info, &mut reward_info)?;

        let (asset_spec_share, asset_spec_amount) = if let Some(request_amount) = request_spec_amount {
            let avail_amount = calc_spec_balance(reward_info.spec_share, staked);
            let asset_spec_amount = if request_amount > avail_amount { avail_amount } else { request_amount };
            let mut asset_spec_share = calc_spec_share(asset_spec_amount, staked);
            if calc_spec_balance(asset_spec_share, staked) < asset_spec_amount {
                asset_spec_share += Uint128::new(1u128);
            }
            request_spec_amount = Some(request_amount.checked_sub(asset_spec_amount)?);
            (asset_spec_share, asset_spec_amount)
        } else {
            (reward_info.spec_share, calc_spec_balance(reward_info.spec_share, staked))
        };
        share += asset_spec_share;
        amount += asset_spec_amount;
        reward_info.spec_share = reward_info.spec_share.checked_sub(asset_spec_share)?;

        // Update rewards info
        pool_info_store(storage).save(key, &pool_info)?;
        if reward_info.spec_share.is_zero() && reward_info.bond_amount.is_zero() {
            rewards_store(storage, staker_addr).remove(key);
        } else {
            rewards_store(storage, staker_addr).save(key, &reward_info)?;
        }
    }

    if let Some(request_amount) = request_spec_amount {
        if !request_amount.is_zero() {
            return Err(StdError::generic_err("Cannot withdraw more than remaining amount"));
        }
    }

    Ok((amount, share))
}

fn calc_spec_balance(share: Uint128, staked: &BalanceResponse) -> Uint128 {
    if staked.share.is_zero() {
        Uint128::zero()
    } else {
        share.multiply_ratio(staked.balance, staked.share)
    }
}

fn calc_spec_share(amount: Uint128, stated: &BalanceResponse) -> Uint128 {
    if stated.balance.is_zero() {
        amount
    } else {
        amount.multiply_ratio(stated.share, stated.balance)
    }
}

pub fn query_reward_info(
    deps: Deps,
    staker_addr: String,
    asset_token: Option<String>,
) -> StdResult<RewardInfoResponse> {
    let staker_addr_raw = deps.api.addr_canonicalize(&staker_addr)?;
    let mut state = read_state(deps.storage)?;

    let config = read_config(deps.storage)?;
    let staked = deposit_reward(deps, &mut state, &config, true)?;
    let reward_infos = read_reward_infos(
        deps,
        &state,
        &staker_addr_raw,
        &asset_token,
        &staked,
    )?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_infos,
    })
}

fn read_reward_infos(
    deps: Deps,
    state: &State,
    staker_addr: &CanonicalAddr,
    asset_token: &Option<String>,
    staked: &BalanceResponse,
) -> StdResult<Vec<RewardInfoResponseItem>> {
    let rewards_bucket = rewards_read(deps.storage, staker_addr);
    let reward_infos: Vec<RewardInfoResponseItem>;
    if let Some(asset_token) = asset_token {
        let asset_token_raw = deps.api.addr_canonicalize(asset_token)?;
        let key = asset_token_raw.as_slice();
        reward_infos = if let Some(mut reward_info) = rewards_bucket.may_load(key)? {
            let spec_share_index = reward_info.spec_share_index;
            let mut pool_info = pool_info_read(deps.storage).load(key)?;

            reward_to_pool(state, &mut pool_info)?;
            before_share_change(&pool_info, &mut reward_info)?;

            vec![RewardInfoResponseItem {
                asset_token: asset_token.clone(),
                bond_amount: reward_info.bond_amount,
                spec_share: reward_info.spec_share,
                pending_spec_reward: calc_spec_balance(reward_info.spec_share, staked),
                spec_share_index,
            }]
        } else {
            vec![]
        };
    } else {
        reward_infos = rewards_bucket
            .range(None, None, Order::Ascending)
            .map(|item| {
                let (key, reward_info) = item?;
                let asset_token_raw = CanonicalAddr::from(key);
                let mut reward_info = reward_info;

                let spec_share_index = reward_info.spec_share_index;
                let mut pool_info =
                    pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
                reward_to_pool(state, &mut pool_info)?;
                before_share_change(&pool_info, &mut reward_info)?;

                Ok(RewardInfoResponseItem {
                    asset_token: deps.api.addr_humanize(&asset_token_raw)?.to_string(),
                    bond_amount: reward_info.bond_amount,
                    spec_share: reward_info.spec_share,
                    pending_spec_reward: calc_spec_balance(reward_info.spec_share, staked),
                    spec_share_index,
                })
            })
            .collect::<StdResult<Vec<RewardInfoResponseItem>>>()?;
    }

    Ok(reward_infos)
}
