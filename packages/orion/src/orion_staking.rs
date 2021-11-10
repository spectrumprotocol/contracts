use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub orion_token: String,
    pub orion_token_decimals: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    UnbondInit {
        amount: Uint128,
    },
    /// Claim pending rewards
    Claim {},
    /// Owner's operation to notify rewards amount and distribution period
    NotifyRewards {
        period_start: u64,
        period_finish: u64,
        amount: Uint128,
        unstaking_period: u64,
    },
    /// Owner's operation to recover unused reward tokens
    RecoverRewards {
        amount: Uint128
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Bond {},
}

/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {
        timestamp: Option<u64>,
    },
    StakerInfo {
        staker: String,
        timestamp: Option<u64>,
    },
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub orion_token: String,
    pub orion_token_decimals: u8,
    pub distribution_schedule: (u64, u64, Uint128),
    pub unstaking_period: u64,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub last_distributed: u64,
    pub total_bond_amount: Uint128,
    pub global_reward_index: Decimal,
    pub leftover: Uint128,
    pub reward_rate_per_token: Decimal,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakerInfoResponse {
    pub staker: String,
    pub reward_index: Decimal,
    pub bond_amount: Uint128,
    pub pending_reward: Uint128,
    pub orders: Vec<(u64, Uint128)>,
}
