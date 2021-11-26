use cosmwasm_std::{CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery, to_binary};
use astroport::generator::{
    QueryMsg as AstroportQueryMsg, RewardInfoResponse
};
use spectrum_protocol::astroport_gov_proxy::{
    StakerInfoGovResponse, QueryMsg as GovProxyQueryMsg
};


pub fn query_reward_info(
    deps: Deps,
    lp_token: &CanonicalAddr,
    staker: &CanonicalAddr,
    astroport_generator: &CanonicalAddr
) -> StdResult<RewardInfoResponse> {
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::RewardInfo {
            lp_token: deps.api.addr_humanize(lp_token)?
        })?,
    }))?;

    Ok(res)
}

pub fn query_pool_balance(
    deps: Deps,
    lp_token: &CanonicalAddr,
    staker: &CanonicalAddr,
    astroport_generator: &CanonicalAddr
) -> StdResult<Uint128> {
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::Deposit {
            lp_token: deps.api.addr_humanize(lp_token)?,
            user: deps.api.addr_humanize(staker)?,
        })?,
    }))?;
    
    Ok(res)
}

pub fn query_gov_balance(
    deps: Deps,
    gov_proxy: &CanonicalAddr,
    staker: &CanonicalAddr,
    astroport_generator: &CanonicalAddr
) -> StdResult<StakerInfoGovResponse> {
    let res: StakerInfoGovResponse  = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(gov_proxy)?.to_string(),
        msg: to_binary(&GovProxyQueryMsg::StakerInfo{
            staker_addr: deps.api.addr_humanize(staker)?.to_string(),
        } )?,
    }))?;
    
    Ok(res)
}

// pub fn query_mirror_pool_info(
//     deps: Deps,
//     mirror_staking: String,
//     asset_token: String,
// ) -> StdResult<PoolInfoResponse> {
//     let res: PoolInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
//         contract_addr: mirror_staking,
//         msg: to_binary(&QueryMsg::PoolInfo { asset_token })?,
//     }))?;

//     Ok(res)
// }
