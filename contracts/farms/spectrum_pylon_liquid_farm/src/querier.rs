use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use pylon_gateway::pool_msg::{QueryMsg as PylonGatewayPoolQueryMsg};
use pylon_gateway::pool_resp::{BalanceOfResponse, ClaimableRewardResponse};

pub fn query_claimable_reward(
    deps: Deps,
    gateway: &CanonicalAddr,
    owner: &Addr,
    timestamp: Option<u64>,
) -> StdResult<ClaimableRewardResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(gateway)?.to_string(),
        msg: to_binary(&PylonGatewayPoolQueryMsg::ClaimableReward {
            owner: owner.to_string(),
            timestamp
        })?,
    }))
}

pub fn query_balance_of(
    deps: Deps,
    gateway: &CanonicalAddr,
    owner: &Addr,
) -> StdResult<BalanceOfResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(gateway)?.to_string(),
        msg: to_binary(&PylonGatewayPoolQueryMsg::BalanceOf {
            owner: owner.to_string(),
        })?,
    }))
}