use cosmwasm_std::{
    to_binary, Api, CanonicalAddr, Extern, Querier, QueryRequest, StdResult, Storage, WasmQuery,
};

use pylon_protocol::staking::{QueryMsg as PylonStakingQueryMsg, StakerInfoResponse};

pub fn query_pylon_reward_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    pylon_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    height: u64,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.human_address(pylon_staking)?,
        msg: to_binary(&PylonStakingQueryMsg::StakerInfo {
            staker: deps.api.human_address(staker)?,
            block_height: Some(height),
        })?,
    }))?;

    Ok(res)
}
