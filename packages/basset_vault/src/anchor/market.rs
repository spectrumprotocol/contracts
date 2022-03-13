use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{to_binary, Addr, Binary, Deps, QueryRequest, StdResult, WasmQuery};
use cosmwasm_storage::to_length_prefixed;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::querier::AnchorMarketQueryMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BorrowerInfoResponse {
    pub borrower: String,
    pub loan_amount: Uint256,
    pub pending_rewards: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BorrowerInfo {
    pub loan_amount: Uint256,
    pub pending_rewards: Decimal256,
    // we do not need those fields, removing it will save some space in
    // compiled wasm file
    //
    // pub interest_index: Decimal256,
    // pub reward_index: Decimal256,
}

pub fn query_borrower_info(
    deps: Deps,
    anchor_market_contract: &Addr,
    borrower: &Addr,
) -> StdResult<BorrowerInfoResponse> {
    let borrower_info: StdResult<BorrowerInfo> =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: anchor_market_contract.to_string(),
            msg: to_binary(&AnchorMarketQueryMsg::BorrowerInfo {
                borrower: borrower.to_string(),
                block_height: None,
            })?,
        }));

    match borrower_info {
        Ok(bi) => Ok(BorrowerInfoResponse {
            borrower: borrower.to_string(),
            loan_amount: bi.loan_amount,
            pending_rewards: bi.pending_rewards,
        }),
        Err(_) => Ok(BorrowerInfoResponse {
            borrower: borrower.to_string(),
            loan_amount: Uint256::zero(),
            pending_rewards: Decimal256::zero(),
        }),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub total_liabilities: Decimal256,
    pub total_reserves: Decimal256,
    // we do not need those fields, removing it will save some space in
    // compiled wasm file
    //
    // pub last_interest_updated: u64,
    // pub last_reward_updated: u64,
    // pub global_interest_index: Decimal256,
    // pub global_reward_index: Decimal256,
    // pub anc_emission_rate: Decimal256,
}

pub fn query_market_state(deps: Deps, anchor_market_contract: &Addr) -> StdResult<StateResponse> {
    let market_state: StateResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: anchor_market_contract.to_string(),
        //Anchor use cosmwasm_storage::Singleton which add length prefix
        key: Binary::from(to_length_prefixed(b"state").to_vec()),
    }))?;

    Ok(market_state)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    // we do not need those fields, removing it will save some space in
    // compiled wasm file
    //
    // pub contract_addr: CanonicalAddr,
    // pub owner_addr: CanonicalAddr,
    // pub aterra_contract: CanonicalAddr,
    // pub interest_model: CanonicalAddr,
    // pub distribution_model: CanonicalAddr,
    // pub overseer_contract: CanonicalAddr,
    // pub collector_contract: CanonicalAddr,
    // pub distributor_contract: CanonicalAddr,
    // pub stable_denom: String,
    // pub reserve_factor: Decimal256,
    pub max_borrow_factor: Decimal256,
}

pub fn query_market_config(deps: Deps, anchor_market_contract: &Addr) -> StdResult<ConfigResponse> {
    let market_config: ConfigResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: anchor_market_contract.to_string(),
            //Anchor use cosmwasm_storage::Singleton which add length prefix
            key: Binary::from(to_length_prefixed(b"config").to_vec()),
        }))?;

    Ok(market_config)
}
