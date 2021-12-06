use cosmwasm_std::{attr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, to_binary, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::gov_proxy::StakerInfoGovResponse;
use crate::querier::query_anchor_gov;
use crate::state::{Config, read_config, read_state, State, store_state};
use anchor_token::gov::{
    Cw20HookMsg as AnchorGovCw20HookMsg,
    ExecuteMsg as AnchorGovExecuteMsg
};

pub fn query_staker_info_gov(
    deps: Deps,
    _env: Env,
    _staker_addr: String
) -> StdResult<StakerInfoGovResponse> {
    let config: Config = read_config(deps.storage)?;
    let gov_response = query_anchor_gov(deps, &config.farm_gov, env.contract.address.to_string())?;
    let proxy_response = StakerInfoGovResponse {
        bond_amount: gov_response.balance
    };
    Ok(proxy_response)
}

pub fn stake(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _sender: String,
    amount: Uint128
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    if config.farm_contract.unwrap() != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    let anchor_token = deps.api.addr_humanize(&config.farm_token)?;
    let anchor_gov = deps.api.addr_humanize(&config.farm_gov)?;
    let mut state: State = read_state(deps.storage)?;
    state.total_deposit = state.total_deposit + amount;
    store_state(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: anchor_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: anchor_gov.to_string(),
                msg: to_binary(&AnchorGovCw20HookMsg::StakeVotingTokens {})?,
                amount
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("contract_addr", anchor_gov),
            attr("token", anchor_token),
            attr("amount", amount),
        ]))
}

pub fn unstake(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Option<Uint128>
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    if config.farm_contract.unwrap() != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    let anchor_token = deps.api.addr_humanize(&config.farm_token)?;
    let anchor_gov = deps.api.addr_humanize(&config.farm_gov)?;

    let amount = if amount.is_some(){
        amount.unwrap()
    } else {
        query_anchor_gov(deps.as_ref(), &config.farm_gov, env.contract.address.to_string())?.balance;
    };

    let mut state: State = read_state(deps.storage)?;
    state.total_withdraw = state.total_withdraw + amount;
    store_state(deps.storage).save(&state)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: anchor_gov.to_string(),
        msg: to_binary(&AnchorGovExecuteMsg::WithdrawVotingTokens {
            amount: Some(amount),
        })?,
        funds: vec![],
    }));
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: anchor_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount
        })?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "unstake"),
        attr("contract_addr", anchor_gov),
        attr("token", anchor_token),
        attr("amount", amount),
    ]))
}