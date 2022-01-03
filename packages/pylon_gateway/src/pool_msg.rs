use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;
use pylon_utils::common::OrderBy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::time_range::TimeRange;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub pool_token_code_id: Option<u64>,
    // pool
    pub share_token: String,
    pub deposit_time: Vec<TimeRange>,
    pub withdraw_time: Vec<TimeRange>,
    pub deposit_cap_strategy: Option<String>,
    // reward
    pub reward_token: String,
    pub reward_amount: Uint128, // without decimal
    pub reward_claim_time: Vec<TimeRange>,
    pub reward_distribution_time: TimeRange,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfigureMsg {
    Config {
        owner: Option<String>,
        share_token: Option<String>,
        reward_token: Option<String>,
        claim_time: Option<Vec<TimeRange>>,
        deposit_time: Option<Vec<TimeRange>>,
        withdraw_time: Option<Vec<TimeRange>>,
        deposit_cap_strategy: Option<String>,
    },
    SubReward {
        amount: Uint128,
    },
    AddReward {
        amount: Uint128,
    },
    AddPoolToken {
        code_id: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // core
    Receive(Cw20ReceiveMsg),
    Update {
        target: Option<String>,
    },
    Withdraw {
        amount: Uint128,
    },
    Claim {
        target: Option<String>,
    },
    // internal
    TransferInternal {
        owner: String,
        recipient: String,
        amount: Uint128,
    },
    DepositInternal {
        sender: String,
        amount: Uint128,
    },
    WithdrawInternal {
        sender: String,
        amount: Uint128,
    },
    ClaimInternal {
        sender: String,
    },
    // owner
    Configure(ConfigureMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Deposit {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // v1
    Config {},
    BalanceOf {
        owner: String,
    },
    ClaimableReward {
        owner: String,
        timestamp: Option<u64>,
    },
    AvailableCapOf {
        address: String,
    },

    // v2
    ConfigV2 {},

    // common
    Reward {},
    Staker {
        address: String,
    },
    Stakers {
        start_after: Option<String>,
        limit: Option<u32>,
        order: Option<OrderBy>,
    },
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
