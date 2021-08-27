use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128};

use pylon_token::staking::{QueryMsg as PylonStakingQueryMsg, StakerInfoResponse};

pub fn query_pylon_reward_info(
    deps: Deps,
    pylon_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(pylon_staking)?.to_string(),
        msg: to_binary(&PylonStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            block_height,
        })?,
    }))?;

    Ok(res)
}

pub fn query_pylon_pool_balance(
    deps: Deps,
    anchor_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
) -> StdResult<Uint128> {
    let res = query_pylon_reward_info(deps, anchor_staking, staker, None)?;
    Ok(res.bond_amount)
}
