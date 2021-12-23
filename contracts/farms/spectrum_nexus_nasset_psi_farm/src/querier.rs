use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery, Addr};

use nexus_token::staking::{QueryMsg as NexusStakingQueryMsg, StakerInfoResponse};

pub fn query_nexus_reward_info(
    deps: Deps,
    nasset_staking: &CanonicalAddr,
    staker: &Addr,
    time_seconds: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(nasset_staking)?.to_string(),
        msg: to_binary(&NexusStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
            time_seconds,
        })?,
    }))
}

pub fn query_nexus_pool_balance(
    deps: Deps,
    nasset_staking: &CanonicalAddr,
    staker: &Addr,
    time_seconds: u64,
) -> StdResult<Uint128> {
    query_nexus_reward_info(deps, nasset_staking, staker, Some(time_seconds))
        .map(|it| it.bond_amount)
}
