use classic_bindings::TerraQuery;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{attr, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery, Coin};

use crate::state::{config_store, read_config, read_reward, read_rewards, reward_store, Config, state_store, read_state};
use cw20::{Cw20ExecuteMsg};
use classic_terraswap::asset::{Asset, AssetInfo};
use classic_terraswap::pair::{ExecuteMsg as PairExecuteMsg};
use classic_terraswap::querier::{query_pair_info, query_token_balance, simulate};
use spectrum_protocol::gov::{BalanceResponse as GovBalanceResponse, Cw20HookMsg as GovCw20HookMsg, ExecuteMsg as GovExecuteMsg, QueryMsg as GovQueryMsg, VoteOption};
use spectrum_protocol::wallet::{BalanceResponse, ConfigInfo, ExecuteMsg, MigrateMsg, QueryMsg, ShareInfo, SharesResponse, StateInfo};
use moneymarket::market::{Cw20HookMsg as MarketCw20HookMsg};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<TerraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    config_store(deps.storage).save(&Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        spectrum_token: deps.api.addr_canonicalize(&msg.spectrum_token)?,
        spectrum_gov: deps.api.addr_canonicalize(&msg.spectrum_gov)?,
        aust_token: deps.api.addr_canonicalize(&msg.aust_token)?,
        anchor_market: deps.api.addr_canonicalize(&msg.anchor_market)?,
        terraswap_factory: deps.api.addr_canonicalize(&msg.terraswap_factory)?,
    })?;

    state_store(deps.storage).save(&StateInfo {
        total_burn: Uint128::zero(),
        buyback_ust: Uint128::zero(),
        buyback_spec: Uint128::zero(),
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut<TerraQuery>, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::poll_vote {
            poll_id,
            vote,
            amount,
        } => poll_vote(deps, info, poll_id, vote, amount),
        ExecuteMsg::stake { amount, days } => stake(deps, info, amount, days),
        ExecuteMsg::unstake { amount, days } => unstake(deps, info, amount, days),
        ExecuteMsg::upsert_share {
            address,
            lock_start,
            lock_end,
            lock_amount,
            disable_withdraw,
        } => upsert_share(
            deps,
            info,
            address,
            lock_start,
            lock_end,
            lock_amount,
            disable_withdraw,
        ),
        ExecuteMsg::update_config { owner } => update_config(deps, info, owner),
        ExecuteMsg::update_stake { amount, from_days, to_days } => update_stake(deps, info, amount, from_days, to_days),
        ExecuteMsg::withdraw { spec_amount, aust_amount } => withdraw(deps, env, info, spec_amount, aust_amount),
        ExecuteMsg::gov_claim { aust_amount, days } => harvest(deps, info, aust_amount, days),
        ExecuteMsg::burn { spec_amount } => burn(deps, env, info, spec_amount),
        ExecuteMsg::aust_redeem { aust_amount } => aust_redeem(deps, env, info, aust_amount),
        ExecuteMsg::buy_spec { ust_amount } => buy_spec(deps, env, info, ust_amount),
    }
}

fn poll_vote(
    deps: DepsMut<TerraQuery>,
    info: MessageInfo,
    poll_id: u64,
    vote: VoteOption,
    amount: Uint128,
) -> StdResult<Response> {
    // anyone in shared wallet can vote
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    Ok(Response::new()
           .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
                msg: to_binary(&GovExecuteMsg::poll_vote {
                    poll_id,
                    vote,
                    amount,
                })?,
                funds: vec![],
            })]))
}

fn stake(deps: DepsMut<TerraQuery>, info: MessageInfo, amount: Uint128, days: Option<u64>) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
                amount,
                msg: to_binary(&GovCw20HookMsg::stake_tokens { staker_addr: None, days })?,
            })?,
            funds: vec![],
        })]))
}

fn unstake(deps: DepsMut<TerraQuery>, info: MessageInfo, amount: Option<Uint128>, days: Option<u64>) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&GovExecuteMsg::withdraw {
                amount,
                days,
            })?,
            funds: vec![],
        })]))
}

fn harvest(deps: DepsMut<TerraQuery>, info: MessageInfo, aust_amount: Option<Uint128>, days: Option<u64>) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&GovExecuteMsg::harvest {
                aust_amount,
                days,
            })?,
            funds: vec![],
        })]))
}

fn update_stake(
    deps: DepsMut<TerraQuery>,
    info: MessageInfo,
    amount: Uint128,
    from_days: u64,
    to_days: u64,
) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&GovExecuteMsg::update_stake {
                amount,
                from_days,
                to_days
            })?,
            funds: vec![],
        })]))
}

fn withdraw(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    spec_amount: Option<Uint128>,
    aust_amount: Option<Uint128>,
) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let reward_info = read_reward(deps.storage, &sender_addr)?;
    if reward_info.disable_withdraw {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;

    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spec_onhand = query_token_balance(&deps.querier, spectrum_token.clone(), env.contract.address.clone())?;
    let balance: GovBalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        msg: to_binary(&GovQueryMsg::balance {
            address: env.contract.address.to_string(),
        })?,
    }))?;

    let total_amount = spec_onhand + balance.balance;
    let locked_amount = reward_info.calc_locked_amount(env.block.height);
    let withdrawable = total_amount.checked_sub(locked_amount)?;
    let spec_withdraw_amount = if let Some(amount) = spec_amount {
        if amount > withdrawable {
            return Err(StdError::generic_err("not enough amount to withdraw"));
        }
        amount
    } else if spec_onhand > withdrawable {
        withdrawable
    } else {
        spec_onhand
    };

    let mut messages: Vec<CosmosMsg> = vec![];
    if !spec_withdraw_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: spec_withdraw_amount,
            })?,
            funds: vec![],
        }));
    }

    let aust_token = deps.api.addr_humanize(&config.aust_token)?;
    let aust_onhand = query_token_balance(&deps.querier, aust_token.clone(), env.contract.address)?;
    let aust_withdraw_amount = aust_amount.unwrap_or(aust_onhand);
    if !aust_withdraw_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: aust_withdraw_amount,
            })?,
            funds: vec![],
        }));
    }

    Ok(Response::new()
        .add_messages(messages))
}

fn burn(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    spec_amount: Option<Uint128>,
) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spec_onhand = query_token_balance(&deps.querier, spectrum_token.clone(), env.contract.address)?;
    let burn_amount = spec_amount.unwrap_or(spec_onhand);

    let mut state = read_state(deps.storage)?;
    state.total_burn += burn_amount;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Burn {
                amount: burn_amount,
            })?,
            funds: vec![],
        })]))
}

fn aust_redeem(deps: DepsMut<TerraQuery>, env: Env, info: MessageInfo, aust_amount: Option<Uint128>) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    let aust_token = deps.api.addr_humanize(&config.aust_token)?;
    let aust_balance = query_token_balance(
        &deps.querier,
        aust_token,
        env.contract.address)?;
    let amount = aust_amount.unwrap_or(aust_balance);

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.aust_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
                amount,
                msg: to_binary(&MarketCw20HookMsg::RedeemStable { })?,
            })?,
            funds: vec![],
        })]))
}

fn buy_spec(deps: DepsMut<TerraQuery>, env: Env, info: MessageInfo, ust_amount: Option<Uint128>) -> StdResult<Response> {
    let sender_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let found = reward_store(deps.storage).may_load(&sender_addr)?.is_some();
    if !found {
        return Err(StdError::generic_err("unauthorized"));
    }

    let config = read_config(deps.storage)?;
    let factory_contract = deps.api.addr_humanize(&config.terraswap_factory)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spec_info = AssetInfo::Token {
        contract_addr: spectrum_token.to_string(),
    };
    let ust_info = AssetInfo::NativeToken {
        denom: "uusd".to_string()
    };
    let pair_info = query_pair_info(&deps.querier, factory_contract, &[spec_info, ust_info.clone()])?;
    let avail_ust = deps.querier.query_balance(env.contract.address, "uusd")?;
    let amount = ust_amount.unwrap_or(avail_ust.amount);
    let swap_amount = Asset {
        info: ust_info.clone(),
        amount,
    }.deduct_tax(&deps.querier)?.amount;
    let offer_asset = Asset {
        info: ust_info,
        amount: swap_amount,
    };

    let simulate = simulate(
        &deps.querier,
        deps.api.addr_validate(&pair_info.contract_addr)?,
        &offer_asset)?;

    let mut state = read_state(deps.storage)?;
    state.buyback_ust += amount;
    state.buyback_spec += simulate.return_amount;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pair_info.contract_addr,
                msg: to_binary(&PairExecuteMsg::Swap {
                    offer_asset,
                    to: None,
                    belief_price: None,
                    max_spread: None,
                    deadline: None,
                })?,
                funds: vec![Coin { denom: "uusd".to_string(), amount: swap_amount }],
            }),
        ]))
}

#[allow(clippy::too_many_arguments)]
fn upsert_share(
    deps: DepsMut<TerraQuery>,
    info: MessageInfo,
    address: String,
    lock_start: Option<u64>,
    lock_end: Option<u64>,
    lock_amount: Option<Uint128>,
    disable_withdraw: Option<bool>,
) -> StdResult<Response> {

    let lock_start = lock_start.unwrap_or_default();
    let lock_end = lock_end.unwrap_or_default();
    if lock_end < lock_start {
        return Err(StdError::generic_err("invalid lock parameters"));
    }
    let disable_withdraw = disable_withdraw.unwrap_or_default();

    let config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    // allow only 1 share
    let address_raw = deps.api.addr_canonicalize(&address)?;
    let key = address_raw.as_slice();
    let reward_info_op = reward_store(deps.storage).may_load(key)?;
    if reward_info_op.is_none() && !read_rewards(deps.storage)?.is_empty() {
        return Err(StdError::generic_err("allow only 1 share"));
    }

    let mut reward_info = reward_info_op.unwrap_or_default();
    reward_info.lock_start = lock_start;
    reward_info.lock_end = lock_end;
    reward_info.lock_amount = lock_amount.unwrap_or_else(Uint128::zero);
    reward_info.disable_withdraw = disable_withdraw;

    reward_store(deps.storage).save(key, &reward_info)?;
    Ok(Response::default())
}

fn update_config(
    deps: DepsMut<TerraQuery>,
    info: MessageInfo,
    owner: Option<String>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        if config.owner == config.spectrum_gov {
            return Err(StdError::generic_err("cannot update owner"));
        }
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    config_store(deps.storage).save(&config)?;
    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TerraQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::balance { address } => to_binary(&query_balance(deps, env, address)?),
        QueryMsg::shares {} => to_binary(&query_shares(deps)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
    }
}

fn query_config(deps: Deps<TerraQuery>) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        spectrum_token: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
        spectrum_gov: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        aust_token: deps.api.addr_humanize(&config.aust_token)?.to_string(),
        anchor_market: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
        terraswap_factory: deps.api.addr_humanize(&config.terraswap_factory)?.to_string(),
    };

    Ok(resp)
}

fn query_balance(deps: Deps<TerraQuery>, env: Env, staker_addr: String) -> StdResult<BalanceResponse> {

    let staker_addr_raw = deps.api.addr_canonicalize(&staker_addr)?;
    let reward_info = read_reward(deps.storage, &staker_addr_raw)?;
    let config = read_config(deps.storage)?;

    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let unstaked_amount = query_token_balance(&deps.querier, spectrum_token, env.contract.address.clone())?;
    let balance: GovBalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        msg: to_binary(&GovQueryMsg::balance {
            address: env.contract.address.to_string(),
        })?,
    }))?;

    Ok(BalanceResponse {
        share: balance.share,
        staked_amount: balance.balance,
        unstaked_amount,
        locked_amount: reward_info.calc_locked_amount(env.block.height),
    })
}

fn query_state(deps: Deps<TerraQuery>) -> StdResult<StateInfo> {
    let state = read_state(deps.storage)?;
    Ok(state)
}

fn query_shares(deps: Deps<TerraQuery>) -> StdResult<SharesResponse> {
    let shares = read_rewards(deps.storage)?;
    Ok(SharesResponse {
        shares: shares
            .into_iter()
            .map(|it| ShareInfo {
                address: deps.api.addr_humanize(&it.0).unwrap().to_string(),
                lock_start: it.1.lock_start,
                lock_end: it.1.lock_end,
                lock_amount: it.1.lock_amount,
            })
            .collect(),
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut<TerraQuery>, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    config.aust_token = deps.api.addr_canonicalize(&msg.aust_token)?;
    config.anchor_market = deps.api.addr_canonicalize(&msg.anchor_market)?;
    config.terraswap_factory = deps.api.addr_canonicalize(&msg.terraswap_factory)?;
    config_store(deps.storage).save(&config)?;

    Ok(Response::default())
}
