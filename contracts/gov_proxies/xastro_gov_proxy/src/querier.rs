use cosmwasm_std::{Deps, StdResult, Addr};
use astroport::querier::{query_supply, query_token_balance};
use spectrum_protocol::gov_proxy::StakerResponse;
use crate::state::Config;

pub fn query_xastro_gov(
    deps: Deps,
    config: &Config,
    staker: &Addr,
) -> StdResult<StakerResponse> {
    let astro_token = deps.api.addr_humanize(&config.farm_token)?;
    let xastro_gov = deps.api.addr_humanize(&config.farm_gov)?;
    let xastro_token = deps.api.addr_humanize(&config.xastro_token)?;

    let total_share = query_supply(&deps.querier, xastro_token.clone())?;
    let total_balance = query_token_balance(&deps.querier, astro_token, xastro_gov)?;
    let share = query_token_balance(&deps.querier, xastro_token, staker.clone())?;

    let balance = share.multiply_ratio(total_balance, total_share);
    Ok(StakerResponse {
        balance,
    })
}
