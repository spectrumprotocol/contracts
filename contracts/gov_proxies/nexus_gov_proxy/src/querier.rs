use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Addr};
use nexus_token::governance::{QueryMsg, StakerResponse};

pub fn query_nexus_gov(
    deps: Deps,
    nexus_gov: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<StakerResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(nexus_gov)?.to_string(),
        msg: to_binary(&QueryMsg::Staker {
            address: staker.to_string()
        })?,
    }))
}
