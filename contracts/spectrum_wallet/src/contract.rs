#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    attr, from_binary, to_binary, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::state::{
    config_store, read_config, read_reward, read_rewards, read_state, reward_store, state_store,
    Config, RewardInfo, State,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::gov::{
    BalanceResponse as GovBalanceResponse, Cw20HookMsg as GovCw20HookMsg,
    ExecuteMsg as GovExecuteMsg, QueryMsg as GovQueryMsg, StateInfo as GovStateInfo, VoteOption,
};
use spectrum_protocol::wallet::{
    BalanceResponse, ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg, ShareInfo,
    SharesResponse, StateInfo,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    config_store(deps.storage).save(&Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        spectrum_token: deps.api.addr_canonicalize(&msg.spectrum_token)?,
        spectrum_gov: deps.api.addr_canonicalize(&msg.spectrum_gov)?,
    })?;

    state_store(deps.storage).save(&State {
        contract_addr: deps.api.addr_canonicalize(env.contract.address.as_str())?,
        previous_share: Uint128::zero(),
        share_index: Decimal::zero(),
        total_weight: 0u32,
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::poll_vote {
            poll_id,
            vote,
            amount,
        } => poll_vote(deps, env, info, poll_id, vote, amount),
        ExecuteMsg::receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::stake { amount } => stake(deps, env, info, amount),
        ExecuteMsg::unstake { amount } => unstake(deps, env, info, amount),
        ExecuteMsg::upsert_share {
            address,
            weight,
            lock_start,
            lock_end,
            lock_amount,
        } => upsert_share(
            deps,
            info,
            env,
            address,
            weight,
            lock_start,
            lock_end,
            lock_amount,
        ),
        ExecuteMsg::update_config { owner } => update_config(deps, env, info, owner),
        ExecuteMsg::withdraw { amount } => withdraw(deps, env, info, amount),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    // only asset contract can execute this message
    let config = read_config(deps.storage)?;
    if config.spectrum_token != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::deposit {}) => deposit(deps, env, cw20_msg.sender, cw20_msg.amount),
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

fn deposit(deps: DepsMut, _env: Env, sender: String, amount: Uint128) -> StdResult<Response> {
    let staker_addr = deps.api.addr_canonicalize(&sender)?;
    let mut reward_info = read_reward(deps.storage, &staker_addr)?;
    reward_info.amount += amount;
    reward_store(deps.storage).save(staker_addr.as_slice(), &reward_info)?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "dep"),
        attr("amount", amount.to_string()),
    ]))
}

fn poll_vote(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    poll_id: u64,
    vote: VoteOption,
    amount: Uint128,
) -> StdResult<Response> {
    // anyone in shared wallet can vote
    let shares = read_rewards(deps.storage)?;
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = shares.into_iter().any(|(key, _)| key == sender_addr);
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;

    Ok(
        Response::new().add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&GovExecuteMsg::poll_vote {
                poll_id,
                vote,
                amount,
            })?,
            funds: vec![],
        })]),
    )
}

fn stake(deps: DepsMut, env: Env, info: MessageInfo, amount: Uint128) -> StdResult<Response> {
    // record reward before any share change
    let mut state = read_state(deps.storage)?;
    let config = read_config(deps.storage)?;
    deposit_reward(deps.as_ref(), &mut state, &config, env.block.height, false)?;

    let staker_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let mut reward_info = read_reward(deps.storage, &staker_addr)?;
    before_share_change(&state, &mut reward_info)?;

    // calculate new stake share
    let gov_state: GovStateInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        msg: to_binary(&GovQueryMsg::state {
            height: env.block.height,
        })?,
    }))?;
    let new_share = amount.multiply_ratio(gov_state.total_share, gov_state.total_staked);

    // move from amount to staked share
    reward_info.amount = reward_info.amount.checked_sub(amount)?;
    reward_info.share += new_share;
    reward_store(deps.storage).save(staker_addr.as_slice(), &reward_info)?;

    state.previous_share += new_share;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
                amount,
                msg: to_binary(&GovCw20HookMsg::stake_tokens { staker_addr: None })?,
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("amount", amount.to_string()),
        ]))
}

fn unstake(deps: DepsMut, env: Env, info: MessageInfo, amount: Uint128) -> StdResult<Response> {
    // record reward before any share change
    let mut state = read_state(deps.storage)?;
    let config = read_config(deps.storage)?;
    let staked = deposit_reward(deps.as_ref(), &mut state, &config, env.block.height, false)?;

    let staker_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let mut reward_info = read_reward(deps.storage, &staker_addr)?;
    before_share_change(&state, &mut reward_info)?;

    let share = amount.multiply_ratio(staked.share, staked.balance);
    reward_info.share = reward_info.share.checked_sub(share)?;
    reward_info.amount += amount;
    reward_store(deps.storage).save(staker_addr.as_slice(), &reward_info)?;

    state.previous_share = state.previous_share.checked_sub(share)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&GovExecuteMsg::withdraw {
                amount: Some(amount),
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            attr("action", "unstake"),
            attr("amount", amount.to_string()),
        ]))
}

fn withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> StdResult<Response> {
    // record reward before any share change
    let mut state = read_state(deps.storage)?;
    let config = read_config(deps.storage)?;
    let staked = deposit_reward(deps.as_ref(), &mut state, &config, env.block.height, false)?;

    let staker_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let mut reward_info = read_reward(deps.storage, &staker_addr)?;
    before_share_change(&state, &mut reward_info)?;

    let staked_amount = calc_balance(reward_info.share, &staked);
    let total_amount = staked_amount + reward_info.amount;
    let locked_amount = reward_info.calc_locked_amount(env.block.height);
    let withdrawable = total_amount.checked_sub(locked_amount)?;
    let withdraw_amount = if let Some(amount) = amount {
        if amount > withdrawable {
            return Err(StdError::generic_err("not enough amount to withdraw"));
        }
        amount
    } else {
        withdrawable
    };

    reward_info.amount = reward_info.amount.checked_sub(withdraw_amount)?;
    reward_store(deps.storage).save(staker_addr.as_slice(), &reward_info)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: withdraw_amount,
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            attr("action", "withdraw"),
            attr("amount", withdraw_amount.to_string()),
        ]))
}

fn deposit_reward(
    deps: Deps,
    state: &mut State,
    config: &Config,
    height: u64,
    query: bool,
) -> StdResult<GovBalanceResponse> {
    if state.total_weight == 0u32 {
        return Ok(GovBalanceResponse {
            share: Uint128::zero(),
            balance: Uint128::zero(),
            locked_balance: vec![],
        });
    }

    let staked: GovBalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        msg: to_binary(&GovQueryMsg::balance {
            address: deps.api.addr_humanize(&state.contract_addr)?.to_string(),
            height: Some(height),
        })?,
    }))?;
    let diff = staked.share.checked_sub(state.previous_share);
    let deposit_share = if query {
        diff.unwrap_or_else(|_| Uint128::zero())
    } else {
        diff?
    };
    let share_per_weight = Decimal::from_ratio(deposit_share, state.total_weight);
    state.share_index = state.share_index + share_per_weight;
    state.previous_share = staked.share;

    Ok(staked)
}

fn before_share_change(state: &State, reward_info: &mut RewardInfo) -> StdResult<()> {
    let share =
        Uint128::from(reward_info.weight as u128) * (state.share_index - reward_info.share_index);
    reward_info.share += share;
    reward_info.share_index = state.share_index;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_share(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    address: String,
    weight: u32,
    lock_start: Option<u64>,
    lock_end: Option<u64>,
    lock_amount: Option<Uint128>,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    let mut state = state_store(deps.storage).load()?;
    deposit_reward(deps.as_ref(), &mut state, &config, env.block.height, false)?;

    let address_raw = deps.api.addr_canonicalize(&address)?;
    let key = address_raw.as_slice();
    let mut reward_info = reward_store(deps.storage)
        .may_load(key)?
        .unwrap_or_default();

    state.total_weight = state.total_weight + weight - reward_info.weight;
    reward_info.weight = weight;
    reward_info.lock_start = lock_start.unwrap_or(0u64);
    reward_info.lock_end = lock_end.unwrap_or(0u64);
    reward_info.lock_amount = lock_amount.unwrap_or_else(Uint128::zero);

    state_store(deps.storage).save(&state)?;

    if weight == 0 {
        reward_store(deps.storage).remove(key);
    } else {
        reward_store(deps.storage).save(key, &reward_info)?;
    }
    Ok(Response::new().add_attributes(vec![attr(
        "new_total_weight",
        state.total_weight.to_string(),
    )]))
}

fn update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    config_store(deps.storage).save(&config)?;
    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::balance { address, height } => to_binary(&query_balance(deps, address, height)?),
        QueryMsg::shares {} => to_binary(&query_shares(deps)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        spectrum_token: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
        spectrum_gov: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
    };

    Ok(resp)
}

pub fn query_balance(deps: Deps, staker_addr: String, height: u64) -> StdResult<BalanceResponse> {
    let staker_addr_raw = deps.api.addr_canonicalize(&staker_addr)?;
    let mut state = read_state(deps.storage)?;

    let config = read_config(deps.storage)?;
    let staked = deposit_reward(deps, &mut state, &config, height, true)?;
    let mut reward_info = read_reward(deps.storage, &staker_addr_raw)?;
    before_share_change(&state, &mut reward_info)?;

    Ok(BalanceResponse {
        share: reward_info.share,
        staked_amount: calc_balance(reward_info.share, &staked),
        unstaked_amount: reward_info.amount,
        locked_amount: reward_info.calc_locked_amount(height),
    })
}

fn calc_balance(share: Uint128, staked: &GovBalanceResponse) -> Uint128 {
    if staked.share.is_zero() {
        Uint128::zero()
    } else {
        share.multiply_ratio(staked.balance, staked.share)
    }
}

fn query_state(deps: Deps) -> StdResult<StateInfo> {
    let state = read_state(deps.storage)?;
    Ok(StateInfo {
        previous_share: state.previous_share,
        share_index: state.share_index,
        total_weight: state.total_weight,
    })
}

fn query_shares(deps: Deps) -> StdResult<SharesResponse> {
    let shares = read_rewards(deps.storage)?;
    Ok(SharesResponse {
        shares: shares
            .into_iter()
            .map(|it| ShareInfo {
                address: deps.api.addr_humanize(&it.0).unwrap().to_string(),
                weight: it.1.weight,
                share_index: it.1.share_index,
                share: it.1.share,
                amount: it.1.amount,
                lock_start: it.1.lock_start,
                lock_end: it.1.lock_end,
                lock_amount: it.1.lock_amount,
            })
            .collect(),
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
