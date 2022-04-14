use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use pylon_token::staking::{QueryMsg as PylonStakingQueryMsg, StakerInfoResponse};

pub fn query_pylon_reward_info(
    deps: Deps,
    pylon_staking: &CanonicalAddr,
    staker: &Addr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(pylon_staking)?.to_string(),
        msg: to_binary(&PylonStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
            block_height,
        })?,
    }))
}

pub fn query_pylon_pool_balance(
    deps: Deps,
    pylon_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    query_pylon_reward_info(deps, pylon_staking, staker, None)
        .map(|it| it.bond_amount)
}
