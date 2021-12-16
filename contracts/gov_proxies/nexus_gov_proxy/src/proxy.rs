use cosmwasm_std::{attr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, to_binary, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::gov_proxy::StakerInfoGovResponse;
use crate::querier::query_nexus_gov;
use crate::state::{Config, read_config, read_state, State, state_store};
use nexus_token::governance::{AnyoneMsg, Cw20HookMsg as NexusGovCw20HookMsg, ExecuteMsg as NexusGovExecuteMsg};

pub fn query_staker_info_gov(
    deps: Deps,
    env: Env,
) -> StdResult<StakerInfoGovResponse> {
    let config: Config = read_config(deps.storage)?;
    let gov_response = query_nexus_gov(deps, &config.farm_gov, &env.contract.address)?;
    let proxy_response = StakerInfoGovResponse {
        bond_amount: gov_response.balance
    };
    Ok(proxy_response)
}

pub fn stake(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    if config.farm_contract.unwrap() != deps.api.addr_canonicalize(cw20_msg.sender.as_str())? || config.farm_token != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    let nexus_token = deps.api.addr_humanize(&config.farm_token)?;
    let nexus_gov = deps.api.addr_humanize(&config.farm_gov)?;
    let mut state: State = read_state(deps.storage)?;
    state.total_deposit = state.total_deposit + cw20_msg.amount;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: nexus_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: nexus_gov.to_string(),
                msg: to_binary(&NexusGovCw20HookMsg::StakeVotingTokens {})?,
                amount: cw20_msg.amount
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("sender", info.sender.to_string()),
            attr("contract_addr", nexus_gov),
            attr("token", nexus_token),
            attr("amount", cw20_msg.amount),
        ]))
}

pub fn unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    if config.farm_contract.unwrap() != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    let nexus_token = deps.api.addr_humanize(&config.farm_token)?;
    let nexus_gov = deps.api.addr_humanize(&config.farm_gov)?;

    let available_amount = query_nexus_gov(deps.as_ref(), &config.farm_gov, &env.contract.address)?.balance;

    if amount.unwrap_or_else(|| available_amount) > available_amount {
        return Err(StdError::generic_err("cannot unstake gov more than available"));
    }

    let amount = if amount.is_some(){
        amount.unwrap()
    } else {
        available_amount
    };

    let mut state: State = read_state(deps.storage)?;
    state.total_withdraw = state.total_withdraw + amount;
    state_store(deps.storage).save(&state)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: nexus_gov.to_string(),
        msg: to_binary(&NexusGovExecuteMsg::Anyone {
            anyone_msg: AnyoneMsg::WithdrawVotingTokens {
                amount: Some(amount),
            },
        })?,
        funds: vec![],
    }));
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: nexus_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount
        })?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "unstake"),
        attr("sender", info.sender.to_string()),
        attr("contract_addr", nexus_gov),
        attr("token", nexus_token),
        attr("amount", amount),
    ]))
}