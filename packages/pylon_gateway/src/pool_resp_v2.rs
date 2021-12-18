use cosmwasm_std::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::time_range::TimeRange;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub token: String,
    // pool
    pub share_token: String,
    pub deposit_time: Vec<TimeRange>,
    pub withdraw_time: Vec<TimeRange>,
    pub deposit_cap_strategy: Option<String>,
    // reward
    pub reward_token: String,
    pub reward_rate: Decimal,
    pub reward_claim_time: Vec<TimeRange>,
    pub reward_distribution_time: TimeRange,
}
