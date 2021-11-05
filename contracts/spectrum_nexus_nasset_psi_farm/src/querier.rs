use cosmwasm_std::{to_binary, CanonicalAddr, Deps, QueryRequest, StdResult, Uint128, WasmQuery};

use nexus_token::staking::{QueryMsg as NexusStakingQueryMsg, StakerInfoResponse};
use terraswap::{
    asset::AssetInfo,
    router::{QueryMsg as TerraswapRouterQueryMsg, SimulateSwapOperationsResponse, SwapOperation},
};

use crate::state::read_config;

pub fn query_nexus_reward_info(
    deps: Deps,
    nasset_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    time_seconds: Option<u64>,
) -> StdResult<StakerInfoResponse> {
    let res: StakerInfoResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.addr_humanize(nasset_staking)?.to_string(),
        msg: to_binary(&NexusStakingQueryMsg::StakerInfo {
            staker: deps.api.addr_humanize(staker)?.to_string(),
            time_seconds,
        })?,
    }))?;

    Ok(res)
}

pub fn query_nexus_pool_balance(
    deps: Deps,
    nasset_staking: &CanonicalAddr,
    staker: &CanonicalAddr,
    time_seconds: u64,
) -> StdResult<Uint128> {
    let res = query_nexus_reward_info(deps, nasset_staking, staker, Some(time_seconds))?;
    Ok(res.bond_amount)
}

pub fn simulate_swap_operations(
    deps: Deps,
    offer_amount: Uint128,
) -> StdResult<SimulateSwapOperationsResponse> {
    let config = read_config(deps.storage)?;
    let res: SimulateSwapOperationsResponse = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.terraswap_router.to_string(),
            msg: to_binary(&TerraswapRouterQueryMsg::SimulateSwapOperations {
                offer_amount,
                operations: vec![
                    SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::Token {
                            contract_addr: config.nexus_token.to_string(),
                        },
                        ask_asset_info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                    },
                    SwapOperation::TerraSwap {
                        offer_asset_info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        ask_asset_info: AssetInfo::Token {
                            contract_addr: config.spectrum_token.to_string(),
                        },
                    },
                ],
            })?,
        }))
        .unwrap();
    Ok(res)
}
