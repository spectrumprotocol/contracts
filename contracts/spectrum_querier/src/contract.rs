use classic_bindings::TerraQuery;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Empty, WasmQuery, QueryRequest, QuerierResult, ContractResult};
use schemars::_serde_json::to_vec;
use crate::model::{Query, QueryMsg};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    _deps: DepsMut<TerraQuery>,
    _env: Env,
    _info: MessageInfo,
    _msg: Empty,
) -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(_deps: DepsMut<TerraQuery>, _env: Env, _info: MessageInfo, _msg: Empty) -> StdResult<Response> {
    Err(StdError::generic_err("not support"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TerraQuery>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::bundle { queries } => to_binary(&query_bundle(deps, queries)?),
    }
}

fn query_bundle(deps: Deps<TerraQuery>, queries: Vec<Query>) -> StdResult<Vec<Binary>> {
    let mut results: Vec<Binary> = vec![];
    for query in queries {
        let request: QueryRequest<Empty> = QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: query.addr,
            msg: query.msg
        });
        let raw = to_vec(&request)
            .map_err(|serialize_err| {
                StdError::generic_err(format!("Serializing QueryRequest: {}", serialize_err))
            })?;
        let result = match deps.querier.raw_query(&raw) {
            QuerierResult::Ok(ContractResult::Ok(value)) => Ok(value),
            QuerierResult::Ok(ContractResult::Err(contract_err)) => Err(StdError::generic_err(format!("Querier contract error: {}", contract_err))),
            QuerierResult::Err(system_err) => Err(StdError::generic_err(format!("Querier system error: {}", system_err))),
        }?;
        results.push(result);
    }
    Ok(results)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut<TerraQuery>, _env: Env, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}
