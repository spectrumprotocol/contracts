use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, Uint128};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MockGovStakerInfoResponse {
    pub bond_amount: Uint128,
    pub pending_reward: Uint128,
    pub reward_index: Decimal,
    pub staker: String,
}
