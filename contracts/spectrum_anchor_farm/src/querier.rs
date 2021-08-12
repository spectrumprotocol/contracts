use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery};

use anchor_token::staking::{QueryMsg as AnchorStakingQueryMsg, StakerInfoResponse};

pub fn query_anchor_reward_info(
    deps: Deps,
    anchor_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    height: u64,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(anchor_staking)?.to_string(),
        msg: to_binary(&AnchorStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            block_height: Some(height),
        })?,
    }))?;

    Ok(res)
}
