use cosmwasm_std::{Deps, Env, StdResult};
use spectrum_protocol::gov_proxy::StakerInfoGovResponse;

pub(crate) fn query_staker_info_gov(
    deps: Deps,
    env: Env,
    staker_addr: String
) -> StdResult<StakerInfoGovResponse> {

    Ok(resp)
}