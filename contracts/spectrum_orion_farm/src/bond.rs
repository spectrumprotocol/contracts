use cosmwasm_std::{
    attr, to_binary, Api, CanonicalAddr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::state::{
    pool_info_read, pool_info_store, read_config, read_state, rewards_read, rewards_store,
    state_store, Config, PoolInfo, RewardInfo, State,
};

use cw20::Cw20ExecuteMsg;

use crate::querier::query_orion_pool_balance;
use orion::lp_staking::{Cw20HookMsg as OrionCw20HookMsg, ExecuteMsg as OrionStakingExecuteMsg};
use spectrum_protocol::gov::{
    BalanceResponse as SpecBalanceResponse, ExecuteMsg as SpecExecuteMsg, QueryMsg as SpecQueryMsg,
};
use spectrum_protocol::math::UDec128;
use spectrum_protocol::orion_farm::{RewardInfoResponse, RewardInfoResponseItem};

#[allow(clippy::too_many_arguments)]
fn bond_internal(
    deps: DepsMut,
    sender_addr_raw: CanonicalAddr,
    asset_token_raw: CanonicalAddr,
    amount_to_auto: Uint128,
    deposit_fee: Decimal,
    lp_balance: Uint128,
    config: &Config,
) -> StdResult<PoolInfo> {
    let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let mut state = read_state(deps.storage)?;

    // update reward index; before changing share
    if !pool_info.total_auto_bond_share.is_zero() || !pool_info.total_stake_bond_share.is_zero() {
        deposit_spec_reward(deps.as_ref(), &mut state, config, false)?;
        spec_reward_to_pool(&state, &mut pool_info, lp_balance)?;
    }

    // withdraw reward to pending reward; before changing share
    let mut reward_info = rewards_read(deps.storage, &sender_addr_raw)
        .may_load(asset_token_raw.as_slice())?
        .unwrap_or_else(|| RewardInfo {
            farm_share_index: pool_info.farm_share_index,
            auto_spec_share_index: pool_info.auto_spec_share_index,
            stake_spec_share_index: pool_info.stake_spec_share_index,
            auto_bond_share: Uint128::zero(),
            stake_bond_share: Uint128::zero(),
            spec_share: Uint128::zero(),
            farm_share: Uint128::zero(),
        });
    before_share_change(&pool_info, &mut reward_info)?;

    // increase bond_amount
    increase_bond_amount(
        &mut pool_info,
        &mut reward_info,
        deposit_fee,
        amount_to_auto,
        lp_balance,
    )?;

    rewards_store(deps.storage, &sender_addr_raw)
        .save(asset_token_raw.as_slice(), &reward_info)?;
    pool_info_store(deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    state_store(deps.storage).save(&state)?;

    Ok(pool_info)
}

pub fn bond(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender_addr: String,
    asset_token: String,
    amount: Uint128,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {

    if compound_rate.is_none() || compound_rate.unwrap_or_else(|| Decimal::zero()) != Decimal::one(){
        return Err(StdError::generic_err("auto-stake is disabled"));
    }

    let staker_addr_raw = deps.api.addr_canonicalize(&sender_addr)?;
    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    let pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;

    // only staking token contract can execute this message
    if pool_info.staking_token != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;

    let amount_to_auto = amount;

    let lp_balance = query_orion_pool_balance(
        deps.as_ref(),
        &config.orion_staking,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        env.block.time.seconds()
    )?;

    bond_internal(
        deps.branch(),
        staker_addr_raw,
        asset_token_raw.clone(),
        amount_to_auto,
        config.deposit_fee,
        lp_balance,
        &config,
    )?;

    stake_token(
        deps.api,
        config.orion_staking,
        pool_info.staking_token,
        asset_token_raw,
        amount,
    )
}

pub fn deposit_spec_reward(
    deps: Deps,
    state: &mut State,
    config: &Config,
    query: bool,
) -> StdResult<SpecBalanceResponse> {
    if state.total_weight == 0 {
        return Ok(SpecBalanceResponse {
            share: Uint128::zero(),
            balance: Uint128::zero(),
            locked_balance: vec![],
            pools: vec![],
        });
    }

    let staked: SpecBalanceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&SpecQueryMsg::balance {
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

fn spec_reward_to_pool(
    state: &State,
    pool_info: &mut PoolInfo,
    lp_balance: Uint128,
) -> StdResult<()> {
    if lp_balance.is_zero() {
        return Ok(());
    }

    let share = (UDec128::from(state.spec_share_index) - pool_info.state_spec_share_index.into())
        * Uint128::from(pool_info.weight as u128);

    // pool_info.total_stake_bond_amount / lp_balance = ratio for auto-stake
    // now stake_share is additional SPEC rewards for auto-stake
    let stake_share = share.multiply_ratio(pool_info.total_stake_bond_amount, lp_balance);

    // spec reward to staker is per stake bond share & auto bond share
    if !stake_share.is_zero() {
        let stake_share_per_bond = stake_share / pool_info.total_stake_bond_share;
        pool_info.stake_spec_share_index =
            pool_info.stake_spec_share_index + stake_share_per_bond.into();
    }

    // auto_share is additional SPEC rewards for auto-compound
    let auto_share = share - stake_share;
    if !auto_share.is_zero() {
        let auto_share_per_bond = auto_share / pool_info.total_auto_bond_share;
        pool_info.auto_spec_share_index =
            pool_info.auto_spec_share_index + auto_share_per_bond.into();
    }
    pool_info.state_spec_share_index = state.spec_share_index;

    Ok(())
}

// withdraw reward to pending reward
fn before_share_change(pool_info: &PoolInfo, reward_info: &mut RewardInfo) -> StdResult<()> {
    let farm_share =
        (pool_info.farm_share_index - reward_info.farm_share_index) * reward_info.stake_bond_share;
    reward_info.farm_share += farm_share;
    reward_info.farm_share_index = pool_info.farm_share_index;

    let stake_spec_share = reward_info.stake_bond_share
        * (pool_info.stake_spec_share_index - reward_info.stake_spec_share_index);
    let auto_spec_share = reward_info.auto_bond_share
        * (pool_info.auto_spec_share_index - reward_info.auto_spec_share_index);
    let spec_share = stake_spec_share + auto_spec_share;
    reward_info.spec_share += spec_share;
    reward_info.stake_spec_share_index = pool_info.stake_spec_share_index;
    reward_info.auto_spec_share_index = pool_info.auto_spec_share_index;

    Ok(())
}

// increase share amount in pool and reward info
fn increase_bond_amount(
    pool_info: &mut PoolInfo,
    reward_info: &mut RewardInfo,
    deposit_fee: Decimal,
    amount_to_auto: Uint128,
    lp_balance: Uint128,
) -> StdResult<()> {
    let auto_bond_amount = if deposit_fee.is_zero() {
        amount_to_auto
    } else {
        // calculate deposit fee;
        let deposit_fee = amount_to_auto * deposit_fee;

        // calculate amount after fee
        let auto_bond_amount = amount_to_auto.checked_sub(deposit_fee)?;

        auto_bond_amount
    };

    // convert amount to share & update
    let auto_bond_share = pool_info.calc_auto_bond_share(auto_bond_amount, lp_balance);
    pool_info.total_auto_bond_share += auto_bond_share;
    reward_info.auto_bond_share += auto_bond_share;

    Ok(())
}

// stake LP token to Orion Staking
fn stake_token(
    api: &dyn Api,
    orion_staking: CanonicalAddr,
    staking_token: CanonicalAddr,
    asset_token: CanonicalAddr,
    amount: Uint128,
) -> StdResult<Response> {
    let asset_token = api.addr_humanize(&asset_token)?;
    let orion_staking = api.addr_humanize(&orion_staking)?;
    let staking_token = api.addr_humanize(&staking_token)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: orion_staking.to_string(),
                amount,
                msg: to_binary(&OrionCw20HookMsg::Bond {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "bond"),
            attr("staking_token", staking_token),
            attr("asset_token", asset_token),
            attr("amount", amount),
        ]))
}

fn unbond_internal(
    deps: DepsMut,
    staker_addr_raw: CanonicalAddr,
    asset_token_raw: CanonicalAddr,
    amount: Uint128,
    lp_balance: Uint128,
    config: &Config,
) -> StdResult<PoolInfo> {
    let mut state = read_state(deps.storage)?;
    let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let mut reward_info =
        rewards_read(deps.storage, &staker_addr_raw).load(asset_token_raw.as_slice())?;

    let user_auto_balance =
        pool_info.calc_user_auto_balance(lp_balance, reward_info.auto_bond_share);
    let user_balance = user_auto_balance;

    if user_balance < amount {
        return Err(StdError::generic_err("Cannot unbond more than bond amount"));
    }

    // distribute reward to pending reward; before changing share
    deposit_spec_reward(deps.as_ref(), &mut state, config, false)?;
    spec_reward_to_pool(&state, &mut pool_info, lp_balance)?;
    before_share_change(&pool_info, &mut reward_info)?;

    // decrease bond amount
    let auto_bond_amount = if reward_info.stake_bond_share.is_zero() {
        amount
    } else {
        amount.multiply_ratio(user_auto_balance, user_balance)
    };

    // add 1 to share, otherwise there will always be a fraction
    let mut auto_bond_share = pool_info.calc_auto_bond_share(auto_bond_amount, lp_balance);
    if pool_info.calc_user_auto_balance(lp_balance, auto_bond_share) < auto_bond_amount {
        auto_bond_share += Uint128::new(1u128);
    }

    pool_info.total_auto_bond_share = pool_info
        .total_auto_bond_share
        .checked_sub(auto_bond_share)?;
    reward_info.auto_bond_share = reward_info.auto_bond_share.checked_sub(auto_bond_share)?;

    // update rewards info
    if reward_info.spec_share.is_zero()
        && reward_info.farm_share.is_zero()
        && reward_info.auto_bond_share.is_zero()
        && reward_info.stake_bond_share.is_zero()
    {
        rewards_store(deps.storage, &staker_addr_raw).remove(asset_token_raw.as_slice());
    } else {
        rewards_store(deps.storage, &staker_addr_raw)
            .save(asset_token_raw.as_slice(), &reward_info)?;
    }

    // update pool info
    pool_info_store(deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    state_store(deps.storage).save(&state)?;

    Ok(pool_info)
}

pub fn unbond(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: String,
    amount: Uint128,
) -> StdResult<Response> {
    let staker_addr_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    let config = read_config(deps.storage)?;

    let lp_balance = query_orion_pool_balance(
        deps.as_ref(),
        &config.orion_staking,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        env.block.time.seconds()
    )?;

    let pool_info = unbond_internal(
        deps.branch(),
        staker_addr_raw,
        asset_token_raw,
        amount,
        lp_balance,
        &config,
    )?;

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.addr_humanize(&config.orion_staking)?.to_string(),
                funds: vec![],
                msg: to_binary(&OrionStakingExecuteMsg::Unbond { amount })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps
                    .api
                    .addr_humanize(&pool_info.staking_token)?
                    .to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount,
                })?,
                funds: vec![],
            }),
        ])
        .add_attributes(vec![
            attr("action", "unbond"),
            attr("staker_addr", info.sender),
            attr("asset_token", asset_token),
            attr("amount", amount),
        ]))
}

pub fn withdraw(
    mut deps: DepsMut,
    info: MessageInfo,
    asset_token: Option<String>,
    spec_amount: Option<Uint128>,
    env: Env
) -> StdResult<Response> {
    let staker_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let asset_token = asset_token.map(|a| deps.api.addr_canonicalize(&a).unwrap());
    let mut state = read_state(deps.storage)?;

    // update pending reward; before withdraw
    let config = read_config(deps.storage)?;
    let spec_staked =
        deposit_spec_reward(deps.as_ref(), &mut state, &config, false)?;

    let (spec_amount, spec_share) = withdraw_reward(
        deps.branch(),
        &config,
        &state,
        &staker_addr,
        &asset_token,
        &spec_staked,
        spec_amount,
        env
    )?;

    state.previous_spec_share = state.previous_spec_share.checked_sub(spec_share)?;

    state_store(deps.storage).save(&state)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    if !spec_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&SpecExecuteMsg::withdraw {
                amount: Some(spec_amount),
                days: None,
            })?,
            funds: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: spec_amount,
            })?,
            funds: vec![],
        }));
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw"),
        attr("spec_amount", spec_amount),
    ]))
}

#[allow(clippy::too_many_arguments)]
fn withdraw_reward(
    deps: DepsMut,
    config: &Config,
    state: &State,
    staker_addr: &CanonicalAddr,
    asset_token: &Option<CanonicalAddr>,
    spec_staked: &SpecBalanceResponse,
    mut request_spec_amount: Option<Uint128>,
    env: Env
) -> StdResult<(Uint128, Uint128)> {
    let rewards_bucket = rewards_read(deps.storage, staker_addr);

    // single reward withdraw; or all rewards
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

    let lp_balance = query_orion_pool_balance(
        deps.as_ref(),
        &config.orion_staking,
        &state.contract_addr,
        env.block.time.seconds()
    )?;

    let mut spec_amount = Uint128::zero();
    let mut spec_share = Uint128::zero();
    for reward_pair in reward_pairs {
        let (asset_token_raw, mut reward_info) = reward_pair;

        // withdraw reward to pending reward
        let key = asset_token_raw.as_slice();
        let mut pool_info = pool_info_read(deps.storage).load(key)?;
        spec_reward_to_pool(state, &mut pool_info, lp_balance)?;
        before_share_change(&pool_info, &mut reward_info)?;

        let (asset_spec_share, asset_spec_amount) = if let Some(request_amount) = request_spec_amount {
            let avail_amount = calc_spec_balance(reward_info.spec_share, spec_staked);
            let asset_spec_amount = if request_amount > avail_amount { avail_amount } else { request_amount };
            let mut asset_spec_share = calc_spec_share(asset_spec_amount, spec_staked);
            if calc_spec_balance(asset_spec_share, spec_staked) < asset_spec_amount {
                asset_spec_share += Uint128::new(1u128);
            }
            request_spec_amount = Some(request_amount.checked_sub(asset_spec_amount)?);
            (asset_spec_share, asset_spec_amount)
        } else {
            (reward_info.spec_share, calc_spec_balance(reward_info.spec_share, spec_staked))
        };
        spec_share += asset_spec_share;
        spec_amount += asset_spec_amount;
        reward_info.spec_share = reward_info.spec_share.checked_sub(asset_spec_share)?;

        // update rewards info
        pool_info_store(deps.storage).save(key, &pool_info)?;
        if reward_info.spec_share.is_zero()
            && reward_info.farm_share.is_zero()
            && reward_info.auto_bond_share.is_zero()
            && reward_info.stake_bond_share.is_zero()
        {
            rewards_store(deps.storage, staker_addr).remove(key);
        } else {
            rewards_store(deps.storage, staker_addr).save(key, &reward_info)?;
        }
    }

    if let Some(request_amount) = request_spec_amount {
        if !request_amount.is_zero() {
            return Err(StdError::generic_err("Cannot withdraw more than remaining amount"));
        }
    }

    Ok((spec_amount, spec_share))
}

fn calc_spec_balance(share: Uint128, staked: &SpecBalanceResponse) -> Uint128 {
    if staked.share.is_zero() {
        Uint128::zero()
    } else {
        share.multiply_ratio(staked.balance, staked.share)
    }
}

fn calc_spec_share(amount: Uint128, stated: &SpecBalanceResponse) -> Uint128 {
    if stated.balance.is_zero() {
        amount
    } else {
        amount.multiply_ratio(stated.share, stated.balance)
    }
}

pub fn query_reward_info(
    deps: Deps,
    staker_addr: String,
    env: Env
) -> StdResult<RewardInfoResponse> {
    let staker_addr_raw = deps.api.addr_canonicalize(&staker_addr)?;
    let mut state = read_state(deps.storage)?;

    let config = read_config(deps.storage)?;
    let spec_staked = deposit_spec_reward(deps, &mut state, &config, true)?;
    let reward_infos = read_reward_infos(
        deps,
        &config,
        &state,
        &staker_addr_raw,
        &spec_staked,
        env
    )?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_infos,
    })
}

fn read_reward_infos(
    deps: Deps,
    config: &Config,
    state: &State,
    staker_addr: &CanonicalAddr,
    spec_staked: &SpecBalanceResponse,
    env: Env
) -> StdResult<Vec<RewardInfoResponseItem>> {
    let rewards_bucket = rewards_read(deps.storage, staker_addr);

    let reward_pair = rewards_bucket
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect::<StdResult<Vec<(CanonicalAddr, RewardInfo)>>>()?;

    let lp_balance =
        query_orion_pool_balance(deps, &config.orion_staking, &state.contract_addr, env.block.time.seconds())?;

    let bucket = pool_info_read(deps.storage);
    let reward_infos: Vec<RewardInfoResponseItem> = reward_pair
        .into_iter()
        .map(|(asset_token_raw, reward_info)| {
            let mut pool_info = bucket.load(asset_token_raw.as_slice())?;

            // update pending rewards
            let mut reward_info = reward_info;
            let farm_share_index = reward_info.farm_share_index;
            let auto_spec_index = reward_info.auto_spec_share_index;
            let stake_spec_index = reward_info.stake_spec_share_index;

            spec_reward_to_pool(state, &mut pool_info, lp_balance)?;
            before_share_change(&pool_info, &mut reward_info)?;

            let auto_bond_amount =
                pool_info.calc_user_auto_balance(lp_balance, reward_info.auto_bond_share);
            Ok(RewardInfoResponseItem {
                asset_token: deps.api.addr_humanize(&asset_token_raw)?.to_string(),
                farm_share_index,
                auto_spec_share_index: auto_spec_index,
                stake_spec_share_index: stake_spec_index,
                bond_amount: auto_bond_amount,
                auto_bond_amount,
                stake_bond_amount: Uint128::zero(),
                farm_share: reward_info.farm_share,
                auto_bond_share: reward_info.auto_bond_share,
                stake_bond_share: reward_info.stake_bond_share,
                spec_share: reward_info.spec_share,
                pending_spec_reward: calc_spec_balance(reward_info.spec_share, spec_staked),
                pending_farm_reward: Uint128::zero()
            })
        })
        .collect::<StdResult<Vec<RewardInfoResponseItem>>>()?;

        Ok(reward_infos)
}
