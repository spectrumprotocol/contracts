use classic_bindings::TerraQuery;
use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use loterra::staking::{QueryMsg as LoterraStakingQueryMsg, HolderResponse, AccruedRewardsResponse};

pub fn query_loterra_reward_info(
    deps: Deps<TerraQuery>,
    loterra_staking: &CanonicalAddr,
    staker: &Addr
) -> StdResult<HolderResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(loterra_staking)?.to_string(),
        msg: to_binary(&LoterraStakingQueryMsg::Holder {
            address: staker.to_string(),
        })?,
    }))
}

pub fn query_loterra_pool_balance(
    deps: Deps<TerraQuery>,
    loterra_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    query_loterra_reward_info(deps, loterra_staking, staker)
        .map(|it| it.balance)
}

pub fn query_loterra_accrued_reward(
    deps: Deps<TerraQuery>,
    loterra_staking: &CanonicalAddr,
    staker: &Addr
) -> StdResult<Uint128> {
    deps.querier.query::<AccruedRewardsResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(loterra_staking)?.to_string(),
        msg: to_binary(&LoterraStakingQueryMsg::AccruedRewards {
            address: staker.to_string(),
        })?,
    })).map(|it| it.rewards)
}
