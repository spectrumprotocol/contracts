use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr, StdError};

use spectrum_protocol::spec_farm::{QueryMsg as SpecQueryMsg, RewardInfoResponse};


pub fn query_spec_reward_info(
    deps: Deps,
    spectrum_staking: &CanonicalAddr,
    staker: &Addr
) -> StdResult<RewardInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(spectrum_staking)?.to_string(),
        msg: to_binary(&SpecQueryMsg::reward_info {
            staker_addr: staker.to_string(),
            asset_token: None,
        })?,
    }))
}

pub fn query_spec_pool_balance(
    deps: Deps,
    spectrum_staking: &CanonicalAddr,
    staker: &Addr,
    spectrum_token: &CanonicalAddr
) -> StdResult<Uint128> {
    Ok(query_spec_reward_info(deps, spectrum_staking, staker)?.reward_infos.iter().find(|it| it.asset_token == deps.api.addr_humanize(spectrum_token).unwrap().to_string()).ok_or_else(|| StdError::not_found("pool"))?.bond_amount)
}
