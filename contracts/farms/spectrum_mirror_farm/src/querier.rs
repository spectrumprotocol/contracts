use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery};

use mirror_protocol::staking::{PoolInfoResponse, QueryMsg, RewardInfoResponse};

pub fn query_mirror_reward_info(
    deps: Deps,
    mirror_staking: String,
    staker: String,
) -> StdResult<RewardInfoResponse> {
    let res: RewardInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: mirror_staking,
        msg: to_binary(&QueryMsg::RewardInfo {
            asset_token: None,
            staker_addr: staker,
        })?,
    }))?;

    Ok(res)
}

pub fn query_mirror_pool_balance(
    deps: Deps,
    mirror_staking: &CanonicalAddr,
    asset_token: &CanonicalAddr,
    contract_addr: &CanonicalAddr,
) -> StdResult<Uint128> {
    let staker = deps.api.addr_humanize(contract_addr)?;
    let reward_info = query_mirror_reward_info(
        deps,
        deps.api.addr_humanize(mirror_staking)?.to_string(),
        staker.to_string(),
    )?;
    let asset_token = deps.api.addr_humanize(asset_token)?;
    Ok(reward_info
        .reward_infos
        .into_iter()
        .find(|it| it.asset_token == asset_token)
        .map(|it| it.bond_amount)
        .unwrap_or_else(Uint128::zero))
}

pub fn query_mirror_pool_info(
    deps: Deps,
    mirror_staking: String,
    asset_token: String,
) -> StdResult<PoolInfoResponse> {
    let res: PoolInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: mirror_staking,
        msg: to_binary(&QueryMsg::PoolInfo { asset_token })?,
    }))?;

    Ok(res)
}
