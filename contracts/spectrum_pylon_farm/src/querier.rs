use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery};

use pylon_token::staking::{QueryMsg as PylonStakingQueryMsg, StakerInfoResponse};

pub fn query_pylon_reward_info(
    deps: Deps,
    pylon_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    height: u64,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(pylon_staking)?.to_string(),
        msg: to_binary(&PylonStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            block_height: Some(height),
        })?,
    }))?;

    Ok(res)
}
