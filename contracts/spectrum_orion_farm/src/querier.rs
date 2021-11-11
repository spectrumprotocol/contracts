use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128};

use orion::lp_staking::{QueryMsg as OrionStakingQueryMsg, StakerInfoResponse};

pub fn query_orion_reward_info(
    deps: Deps,
    orion_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    timestamp: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(orion_staking)?.to_string(),
        msg: to_binary(&OrionStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            timestamp
        })?,
    }))?;

    Ok(res)
}

pub fn query_orion_pool_balance(
    deps: Deps,
    orion_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    time_seconds: u64
) -> StdResult<Uint128> {
    let res = query_orion_reward_info(deps, orion_staking, staker, Some(time_seconds))?;
    Ok(res.bond_amount)
}
