#![allow(clippy::large_enum_variant)]

use cosmwasm_std::{to_binary, Addr, Deps, QueryRequest, StdResult, WasmQuery};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal256, Uint256};
use classic_bindings::TerraQuery;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub governance_contract_addr: String,
    pub oracle_contract_addr: String,
    pub basset_token_addr: String,
    pub stable_denom: String,
    pub borrow_ltv_max: Decimal256,
    pub borrow_ltv_min: Decimal256,
    pub borrow_ltv_aim: Decimal256,
    pub basset_max_ltv: Decimal256,
    pub buffer_part: Decimal256,
    pub price_timeframe: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Governance { governance_msg: GovernanceMsg },
    Anyone { anyone_msg: AnyoneMsg },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnyoneMsg {
    AcceptGovernance {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceMsg {
    UpdateConfig {
        oracle_addr: Option<String>,
        basset_token_addr: Option<String>,
        stable_denom: Option<String>,
        borrow_ltv_max: Option<Decimal256>,
        borrow_ltv_min: Option<Decimal256>,
        borrow_ltv_aim: Option<Decimal256>,
        basset_max_ltv: Option<Decimal256>,
        buffer_part: Option<Decimal256>,
        price_timeframe: Option<u64>,
    },
    UpdateGovernanceContract {
        gov_addr: String,
        //how long to wait for 'AcceptGovernance' transaction
        seconds_to_wait_for_accept_gov_tx: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    BorrowerAction {
        borrowed_amount: Uint256,
        locked_basset_amount: Uint256,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub governance_contract: String,
    pub oracle_contract: String,
    pub basset_token: String,
    pub stable_denom: String,
    pub borrow_ltv_max: Decimal256,
    pub borrow_ltv_min: Decimal256,
    pub borrow_ltv_aim: Decimal256,
    pub basset_max_ltv: Decimal256,
    pub buffer_part: Decimal256,
    pub price_timeframe: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum BorrowerActionResponse {
    Nothing {},
    Borrow {
        amount: Uint256,
        advised_buffer_size: Uint256,
    },
    Repay {
        amount: Uint256,
        advised_buffer_size: Uint256,
    },
}

impl BorrowerActionResponse {
    pub fn repay(amount: Uint256, advised_buffer_size: Uint256) -> Self {
        BorrowerActionResponse::Repay {
            amount,
            advised_buffer_size,
        }
    }

    pub fn borrow(amount: Uint256, advised_buffer_size: Uint256) -> Self {
        BorrowerActionResponse::Borrow {
            amount,
            advised_buffer_size,
        }
    }

    pub fn nothing() -> Self {
        BorrowerActionResponse::Nothing {}
    }
}

pub fn query_borrower_action(
    deps: Deps<TerraQuery>,
    basset_vault_strategy_contract: &Addr,
    borrowed_amount: Uint256,
    locked_basset_amount: Uint256,
) -> StdResult<BorrowerActionResponse> {
    let borrower_action: BorrowerActionResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: basset_vault_strategy_contract.to_string(),
            msg: to_binary(&QueryMsg::BorrowerAction {
                borrowed_amount,
                locked_basset_amount,
            })?,
        }))?;

    Ok(borrower_action)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
