use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Addr};
use mirror_protocol::gov::{QueryMsg, StakerResponse};

pub fn query_mirror_gov(
    deps: Deps,
    mirror_gov: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<StakerResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(mirror_gov)?.to_string(),
        msg: to_binary(&QueryMsg::Staker {
            address: staker.to_string()
        })?,
    }))
}
