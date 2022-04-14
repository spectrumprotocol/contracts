use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use orion::lp_staking::{QueryMsg as OrionStakingQueryMsg, StakerInfoResponse};

pub fn query_orion_reward_info(
    deps: Deps,
    orion_staking: &CanonicalAddr,
    staker: &Addr,
    timestamp: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(orion_staking)?.to_string(),
        msg: to_binary(&OrionStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
            timestamp
        })?,
    }))
}

pub fn query_orion_pool_balance(
    deps: Deps,
    orion_staking: &CanonicalAddr,
    staker: &Addr,
    time_seconds: u64
) -> StdResult<Uint128> {
    query_orion_reward_info(deps, orion_staking, staker, Some(time_seconds))
        .map(|it| it.bond_amount)
}
