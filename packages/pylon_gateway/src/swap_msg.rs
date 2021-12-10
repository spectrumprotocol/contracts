use cosmwasm_bignumber::{Decimal256, Uint256};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    Lockup {
        release_time: u64,
        release_amount: Decimal256,
    },
    Vesting {
        release_start_time: u64,
        release_finish_time: u64,
        release_amount: Decimal256,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub beneficiary: String,
    pub pool_x_denom: String,
    pub pool_y_addr: String,
    pub pool_liq_x: Uint256,
    pub pool_liq_y: Uint256, // is also a maximum cap of this pool
    pub price: Decimal256,
    pub cap_strategy: Option<String>,
    pub distribution_strategy: Vec<Strategy>,
    pub whitelist_enabled: bool,
    pub swap_pool_size: Uint256,
    pub start: u64,
    pub period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfigureMsg {
    Swap {
        owner: Option<String>,
        beneficiary: Option<String>,
        cap_strategy: Option<String>,
        whitelist_enabled: Option<bool>,
    },
    Pool {
        x_denom: Option<String>,
        y_addr: Option<String>,
        liq_x: Option<Uint256>,
        liq_y: Option<Uint256>,
    },
    Whitelist {
        whitelist: bool,
        candidates: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Configure(ConfigureMsg),
    Deposit {},
    Withdraw { amount: Uint256 },
    Claim {},
    Earn {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    BalanceOf {
        owner: String,
    },
    IsWhitelisted {
        address: String,
    },
    AvailableCapOf {
        address: String,
    },
    ClaimableTokenOf {
        address: String,
    },
    TotalSupply {},
    CurrentPrice {},
    SimulateWithdraw {
        amount: Uint256,
        address: Option<String>,
    },
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrateMsg {
    Refund {},
    General {},
}
