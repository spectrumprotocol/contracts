use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use glow::staking::{QueryMsg as GlowStakingQueryMsg, StakerInfoResponse};

pub fn query_glow_reward_info(
    deps: Deps,
    glow_staking: &CanonicalAddr,
    staker: &Addr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(glow_staking)?.to_string(),
        msg: to_binary(&GlowStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
            block_height,
        })?,
    }))
}

pub fn query_glow_pool_balance(
    deps: Deps,
    glow_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    query_glow_reward_info(deps, glow_staking, staker, None)
        .map(|it| it.bond_amount)
}
