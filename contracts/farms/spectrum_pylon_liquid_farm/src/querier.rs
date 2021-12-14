use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Addr};

use pylon_gateway::pool_msg::{QueryMsg as PylonGatewayPoolQueryMsg};
use pylon_gateway::pool_resp::{ClaimableRewardResponse};

pub fn query_claimable_reward(
    deps: Deps,
    gateway_pool: &CanonicalAddr,
    owner: &Addr,
    timestamp: Option<u64>,
) -> StdResult<ClaimableRewardResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(gateway_pool)?.to_string(),
        msg: to_binary(&PylonGatewayPoolQueryMsg::ClaimableReward {
            owner: owner.to_string(),
            timestamp
        })?,
    }))
}
