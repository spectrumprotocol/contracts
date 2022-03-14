use cosmwasm_std::{attr, to_binary, CanonicalAddr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery, Order, QuerierWrapper, Addr, BankMsg, Coin};

use crate::state::{read_config, read_state, rewards_read, rewards_store, state_store, Config, RewardInfo, State, Unbonding, user_unbonding_store};

use cw20::Cw20ExecuteMsg;

use spectrum_protocol::farm_helper::compute_deposit_time;
use spectrum_protocol::gov::{
    BalanceResponse as SpecBalanceResponse, ExecuteMsg as SpecExecuteMsg, QueryMsg as SpecQueryMsg,
};
use crate::model::{RewardInfoResponse, RewardInfoResponseItem};

#[allow(clippy::too_many_arguments)]
fn bond_internal(
    deps: DepsMut,
    env: Env,
    sender_addr_raw: CanonicalAddr,
    amount: Uint128,
    config: &Config,
) -> StdResult<()> {
    let mut state = read_state(deps.storage)?;

    // update reward index; before changing share
    deposit_spec_reward(deps.as_ref(), &env, &mut state, config, false)?;

    // withdraw reward to pending reward; before changing share
    let mut reward_info = rewards_read(deps.storage, sender_addr_raw.as_slice())?
        .unwrap_or_else(|| RewardInfo::create(&state));
    before_share_change(&state, &mut reward_info);

    // increase bond_amount
    let deposit_fee = if sender_addr_raw == config.controller { Decimal::zero() } else { config.deposit_fee };
    let earned_deposit_fee = amount * deposit_fee;
    let bond_amount = amount.checked_sub(earned_deposit_fee)?;
    increase_bond_amount(
        &mut state,
        &mut reward_info,
        bond_amount,
    );

    if !earned_deposit_fee.is_zero() {
        let mut ctrl_reward_info = rewards_read(deps.storage, &config.controller)?
            .unwrap_or_else(|| RewardInfo::create(&state));
        increase_bond_amount(
            &mut state,
            &mut ctrl_reward_info,
            earned_deposit_fee,
        );
        rewards_store(deps.storage)
            .save(config.controller.as_slice(), &ctrl_reward_info)?;
    }

    let last_deposit_amount = reward_info.deposit_amount;
    reward_info.deposit_amount = last_deposit_amount + bond_amount;
    reward_info.deposit_time = compute_deposit_time(last_deposit_amount, bond_amount, reward_info.deposit_time, env.block.time.seconds())?;

    rewards_store(deps.storage)
        .save(sender_addr_raw.as_slice(), &reward_info)?;
    state_store(deps.storage).save(&state)?;

    Ok(())
}

pub fn bond(
    deps: DepsMut,
    env: Env,
    sender_addr: String,
    amount: Uint128,
) -> StdResult<Response> {

    let sender_addr_raw = deps.api.addr_canonicalize(&sender_addr)?;
    let config = read_config(deps.storage)?;
    bond_internal(
        deps,
        env,
        sender_addr_raw,
        amount,
        &config,
    )?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("action", "bond"),
            attr("amount", amount),
    ]))
}

pub fn deposit_spec_reward(
    deps: Deps,
    env: &Env,
    state: &mut State,
    config: &Config,
    query: bool,
) -> StdResult<SpecBalanceResponse> {
    if state.total_bond_share.is_zero() {
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
    let deposit_per_share = Decimal::from_ratio(deposit_share, state.total_bond_share);
    state.spec_share_index = state.spec_share_index + deposit_per_share;
    state.previous_spec_share = staked.share;

    Ok(staked)
}

// withdraw reward to pending reward
fn before_share_change(state: &State, reward_info: &mut RewardInfo) {
    let spec_share = reward_info.bond_share * (state.spec_share_index - reward_info.spec_share_index);
    reward_info.spec_share += spec_share;
    reward_info.spec_share_index = state.spec_share_index;
}

// increase share amount in pool and reward info
fn increase_bond_amount(
    state: &mut State,
    reward_info: &mut RewardInfo,
    bond_amount: Uint128,
) {

    // convert amount to share & update
    let bond_share = state.calc_bond_share(bond_amount);
    state.total_bond_share += bond_share;
    state.total_bond_amount += bond_amount;
    reward_info.bond_share += bond_share;
}

#[allow(clippy::too_many_arguments)]
fn unbond_internal(
    deps: DepsMut,
    env: Env,
    staker_addr_raw: CanonicalAddr,
    amount: Uint128,
    config: &Config,
) -> StdResult<()> {
    let mut state = read_state(deps.storage)?;
    let mut reward_info = rewards_read(deps.storage, &staker_addr_raw)?
        .expect("not found");

    let user_balance = state.calc_bond_amount(reward_info.bond_share);
    if user_balance < amount {
        return Err(StdError::generic_err("Cannot unbond more than bond amount"));
    }

    // distribute reward to pending reward; before changing share
    deposit_spec_reward(deps.as_ref(), &env, &mut state, config, false)?;
    before_share_change(&state, &mut reward_info);

    // add 1 to share, otherwise there will always be a fraction
    let mut bond_share = state.calc_bond_share(amount);
    if state.calc_bond_amount(bond_share) < amount {
        bond_share += Uint128::new(1u128);
    }

    state.total_bond_share = state.total_bond_share.checked_sub(bond_share)?;
    state.total_bond_amount = state.total_bond_amount.checked_sub(amount)?;
    state.unbonding_amount += amount;
    reward_info.bond_share = reward_info.bond_share.checked_sub(bond_share)?;
    reward_info.deposit_amount = reward_info.deposit_amount.multiply_ratio(user_balance.checked_sub(amount)?, user_balance);
    reward_info.unbonding_amount += amount;

    let unbond = Unbonding::create(&mut state, env.block.time.seconds(), amount);

    rewards_store(deps.storage).save(staker_addr_raw.as_slice(), &reward_info)?;
    state_store(deps.storage).save(&state)?;
    user_unbonding_store(deps.storage, &staker_addr_raw).save(&unbond.id.to_be_bytes(), &unbond)?;

    Ok(())
}

pub fn unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> StdResult<Response> {
    let staker_addr_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let config = read_config(deps.storage)?;

    let unbond_count = user_unbonding_store(deps.storage, &staker_addr_raw)
        .range(None, None, Order::Ascending)
        .count();
    if unbond_count >= config.max_unbond_count {
        return Err(StdError::generic_err("max unbond count reach"));
    }

    unbond_internal(
        deps,
        env,
        staker_addr_raw,
        amount,
        &config,
    )?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("action", "unbond"),
            attr("staker_addr", info.sender),
            attr("amount", amount),
        ]))
}

pub fn claim_unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> StdResult<Response> {

    let mut state = read_state(deps.storage)?;
    update_claimable(&deps.querier, &env.contract.address, &mut state)?;

    let staker_addr_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;
    let mut reward_info = rewards_read(deps.storage, staker_addr_raw.as_slice())?
        .expect("not found");
    let unbondings = user_unbonding_store(deps.storage, &staker_addr_raw)
        .range(None, None, Order::Ascending)
        .map(|it| {
            let (_, unbonding) = it?;
            Ok(unbonding)
        })
        .collect::<StdResult<Vec<Unbonding>>>()?;

    let mut claimable_amount = Uint128::zero();
    for unbonding in unbondings {
        if unbonding.unbonding_index <= state.unbonded_index {
            claimable_amount += unbonding.amount;
            user_unbonding_store(deps.storage, &staker_addr_raw)
                .remove(&unbonding.id.to_be_bytes());
        }
    }

    state.claimable_amount = state.claimable_amount.checked_sub(claimable_amount)?;
    reward_info.unbonding_amount = reward_info.unbonding_amount.checked_sub(claimable_amount)?;

    rewards_store(deps.storage).save(staker_addr_raw.as_slice(), &reward_info)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_message(CosmosMsg::Bank(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![
                Coin { denom: "uluna".to_string(), amount: claimable_amount },
            ],
        }))
        .add_attributes(vec![
            attr("action", "claim_unbond"),
            attr("amount", claimable_amount),
        ]))
}

fn update_claimable(
    querier: &QuerierWrapper,
    contract_addr: &Addr,
    state: &mut State,
) -> StdResult<()> {
    let balance = querier.query_balance(contract_addr, "uluna")?;
    let tobe_claimed = balance.amount.checked_sub(state.claimable_amount)?;
    let new_claimable_amount = if tobe_claimed > state.unbonding_amount {
        state.unbonding_amount
    } else {
        tobe_claimed
    };
    state.unbonding_amount = state.unbonding_amount.checked_sub(new_claimable_amount)?;
    state.claimable_amount += new_claimable_amount;
    state.unbonded_index += new_claimable_amount;

    Ok(())
}

pub fn withdraw(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    spec_amount: Option<Uint128>,
) -> StdResult<Response> {
    let staker_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let mut state = read_state(deps.storage)?;

    // update pending reward; before withdraw
    let config = read_config(deps.storage)?;
    let spec_staked = deposit_spec_reward(deps.as_ref(), &env, &mut state, &config, false)?;

    let (spec_amount, spec_share) = withdraw_reward(
        deps.branch(),
        &state,
        &staker_addr,
        &spec_staked,
        spec_amount,
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
    state: &State,
    staker_addr: &CanonicalAddr,
    spec_staked: &SpecBalanceResponse,
    mut request_spec_amount: Option<Uint128>,
) -> StdResult<(Uint128, Uint128)> {
    let mut reward_info = rewards_read(deps.storage, staker_addr)?
        .expect("not found");

    let mut spec_amount = Uint128::zero();
    let mut spec_share = Uint128::zero();

    // withdraw reward to pending reward
    before_share_change(state, &mut reward_info);

    // update withdraw
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
    if reward_info.spec_share.is_zero()
        && reward_info.bond_share.is_zero()
        && reward_info.unbonding_amount.is_zero()
    {
        rewards_store(deps.storage).remove(staker_addr.as_slice());
    } else {
        rewards_store(deps.storage).save(staker_addr.as_slice(), &reward_info)?;
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
    env: Env,
    staker_addr: String,
) -> StdResult<RewardInfoResponse> {
    let staker_addr_raw = deps.api.addr_canonicalize(&staker_addr)?;
    let mut state = read_state(deps.storage)?;

    let config = read_config(deps.storage)?;
    let spec_staked = deposit_spec_reward(deps, &env, &mut state, &config, true)?;
    let reward_infos = read_reward_infos(
        deps,
        &state,
        &staker_addr_raw,
        &spec_staked,
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
    spec_staked: &SpecBalanceResponse,
) -> StdResult<Vec<RewardInfoResponseItem>> {
    let reward_info = rewards_read(deps.storage, staker_addr)?;
    match reward_info {
        None => Ok(vec![]),
        Some(reward_info) => {

            // update pending rewards
            let mut reward_info = reward_info;
            let spec_share_index = reward_info.spec_share_index;
            before_share_change(state, &mut reward_info);

            let bond_amount = state.calc_bond_amount(reward_info.bond_share);
            Ok(vec![RewardInfoResponseItem {
                asset_token: "uluna".to_string(),
                spec_share_index,
                bond_amount,
                bond_share: reward_info.bond_share,
                spec_share: reward_info.spec_share,
                pending_spec_reward: calc_spec_balance(reward_info.spec_share, spec_staked),
                deposit_amount: reward_info.deposit_amount,
                deposit_time: reward_info.deposit_time,
                unbonding_amount: reward_info.unbonding_amount,
            }])
        }
    }
}
