use cosmwasm_std::{
    to_binary, Api, CanonicalAddr, Extern, String, Querier, QueryRequest, StdResult, Storage,
    Uint128, WasmQuery,
};

use mirror_protocol::staking::{PoolInfoResponse, QueryMsg, RewardInfoResponse};

pub fn query_mirror_reward_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    mirror_staking: &String,
    staker: &String,
) -> StdResult<RewardInfoResponse> {
    let res: RewardInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: mirror_staking.to_string(),
        msg: to_binary(&QueryMsg::RewardInfo {
            asset_token: None,
            staker_addr: staker.to_string(),
        })?,
    }))?;

    Ok(res)
}

pub fn query_mirror_pool_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    mirror_staking: &CanonicalAddr,
    asset_token: &CanonicalAddr,
    contract_addr: &CanonicalAddr,
) -> StdResult<Uint128> {
    let staker = deps.api.human_address(contract_addr)?;
    let reward_info =
        query_mirror_reward_info(deps, &deps.api.human_address(mirror_staking)?, &staker)?;
    let asset_token = deps.api.human_address(asset_token)?;
    Ok(reward_info
        .reward_infos
        .into_iter()
        .find(|it| it.asset_token == asset_token)
        .map(|it| it.bond_amount)
        .unwrap_or(Uint128::zero()))
}

pub fn query_mirror_pool_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    mirror_staking: &String,
    asset_token: &String,
) -> StdResult<PoolInfoResponse> {
    let res: PoolInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: mirror_staking.to_string(),
        msg: to_binary(&QueryMsg::PoolInfo {
            asset_token: asset_token.to_string(),
        })?,
    }))?;

    Ok(res)
}
