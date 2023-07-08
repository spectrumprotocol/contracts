#![allow(clippy::needless_return)]

use cosmwasm_std::{Decimal256, Uint256};
use cosmwasm_std::{
    to_binary, Addr, BalanceResponse, BankQuery, Binary, Deps, QuerierWrapper, QueryRequest,
    StdResult, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use cw20_base::state::TokenInfo;

use crate::concat;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use classic_bindings::TerraQuery;

pub fn query_balance(
    querier: &QuerierWrapper<TerraQuery>,
    account_addr: &Addr,
    denom: String,
) -> StdResult<Uint128> {
    let balance: BalanceResponse = querier.query(&QueryRequest::Bank(BankQuery::Balance {
        address: account_addr.to_string(),
        denom,
    }))?;
    Ok(balance.amount.amount)
}

// ====================================================================================

pub fn query_token_balance(deps: Deps<TerraQuery>, contract_addr: &Addr, account_addr: &Addr) -> Uint128 {
    if let Ok(balance) = query_token_balance_legacy(&deps, contract_addr, account_addr) {
        return balance;
    }

    if let Ok(balance) = query_token_balance_new(&deps, contract_addr, account_addr) {
        return balance;
    }

    return Uint128::zero();
}

fn query_token_balance_new(
    deps: &Deps<TerraQuery>,
    contract_addr: &Addr,
    account_addr: &Addr,
) -> StdResult<Uint128> {
    // load balance form the cw20 token contract version 0.6+
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.to_string(),
        key: Binary::from(concat(
            &to_length_prefixed(b"balance"),
            account_addr.as_bytes(),
        )),
    }))
}

fn query_token_balance_legacy(
    deps: &Deps<TerraQuery>,
    contract_addr: &Addr,
    account_addr: &Addr,
) -> StdResult<Uint128> {
    // load balance form the cw20 token contract version 0.2.x
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.to_string(),
        key: Binary::from(concat(
            &to_length_prefixed(b"balance"),
            (deps.api.addr_canonicalize(account_addr.as_str())?).as_slice(),
        )),
    }))
}

// ====================================================================================

pub fn query_supply(querier: &QuerierWrapper<TerraQuery>, contract_addr: &Addr) -> StdResult<Uint128> {
    if let Ok(supply) = query_supply_legacy(querier, contract_addr) {
        return Ok(supply);
    }

    return query_supply_new(querier, contract_addr);
}

fn query_supply_new(querier: &QuerierWrapper<TerraQuery>, contract_addr: &Addr) -> StdResult<Uint128> {
    let token_info: TokenInfo = querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.to_string(),
        key: Binary::from(b"token_info"),
    }))?;

    Ok(token_info.total_supply)
}

fn query_supply_legacy(querier: &QuerierWrapper<TerraQuery>, contract_addr: &Addr) -> StdResult<Uint128> {
    let token_info: TokenInfo = querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.to_string(),
        key: Binary::from(to_length_prefixed(b"token_info")),
    }))?;

    Ok(token_info.total_supply)
}

// ====================================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorMarketMsg {
    ClaimRewards {
        to: Option<String>,
    },
    DepositStable {},
    BorrowStable {
        borrow_amount: Uint256,
        to: Option<String>,
    },
    RepayStable {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorMarketCw20Msg {
    /// Return stable coins to a user
    /// according to exchange rate
    RedeemStable {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorMarketQueryMsg {
    EpochState {
        block_height: Option<u64>,
    },
    BorrowerInfo {
        borrower: String,
        block_height: Option<u64>,
    },
    // using Raw query to ask for state
    // State { block_height: Option<u64> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AnchorMarketEpochStateResponse {
    pub exchange_rate: Decimal256,
    pub aterra_supply: Uint256,
}

pub fn query_aterra_state(
    deps: Deps<TerraQuery>,
    anchor_market_contract: &Addr,
) -> StdResult<AnchorMarketEpochStateResponse> {
    let epoch_state: AnchorMarketEpochStateResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: anchor_market_contract.to_string(),
            msg: to_binary(&AnchorMarketQueryMsg::EpochState { block_height: None })?,
        }))?;

    Ok(epoch_state)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorOverseerMsg {
    LockCollateral { collaterals: Vec<(String, Uint256)> },
    UnlockCollateral { collaterals: Vec<(String, Uint256)> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorCustodyMsg {
    WithdrawCollateral { amount: Option<Uint256> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorCustodyCw20Msg {
    DepositCollateral {},
}
