use cosmwasm_std::{CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery, to_binary};
use astroport::generator::{
    QueryMsg as AstroportQueryMsg, RewardInfoResponse
};

pub fn query_anchor_reward_info(
    deps: Deps,
    lp_token: &CanonicalAddr,
    staker: &CanonicalAddr,
    astroport_generator: &CanonicalAddr
) -> StdResult<RewardInfoResponse> {
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::RewardInfo {
            lp_token: deps.api.addr_humanize(lp_token)?
        })?,
    }))?;

    Ok(res)
}

pub fn query_anchor_pool_balance(
    deps: Deps,
    lp_token: &CanonicalAddr,
    staker: &CanonicalAddr,
    astroport_generator: &CanonicalAddr
) -> StdResult<Uint128> {
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::Deposit {
            lp_token: deps.api.addr_humanize(lp_token)?,
            user: deps.api.addr_humanize(staker)?,
        })?,
    }))?;
    
    Ok(res)
}
