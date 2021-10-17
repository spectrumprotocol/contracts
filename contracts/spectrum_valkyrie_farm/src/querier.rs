use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128};

use valkyrie::lp_staking::query_msgs::{QueryMsg as ValkyrieStakingQueryMsg, StakerInfoResponse};

pub fn query_valkyrie_reward_info(
    deps: Deps,
    valkyrie_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(valkyrie_staking)?.to_string(),
        msg: to_binary(&ValkyrieStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
        })?,
    }))?;

    Ok(res)
}

pub fn query_valkyrie_pool_balance(
    deps: Deps,
    valkyrie_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
) -> StdResult<Uint128> {
    let res = query_valkyrie_reward_info(deps, valkyrie_staking, staker)?;
    Ok(res.bond_amount)
}
