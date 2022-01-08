use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use terra_name_service::staking::{QueryMsg as TerraNameServiceStakingQueryMsg, StakerInfoResponse};

pub fn query_terra_name_service_reward_info(
    deps: Deps,
    terra_name_service_staking: &CanonicalAddr,
    staker: &Addr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(terra_name_service_staking)?.to_string(),
        msg: to_binary(&TerraNameServiceStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
            block_height,
        })?,
    }))
}

pub fn query_terra_name_service_pool_balance(
    deps: Deps,
    terra_name_service_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    query_terra_name_service_reward_info(deps, terra_name_service_staking, staker, None)
        .map(|it| it.bond_amount)
}
