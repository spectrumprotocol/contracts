use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Addr, Uint128};

use basset_vault::nasset_token_rewards::{QueryMsg as nAssetQueryMsg, AccruedRewardsResponse};
use spectrum_protocol::gov_proxy::{QueryMsg as GovProxyQueryMsg, StakerResponse};

pub fn query_claimable_reward(
    deps: Deps,
    gateway_pool: &CanonicalAddr,
    address: &Addr
) -> StdResult<AccruedRewardsResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(gateway_pool)?.to_string(),
        msg: to_binary(&nAssetQueryMsg::AccruedRewards {
            address: address.to_string()
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
