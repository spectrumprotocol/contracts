use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::time_range::TimeRange;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponseDepositConfig {
    pub time: TimeRange,
    pub user_cap: String,
    pub total_cap: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponseDistributionConfig {
    pub time: TimeRange,
    pub reward_rate: Decimal,
    pub total_reward_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    // pool
    pub share_token: String,
    pub deposit_config: ConfigResponseDepositConfig,
    pub withdraw_time: Vec<TimeRange>,
    // reward
    pub reward_token: String,
    pub claim_time: TimeRange,
    pub distribution_config: ConfigResponseDistributionConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardResponse {
    pub total_deposit: Uint128,
    pub last_update_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BalanceOfResponse {
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AvailableCapOfResponse {
    pub amount: Option<Uint128>,
    pub unlimited: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ClaimableRewardResponse {
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakerResponse {
    pub address: String,
    pub staked: Uint128,
    pub reward: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StakersResponse {
    pub stakers: Vec<StakerResponse>,
}
