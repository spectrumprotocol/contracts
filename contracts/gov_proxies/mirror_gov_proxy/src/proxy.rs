use cosmwasm_std::{CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, to_binary, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg};
use spectrum_protocol::gov_proxy::StakerResponse;
use crate::querier::query_mirror_gov;
use crate::state::{account_store, read_account, read_config, read_state, state_store};
use mirror_protocol::gov::{Cw20HookMsg as MirrorCw20HookMsg, ExecuteMsg as MirrorExecuteMsg};

pub fn query_staker_info_gov(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<StakerResponse> {
    let config = read_config(deps.storage)?;
    let state = read_state(deps.storage)?;

    let gov_response = query_mirror_gov(deps, &config.farm_gov, &env.contract.address)?;
    let addr_raw = deps.api.addr_canonicalize(&address).unwrap();
    let account = read_account(deps.storage, addr_raw.as_slice())?
        .unwrap_or_default();
    let balance = state.calc_balance(gov_response.balance, account.share);

    Ok(StakerResponse {
        balance,
    })
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: String,
    amount: Uint128,
) -> StdResult<Response> {

    if amount.is_zero() {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    let config = read_config(deps.storage)?;
    if config.farm_token != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut state = state_store(deps.storage).load()?;

    let gov_response = query_mirror_gov(deps.as_ref(), &config.farm_gov, &env.contract.address)?;
    let sender_address_raw = deps.api.addr_canonicalize(&sender)?;
    let key = sender_address_raw.as_slice();
    let mut account = account_store(deps.storage)
        .may_load(key)?
        .unwrap_or_default();

    let share = state.calc_share(gov_response.balance, amount);
    account.share += share;
    state.total_share += share;

    state_store(deps.storage).save(&state)?;
    account_store(deps.storage).save(key, &account)?;

    let mirror_token = deps.api.addr_humanize(&config.farm_token)?;
    let mirror_gov = deps.api.addr_humanize(&config.farm_gov)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: mirror_gov.to_string(),
                msg: to_binary(&MirrorCw20HookMsg::StakeVotingTokens {})?,
                amount
            })?,
        })]))
}

pub fn unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>
) -> StdResult<Response> {

    let sender_address_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let key = sender_address_raw.as_slice();

    let config = read_config(deps.storage)?;
    let mut state = state_store(deps.storage).load()?;

    let gov_response = query_mirror_gov(deps.as_ref(), &config.farm_gov, &env.contract.address)?;
    let mut account = account_store(deps.storage).load(key)?;
    let user_balance = state.calc_balance(gov_response.balance, account.share);
    let amount = amount.unwrap_or(user_balance);
    if amount > user_balance {
        return Err(StdError::generic_err(
            "User is trying to withdraw too many tokens.",
        ));
    }

    let mut withdraw_share = state.calc_share(gov_response.balance, amount);
    if state.calc_balance(gov_response.balance, withdraw_share) < amount {
        withdraw_share += Uint128::from(1u128);
    }

    account.share -= withdraw_share;
    state.total_share -= withdraw_share;

    account_store(deps.storage).save(key, &account)?;
    state_store(deps.storage).save(&state)?;

    let mirror_token = deps.api.addr_humanize(&config.farm_token)?;
    let mirror_gov = deps.api.addr_humanize(&config.farm_gov)?;

    Ok(Response::new().add_messages(vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_gov.to_string(),
            msg: to_binary(&MirrorExecuteMsg::WithdrawVotingTokens {
                amount: Some(amount),
            })?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount
            })?,
            funds: vec![],
        })
    ]))
}
