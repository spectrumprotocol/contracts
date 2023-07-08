use classic_bindings::TerraQuery;
use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use valkyrie::lp_staking::query_msgs::{QueryMsg as ValkyrieStakingQueryMsg, StakerInfoResponse};

pub fn query_valkyrie_reward_info(
    deps: Deps<TerraQuery>,
    valkyrie_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(valkyrie_staking)?.to_string(),
        msg: to_binary(&ValkyrieStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
        })?,
    }))
}

pub fn query_valkyrie_pool_balance(
    deps: Deps<TerraQuery>,
    valkyrie_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    query_valkyrie_reward_info(deps, valkyrie_staking, staker)
        .map(|it| it.bond_amount)
}
