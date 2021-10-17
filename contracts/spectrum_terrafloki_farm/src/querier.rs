use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery, Uint128};

use terrafloki::staking::{QueryMsg as TerraflokiStakingQueryMsg, StakerInfoResponse};

pub fn query_terrafloki_reward_info(
    deps: Deps,
    terrafloki_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    block_height: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(terrafloki_staking)?.to_string(),
        msg: to_binary(&TerraflokiStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            block_height,
        })?,
    }))?;

    Ok(res)
}

pub fn query_terrafloki_pool_balance(
    deps: Deps,
    terrafloki_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
) -> StdResult<Uint128> {
    let res = query_terrafloki_reward_info(deps, terrafloki_staking, staker, None)?;
    Ok(res.bond_amount)
}
