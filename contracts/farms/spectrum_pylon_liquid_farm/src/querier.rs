use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Addr, Uint128};

use pylon_gateway::pool_msg::{QueryMsg as PylonGatewayPoolQueryMsg};
use pylon_gateway::pool_resp::{ClaimableRewardResponse};
use spectrum_protocol::gov_proxy::{QueryMsg as GovProxyQueryMsg, StakerResponse};

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

pub fn query_farm_gov_balance(
    deps: Deps,
    gov_proxy: &Option<CanonicalAddr>,
    address: String,
) -> StdResult<StakerResponse> {
    if let Some(gov_proxy) = gov_proxy {
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.addr_humanize(gov_proxy)?.to_string(),
            msg: to_binary(&GovProxyQueryMsg::Staker {
                address,
            })?,
        }))
    } else {
        Ok(StakerResponse {
            balance: Uint128::zero(),
        })
    }
}
