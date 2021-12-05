use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use anchor_token::staking::{QueryMsg as AnchorStakingQueryMsg, StakerInfoResponse};

pub fn query_anchor_reward_info(
    deps: Deps,
    anchor_staking: &CanonicalAddr,
    staker: &Addr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(anchor_staking)?.to_string(),
        msg: to_binary(&AnchorStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
            block_height,
        })?,
    }))?;

    Ok(res)
}

pub fn query_anchor_pool_balance(
    deps: Deps,
    anchor_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    let res = query_anchor_reward_info(deps, anchor_staking, staker, None)?;
    Ok(res.bond_amount)
}
