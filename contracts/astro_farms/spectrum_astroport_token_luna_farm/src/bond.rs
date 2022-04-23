use cosmwasm_std::{attr, to_binary, CanonicalAddr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Order, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery, Api};

use crate::state::{
    pool_info_read, pool_info_store, read_config, read_state, rewards_read, rewards_store,
    state_store, Config, PoolInfo, RewardInfo, State,
};

use cw20::Cw20ExecuteMsg;

use crate::querier::{query_astroport_pool_balance, query_farm_gov_balance};
use astroport::generator::{
    Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg,
};
use spectrum_protocol::astroport_luna_ust_farm::{RewardInfoResponse, RewardInfoResponseItem};
use spectrum_protocol::farm_helper::compute_deposit_time;
use spectrum_protocol::gov::{
    BalanceResponse as SpecBalanceResponse, ExecuteMsg as SpecExecuteMsg, QueryMsg as SpecQueryMsg,
};
use spectrum_protocol::gov_proxy::{ExecuteMsg as GovProxyExecuteMsg};
use spectrum_protocol::math::UDec128;

#[allow(clippy::too_many_arguments)]
fn bond_internal(
    deps: DepsMut,
    env: &Env,
    sender_addr_raw: CanonicalAddr,
    asset_token_raw: CanonicalAddr,
    amount_to_auto: Uint128,
    amount_to_stake: Uint128,
    lp_balance: Uint128,
    config: &Config,
    reallocate: bool,
) -> StdResult<PoolInfo> {
    let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let mut state = read_state(deps.storage)?;

    // update reward index; before changing share
    if !pool_info.total_auto_bond_share.is_zero() || !pool_info.total_stake_bond_share.is_zero() {
        deposit_spec_reward(deps.as_ref(), env, &mut state, config, false)?;
        spec_reward_to_pool(&state, &mut pool_info, lp_balance)?;
    }

    // withdraw reward to pending reward; before changing share
    let mut reward_info = rewards_read(deps.storage, &sender_addr_raw)
        .may_load(asset_token_raw.as_slice())?
        .unwrap_or_else(|| RewardInfo::create(&pool_info));
    before_share_change(&pool_info, &mut reward_info);

    if !reallocate &&
        reward_info.deposit_amount.is_zero() &&
        (!reward_info.auto_bond_share.is_zero() || !reward_info.stake_bond_share.is_zero()) {

        let auto_bond_amount = pool_info.calc_user_auto_balance(lp_balance, reward_info.auto_bond_share);
        let stake_bond_amount = pool_info.calc_user_stake_balance(reward_info.stake_bond_share);
        reward_info.deposit_amount = auto_bond_amount + stake_bond_amount;
        reward_info.deposit_time = env.block.time.seconds();
    }

    // increase bond_amount
    let deposit_fee = if reallocate || sender_addr_raw == config.controller { Decimal::zero() } else { config.deposit_fee };
    let deposit_fee_auto = amount_to_auto * deposit_fee;
    let deposit_fee_stake = amount_to_stake * deposit_fee;
    let auto_bond_amount = amount_to_auto.checked_sub(deposit_fee_auto)?;
    let stake_bond_amount = amount_to_stake.checked_sub(deposit_fee_stake)?;
    let new_deposit_amount = increase_bond_amount(
        &mut pool_info,
        &mut reward_info,
        auto_bond_amount,
        stake_bond_amount,
        lp_balance,
    );

    let earned_deposit_fee = deposit_fee_auto + deposit_fee_stake;
    if !earned_deposit_fee.is_zero() {
        let mut ctrl_reward_info = rewards_read(deps.storage, &config.controller)
            .may_load(asset_token_raw.as_slice())?
            .unwrap_or_else(|| RewardInfo::create(&pool_info));
        increase_bond_amount(
            &mut pool_info,
            &mut ctrl_reward_info,
            earned_deposit_fee,
            Uint128::zero(),
            lp_balance + auto_bond_amount + stake_bond_amount,
        );
        rewards_store(deps.storage, &config.controller)
            .save(asset_token_raw.as_slice(), &ctrl_reward_info)?;
    }

    if !reallocate {
        let last_deposit_amount = reward_info.deposit_amount;
        reward_info.deposit_amount = last_deposit_amount + new_deposit_amount;
        reward_info.deposit_time = compute_deposit_time(
            last_deposit_amount,
            new_deposit_amount,
            reward_info.deposit_time,
            env.block.time.seconds(),
        )?;
    }

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
    let staker_addr_raw = deps.api.addr_canonicalize(&sender_addr)?;
    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    let pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;

    // only staking token contract can execute this message
    if pool_info.staking_token != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;

    let compound_rate = compound_rate.unwrap_or_else(Decimal::zero);

    let amount_to_auto = amount * compound_rate;
    let amount_to_stake = amount.checked_sub(amount_to_auto)?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    bond_internal(
        deps.branch(),
        &env,
        staker_addr_raw,
        asset_token_raw,
        amount_to_auto,
        amount_to_stake,
        lp_balance,
        &config,
        false,
    )?;

    stake_token(
        deps.api,
        config.astroport_generator,
        pool_info.staking_token,
        amount,
    )
}

pub fn deposit_farm_share(
    deps: Deps,
    env: &Env,
    state: &mut State,
    pool_info: &mut PoolInfo,
    config: &Config,
    amount: Uint128,    // ASTRO
) -> StdResult<()> {
    let staked = query_farm_gov_balance(
        deps,
        &config.xastro_proxy,
        &env.contract.address,
    )?;
    let mut new_total_share = Uint128::zero();
    if !pool_info.total_stake_bond_share.is_zero() {
        let new_share = state.calc_farm_share(amount, staked.balance);
        let share_per_bond = Decimal::from_ratio(new_share, pool_info.total_stake_bond_share);
        pool_info.farm_share_index = pool_info.farm_share_index + share_per_bond;
        pool_info.farm_share += new_share;
        new_total_share += new_share;
    }

    state.total_farm_share += new_total_share;

    Ok(())
}

pub fn deposit_spec_reward(
    deps: Deps,
    env: &Env,
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
                address: env.contract.address.to_string(),
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
fn before_share_change(pool_info: &PoolInfo, reward_info: &mut RewardInfo) {
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
}

// increase share amount in pool and reward info
fn increase_bond_amount(
    pool_info: &mut PoolInfo,
    reward_info: &mut RewardInfo,
    auto_bond_amount: Uint128,
    stake_bond_amount: Uint128,
    lp_balance: Uint128,
) -> Uint128 {

    // convert amount to share & update
    let auto_bond_share = pool_info.calc_auto_bond_share(auto_bond_amount, lp_balance);
    let stake_bond_share = pool_info.calc_stake_bond_share(stake_bond_amount);
    pool_info.total_auto_bond_share += auto_bond_share;
    pool_info.total_stake_bond_amount += stake_bond_amount;
    pool_info.total_stake_bond_share += stake_bond_share;
    reward_info.auto_bond_share += auto_bond_share;
    reward_info.stake_bond_share += stake_bond_share;

    let new_auto_bond_amount = pool_info.calc_user_auto_balance(lp_balance + auto_bond_amount + stake_bond_amount, auto_bond_share);
    let new_stake_bond_amount = pool_info.calc_user_stake_balance(stake_bond_share);

    new_auto_bond_amount + new_stake_bond_amount
}

// stake LP token to Astroport Generator
fn stake_token(
    api: &dyn Api,
    astroport_generator: CanonicalAddr,
    staking_token: CanonicalAddr,
    amount: Uint128,
) -> StdResult<Response> {
    let astroport_generator = api.addr_humanize(&astroport_generator)?;
    let staking_token = api.addr_humanize(&staking_token)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_generator.to_string(),
                amount,
                msg: to_binary(&AstroportCw20HookMsg::Deposit {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "bond"),
            attr("lp_token", staking_token),
            attr("amount", amount),
        ]))
}

#[allow(clippy::too_many_arguments)]
fn unbond_internal(
    deps: DepsMut,
    env: &Env,
    staker_addr_raw: CanonicalAddr,
    asset_token_raw: CanonicalAddr,
    amount: Uint128,
    lp_balance: Uint128,
    config: &Config,
    reallocate: bool,
) -> StdResult<PoolInfo> {
    let mut state = read_state(deps.storage)?;
    let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let mut reward_info =
        rewards_read(deps.storage, &staker_addr_raw).load(asset_token_raw.as_slice())?;

    let user_auto_balance =
        pool_info.calc_user_auto_balance(lp_balance, reward_info.auto_bond_share);
    let user_stake_balance = pool_info.calc_user_stake_balance(reward_info.stake_bond_share);
    let user_balance = user_auto_balance + user_stake_balance;

    if user_balance < amount {
        return Err(StdError::generic_err("Cannot unbond more than bond amount"));
    }

    // distribute reward to pending reward; before changing share
    deposit_spec_reward(deps.as_ref(), env, &mut state, config, false)?;
    spec_reward_to_pool(&state, &mut pool_info, lp_balance)?;
    before_share_change(&pool_info, &mut reward_info);

    // decrease bond amount
    let auto_bond_amount = if reward_info.stake_bond_share.is_zero() {
        amount
    } else {
        amount.multiply_ratio(user_auto_balance, user_balance)
    };
    let stake_bond_amount = amount.checked_sub(auto_bond_amount)?;

    // add 1 to share, otherwise there will always be a fraction
    let mut auto_bond_share = pool_info.calc_auto_bond_share(auto_bond_amount, lp_balance);
    if pool_info.calc_user_auto_balance(lp_balance, auto_bond_share) < auto_bond_amount {
        auto_bond_share += Uint128::new(1u128);
    }
    let mut stake_bond_share = pool_info.calc_stake_bond_share(stake_bond_amount);
    if pool_info.calc_user_stake_balance(stake_bond_share) < stake_bond_amount {
        stake_bond_share += Uint128::new(1u128);
    }

    pool_info.total_auto_bond_share = pool_info
        .total_auto_bond_share
        .checked_sub(auto_bond_share)?;
    pool_info.total_stake_bond_amount = pool_info
        .total_stake_bond_amount
        .checked_sub(stake_bond_amount)?;
    pool_info.total_stake_bond_share = pool_info
        .total_stake_bond_share
        .checked_sub(stake_bond_share)?;
    reward_info.auto_bond_share = reward_info.auto_bond_share.checked_sub(auto_bond_share)?;
    reward_info.stake_bond_share = reward_info.stake_bond_share.checked_sub(stake_bond_share)?;

    if !reallocate {
        reward_info.deposit_amount = reward_info
            .deposit_amount
            .multiply_ratio(user_balance.checked_sub(amount)?, user_balance);
    }

    // update rewards info
    if reward_info.spec_share.is_zero()
        && reward_info.farm_share.is_zero()
        && reward_info.auto_bond_share.is_zero()
        && reward_info.stake_bond_share.is_zero()
        && !reallocate
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
    let pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let pool_info = unbond_internal(
        deps.branch(),
        &env,
        staker_addr_raw,
        asset_token_raw,
        amount,
        lp_balance,
        &config,
        false,
    )?;

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps
                    .api
                    .addr_humanize(&config.astroport_generator)?
                    .to_string(),
                funds: vec![],
                msg: to_binary(&AstroportExecuteMsg::Withdraw {
                    lp_token: deps.api.addr_humanize(&pool_info.staking_token)?,
                    amount,
                })?,
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

pub fn update_bond(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: String,
    amount_to_auto: Uint128,
    amount_to_stake: Uint128,
) -> StdResult<Response> {

    let config = read_config(deps.storage)?;

    let staker_addr_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    let amount = amount_to_auto + amount_to_stake;
    let pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    unbond_internal(
        deps.branch(),
        &env,
        staker_addr_raw.clone(),
        asset_token_raw.clone(),
        amount,
        lp_balance,
        &config,
        true,
    )?;

    bond_internal(
        deps,
        &env,
        staker_addr_raw,
        asset_token_raw,
        amount_to_auto,
        amount_to_stake,
        lp_balance.checked_sub(amount)?,
        &config,
        true,
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_bond"),
        attr("asset_token", asset_token),
        attr("amount_to_auto", amount_to_auto),
        attr("amount_to_stake", amount_to_stake),
    ]))
}

pub fn withdraw(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: Option<String>,
    spec_amount: Option<Uint128>,
    farm_amount: Option<Uint128>,
) -> StdResult<Response> {
    let staker_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let asset_token = asset_token.map(|a| deps.api.addr_canonicalize(&a).unwrap());
    let mut state = read_state(deps.storage)?;

    // update pending reward; before withdraw
    let config = read_config(deps.storage)?;
    let spec_staked = deposit_spec_reward(deps.as_ref(), &env, &mut state, &config, false)?;

    let (spec_amount, spec_share,
        farm_amount, farm_share,
    ) = withdraw_reward(
        deps.branch(),
        &env,
        &config,
        &state,
        &staker_addr,
        &asset_token,
        &spec_staked,
        spec_amount,
        farm_amount,
    )?;

    state.previous_spec_share = state.previous_spec_share.checked_sub(spec_share)?;
    state.total_farm_share = state.total_farm_share.checked_sub(farm_share)?;

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

    if !farm_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps
                .api
                .addr_humanize(&config.xastro_proxy)?
                .to_string(),
            msg: to_binary(&GovProxyExecuteMsg::Unstake {
                amount: Some(farm_amount),
            })?,
            funds: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.astro_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: farm_amount,
            })?,
            funds: vec![],
        }));
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw"),
        attr("farm_amount", farm_amount),
        attr("spec_amount", spec_amount),
    ]))
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::needless_late_init)]
fn withdraw_reward(
    deps: DepsMut,
    env: &Env,
    config: &Config,
    state: &State,
    staker_addr: &CanonicalAddr,
    asset_token: &Option<CanonicalAddr>,
    spec_staked: &SpecBalanceResponse,
    mut request_spec_amount: Option<Uint128>,
    mut request_farm_amount: Option<Uint128>,
) -> StdResult<(Uint128, Uint128, Uint128, Uint128)> {
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

    let farm_staked = query_farm_gov_balance(
        deps.as_ref(),
        &config.xastro_proxy,
        &env.contract.address,
    )?;

    let mut spec_amount = Uint128::zero();
    let mut spec_share = Uint128::zero();
    let mut farm_amount = Uint128::zero();
    let mut farm_share = Uint128::zero();
    for reward_pair in reward_pairs {
        let (asset_token_raw, mut reward_info) = reward_pair;

        // withdraw reward to pending reward
        let key = asset_token_raw.as_slice();
        let mut pool_info = pool_info_read(deps.storage).load(key)?;
        let lp_balance = query_astroport_pool_balance(
            deps.as_ref(),
            &pool_info.staking_token,
            &env.contract.address,
            &config.astroport_generator,
        )?;

        spec_reward_to_pool(state, &mut pool_info, lp_balance)?;
        before_share_change(&pool_info, &mut reward_info);

        // update withdraw
        let (asset_farm_share, asset_farm_amount) = if let Some(request_amount) = request_farm_amount {
            let avail_amount = calc_farm_balance(reward_info.farm_share, farm_staked.balance, state.total_farm_share);
            let asset_farm_amount = if request_amount > avail_amount { avail_amount } else { request_amount };
            let mut asset_farm_share = calc_farm_share(asset_farm_amount, farm_staked.balance, state.total_farm_share);
            if calc_farm_balance(asset_farm_share, farm_staked.balance, state.total_farm_share) < asset_farm_amount {
                asset_farm_share += Uint128::new(1u128);
            }
            request_farm_amount = Some(request_amount.checked_sub(asset_farm_amount)?);
            (asset_farm_share, asset_farm_amount)
        } else {
            (reward_info.farm_share, calc_farm_balance(
                reward_info.farm_share,
                farm_staked.balance,
                state.total_farm_share,
            ))
        };
        farm_share += asset_farm_share;
        farm_amount += asset_farm_amount;

        let (asset_spec_share, asset_spec_amount) =
            if let Some(request_amount) = request_spec_amount {
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
        pool_info.farm_share = pool_info.farm_share.checked_sub(asset_farm_share)?;
        reward_info.farm_share = reward_info.farm_share.checked_sub(asset_farm_share)?;
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

    if let Some(request_amount) = request_farm_amount {
        if !request_amount.is_zero() {
            return Err(StdError::generic_err(
                "Cannot withdraw farm_amount more than remaining amount",
            ));
        }
    }
    if let Some(request_amount) = request_spec_amount {
        if !request_amount.is_zero() {
            return Err(StdError::generic_err(
                "Cannot withdraw more than remaining amount",
            ));
        }
    }

    Ok((spec_amount, spec_share, farm_amount, farm_share))
}

fn calc_farm_balance(share: Uint128, total_balance: Uint128, total_farm_share: Uint128) -> Uint128 {
    if total_farm_share.is_zero() {
        Uint128::zero()
    } else {
        total_balance.multiply_ratio(share, total_farm_share)
    }
}

fn calc_farm_share(amount: Uint128, total_balance: Uint128, total_farm_share: Uint128) -> Uint128 {
    if total_balance.is_zero() {
        amount
    } else {
        amount.multiply_ratio(total_farm_share, total_balance)
    }
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
    env: Env,
    staker_addr: String,
) -> StdResult<RewardInfoResponse> {
    let staker_addr_raw = deps.api.addr_canonicalize(&staker_addr)?;
    let mut state = read_state(deps.storage)?;

    let config = read_config(deps.storage)?;
    let spec_staked = deposit_spec_reward(deps, &env, &mut state, &config, true)?;
    let reward_infos =
        read_reward_infos(deps, env, &config, &state, &staker_addr_raw, &spec_staked)?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_infos,
    })
}

fn read_reward_infos(
    deps: Deps,
    env: Env,
    config: &Config,
    state: &State,
    staker_addr: &CanonicalAddr,
    spec_staked: &SpecBalanceResponse,
) -> StdResult<Vec<RewardInfoResponseItem>> {
    let rewards_bucket = rewards_read(deps.storage, staker_addr);

    let reward_pair = rewards_bucket
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect::<StdResult<Vec<(CanonicalAddr, RewardInfo)>>>()?;

    let farm_staked = query_farm_gov_balance(
        deps,
        &config.xastro_proxy,
        &env.contract.address,
    )?;

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

            let has_deposit_amount = !reward_info.deposit_amount.is_zero();

            let lp_balance = query_astroport_pool_balance(
                deps,
                &pool_info.staking_token,
                &env.contract.address,
                &config.astroport_generator,
            )?;

            spec_reward_to_pool(state, &mut pool_info, lp_balance)?;
            before_share_change(&pool_info, &mut reward_info);

            let auto_bond_amount =
                pool_info.calc_user_auto_balance(lp_balance, reward_info.auto_bond_share);
            let stake_bond_amount = pool_info.calc_user_stake_balance(reward_info.stake_bond_share);
            Ok(RewardInfoResponseItem {
                asset_token: deps.api.addr_humanize(&asset_token_raw)?.to_string(),
                farm_share_index,
                auto_spec_share_index: auto_spec_index,
                stake_spec_share_index: stake_spec_index,
                bond_amount: auto_bond_amount + stake_bond_amount,
                auto_bond_amount,
                stake_bond_amount,
                farm_share: reward_info.farm_share,
                auto_bond_share: reward_info.auto_bond_share,
                stake_bond_share: reward_info.stake_bond_share,
                spec_share: reward_info.spec_share,
                pending_spec_reward: calc_spec_balance(reward_info.spec_share, spec_staked),
                pending_farm_reward: calc_farm_balance(
                    reward_info.farm_share,
                    farm_staked.balance,
                    state.total_farm_share,
                ),
                deposit_amount: if has_deposit_amount {
                    Some(reward_info.deposit_amount)
                } else {
                    None
                },
                deposit_time: if has_deposit_amount {
                    Some(reward_info.deposit_time)
                } else {
                    None
                },
            })
        })
        .collect::<StdResult<Vec<RewardInfoResponseItem>>>()?;

    Ok(reward_infos)
}
