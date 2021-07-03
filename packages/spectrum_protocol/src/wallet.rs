use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::gov::VoteOption;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: HumanAddr,
    pub spectrum_token: HumanAddr,
    pub spectrum_gov: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum HandleMsg {
    poll_votes {
        poll_id: u64,
        vote: VoteOption,
        amount: Uint128,
    },
    update_config {
        owner: Option<HumanAddr>,
    },
    upsert_share {
        address: HumanAddr,
        weight: u32,
    },
    withdraw {
        amount: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    balance { address: HumanAddr, height: u64 },
    config {},
    state {},
    shares {},
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct BalanceResponse {
    pub balance: Uint128,
    pub share: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub total_weight: u32,
    pub previous_share: Uint128,
    pub share_index: Decimal,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct ShareInfo {
    pub address: HumanAddr,
    pub weight: u32,
    pub share_index: Decimal,
    pub share: Uint128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct SharesResponse {
    pub shares: Vec<ShareInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
