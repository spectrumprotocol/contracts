use cosmwasm_std::{CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery, to_binary};
use astroport::generator::{PendingTokenResponse, QueryMsg as AstroportQueryMsg, RewardInfoResponse};
use spectrum_protocol::gov_proxy::{
    StakerInfoGovResponse, QueryMsg as GovProxyQueryMsg
};

pub fn query_astroport_pending_token(
    deps: Deps,
    lp_token: &CanonicalAddr,
    staker: &CanonicalAddr,
    astroport_generator: &CanonicalAddr
) -> StdResult<PendingTokenResponse> {
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(astroport_generator)?.to_string(),
        msg: to_binary(&AstroportQueryMsg::PendingToken {
            lp_token: deps.api.addr_humanize(lp_token)?,
            user: deps.api.addr_humanize(staker)?,
        })?,
    }))?;

    Ok(res)
}

pub fn query_astroport_pool_balance(
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
