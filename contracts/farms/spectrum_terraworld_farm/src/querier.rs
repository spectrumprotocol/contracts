use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128, Addr};

use terraworld_token::staking::{QueryMsg as TerraworldStakingQueryMsg, StakerInfoResponse};

pub fn query_terraworld_reward_info(
    deps: Deps,
    terraworld_staking: &CanonicalAddr,
    staker: &Addr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(terraworld_staking)?.to_string(),
        msg: to_binary(&TerraworldStakingQueryMsg::StakerInfo {
            staker: staker.to_string(),
            block_height,
        })?,
    }))
}

pub fn query_terraworld_pool_balance(
    deps: Deps,
    terraworld_staking: &CanonicalAddr,
    staker: &Addr,
) -> StdResult<Uint128> {
    query_terraworld_reward_info(deps, terraworld_staking, staker, None)
        .map(|it| it.bond_amount)
}
