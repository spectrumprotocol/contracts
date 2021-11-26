use cosmwasm_std::{Binary, CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery, to_binary};

use anchor_token::staking::{QueryMsg as AnchorStakingQueryMsg, StakerInfoResponse};
use astroport::generator::{
    QueryMsg as AstroportQueryMsg
};

pub fn query_anchor_reward_info(
    deps: Deps,
    anchor_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(anchor_staking)?.to_string(),
        msg: to_binary(&AnchorStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            block_height,
        })?,
    }))?;

    Ok(res)
}

pub fn query_anchor_pool_balance(
    deps: Deps,
    anchor_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    astroport_generator: &CanonicalAddr
) -> StdResult<Binary> {
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::Deposit{

        })?,
    }))?;
    
    Ok(res)
}
