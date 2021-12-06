use cosmwasm_std::{CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, to_binary, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::gov_proxy::StakerInfoGovResponse;
use crate::querier::query_anchor_gov;
use crate::state::{Config, read_config};

pub fn query_staker_info_gov(
    deps: Deps,
    _env: Env,
    staker_addr: String
) -> StdResult<StakerInfoGovResponse> {
    let config: Config = read_config(deps.storage)?;
    let gov_response = query_anchor_gov(deps, &config.farm_gov, &deps.api.addr_validate(staker_addr.as_str())?)?;
    let proxy_response = StakerInfoGovResponse {
        bond_amount: gov_response.balance
    };
    Ok(proxy_response)
}

pub fn stake(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    sender: String,
    amount: Uint128
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    if config.farm_contract.unwrap() != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.farm_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: config.farm_gov.to_string(),
                amount
                msg: to_binary(&AnchorGovCw20HookMsg::StakeVotingTokens {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("staking_token", staking_token),
            attr("asset_token", asset_token),
            attr("amount", amount),
        ]))
}