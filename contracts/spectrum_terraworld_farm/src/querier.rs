use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128};

use terraworld_token::staking::{QueryMsg as TerraworldStakingQueryMsg, StakerInfoResponse};

pub fn query_terraworld_reward_info(
    deps: Deps,
    terraworld_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(terraworld_staking)?.to_string(),
        msg: to_binary(&TerraworldStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            block_height,
        })?,
    }))?;

    Ok(res)
}

pub fn query_terraworld_pool_balance(
    deps: Deps,
    terraworld_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
) -> StdResult<Uint128> {
    let res = query_terraworld_reward_info(deps, terraworld_staking, staker, None)?;
    Ok(res.bond_amount)
}
