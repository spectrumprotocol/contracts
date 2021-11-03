use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128};

use nexus_token::staking::{QueryMsg as NexusStakingQueryMsg, StakerInfoResponse};

pub fn query_nexus_reward_info(
    deps: Deps,
    nasset_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    time_seconds: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(nasset_staking)?.to_string(),
        msg: to_binary(&NexusStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            time_seconds
        })?,
    }))?;

    Ok(res)
}

pub fn query_nexus_pool_balance(
    deps: Deps,
    nasset_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    time_seconds: u64
) -> StdResult<Uint128> {
    let res = query_nexus_reward_info(deps, nasset_staking, staker, Some(time_seconds))?;
    Ok(res.bond_amount)
}
