use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::gov::VoteOption;
use cw20::Cw20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub spectrum_token: String,
    pub spectrum_gov: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    poll_vote {
        poll_id: u64,
        vote: VoteOption,
        amount: Uint128,
    },
    receive(Cw20ReceiveMsg),
    stake {
        amount: Uint128,
        days: Option<u64>,
    },
    unstake {
        amount: Uint128,
        days: Option<u64>,
    },
    update_config {
        owner: Option<String>,
    },
    update_stake {
        amount: Uint128,
        from_days: u64,
        to_days: u64,
    },
    upsert_share {
        address: String,
        weight: u32,
        lock_start: Option<u64>,
        lock_end: Option<u64>,
        lock_amount: Option<Uint128>,
    },
    withdraw {
        amount: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    deposit {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    balance { address: String },
    config {},
    state {},
    shares {},
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct BalanceResponse {
    pub share: Uint128,
    pub staked_amount: Uint128,
    pub unstaked_amount: Uint128,
    pub locked_amount: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StatePoolInfo {
    pub days: u64,
    pub previous_share: Uint128,
    pub share_index: Decimal,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub total_weight: u32,
    pub previous_share: Uint128,
    pub share_index: Decimal,
    pub pools: Vec<StatePoolInfo>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SharePoolInfo {
    pub days: u64,
    pub share_index: Decimal,
    pub share: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct ShareInfo {
    pub address: String,
    pub weight: u32,
    pub share_index: Decimal,
    pub share: Uint128,
    pub amount: Uint128,
    pub lock_start: u64,
    pub lock_end: u64,
    pub lock_amount: Uint128,
    pub pools: Vec<SharePoolInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct SharesResponse {
    pub shares: Vec<ShareInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
