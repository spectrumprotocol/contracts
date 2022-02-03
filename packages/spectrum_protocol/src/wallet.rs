use cosmwasm_std::{Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::gov::VoteOption;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub spectrum_token: String,
    pub spectrum_gov: String,
    pub aust_token: String,
    pub anchor_market: String,
    pub terraswap_factory: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    poll_vote {
        poll_id: u64,
        vote: VoteOption,
        amount: Uint128,
    },
    stake {
        amount: Uint128,
        days: Option<u64>,
    },
    unstake {
        amount: Option<Uint128>,
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
        lock_start: Option<u64>,
        lock_end: Option<u64>,
        lock_amount: Option<Uint128>,
        disable_withdraw: Option<bool>,
    },
    withdraw {
        spec_amount: Option<Uint128>,
        aust_amount: Option<Uint128>,
    },
    gov_claim {
        aust_amount: Option<Uint128>,
        days: Option<u64>,
    },
    burn {
        spec_amount: Option<Uint128>,
    },
    aust_redeem {
        aust_amount: Option<Uint128>,
    },
    buy_spec {
        ust_amount: Option<Uint128>,
    },
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
pub struct StateInfo {
    #[serde(default)] pub total_burn: Uint128,
    #[serde(default)] pub buyback_ust: Uint128,
    #[serde(default)] pub buyback_spec: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct ShareInfo {
    pub address: String,
    pub lock_start: u64,
    pub lock_end: u64,
    pub lock_amount: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct SharesResponse {
    pub shares: Vec<ShareInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub aust_token: String,
    pub anchor_market: String,
    pub terraswap_factory: String,
}
