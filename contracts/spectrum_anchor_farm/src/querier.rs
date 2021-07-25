use cosmwasm_std::{
    to_binary, Api, CanonicalAddr, Extern, Querier, QueryRequest, StdResult, Storage, WasmQuery,
};

use anchor_token::staking::{QueryMsg as AnchorStakingQueryMsg, StakerInfoResponse};

pub fn query_anchor_reward_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    anchor_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    height: u64,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.human_address(anchor_staking)?,
        msg: to_binary(&AnchorStakingQueryMsg::StakerInfo {
            staker: deps.api.human_address(staker)?,
            block_height: Some(height),
        })?,
    }))?;

    Ok(res)
}
