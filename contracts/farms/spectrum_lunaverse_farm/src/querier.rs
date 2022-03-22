use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use crate::lunaverse_staking::{QueryMsg as LunaverseStakingQueryMsg, StakerInfoResponse};

pub fn query_lunaverse_reward_info(
    deps: Deps,
    lunaverse_staking: &CanonicalAddr,
    staker: &Addr
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(lunaverse_staking)?.to_string(),
        msg: to_binary(&LunaverseStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
        })?,
    }))
}

pub fn query_lunaverse_pool_balance(
    deps: Deps,
    lunaverse_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    query_lunaverse_reward_info(deps, lunaverse_staking, staker)
        .map(|it| it.bond_amount)
}
