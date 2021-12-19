use cosmwasm_std::{Deps, StdResult, Addr, Uint128};
use astroport::querier::{query_supply, query_token_balance};
use crate::state::Config;

pub struct XAstroResponse {
    pub balance: Uint128,
    pub share: Uint128,
    pub total_balance: Uint128,
    pub total_share: Uint128,
}

impl XAstroResponse {
    pub fn calc_share(&self, amount: Uint128) -> Uint128 {
        amount.multiply_ratio(self.total_share, self.total_balance)
    }

    pub fn calc_balance(&self, share: Uint128) -> Uint128 {
        share.multiply_ratio(self.total_balance, self.total_share)
    }
}

pub fn query_xastro_gov(
    deps: Deps,
    config: &Config,
    staker: &Addr,
) -> StdResult<XAstroResponse> {
    let astro_token = deps.api.addr_humanize(&config.farm_token)?;
    let xastro_gov = deps.api.addr_humanize(&config.farm_gov)?;
    let xastro_token = deps.api.addr_humanize(&config.xastro_token)?;

    let total_share = query_supply(&deps.querier, xastro_token.clone())?;
    let total_balance = query_token_balance(&deps.querier, astro_token, xastro_gov)?;
    let share = query_token_balance(&deps.querier, xastro_token, staker.clone())?;

    let balance = share.multiply_ratio(total_balance, total_share);
    Ok(XAstroResponse {
        balance,
        share,
        total_balance,
        total_share,
    })
}
