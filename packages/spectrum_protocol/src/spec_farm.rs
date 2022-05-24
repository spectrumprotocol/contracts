use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub spectrum_token: String,
    pub spectrum_gov: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    receive(Cw20ReceiveMsg),
    register_asset {
        asset_token: String,
        staking_token: String,
        weight: u32,
    },
    unbond {
        asset_token: String,
        amount: Uint128,
    },
    update_config {
        owner: Option<String>,
    },
    withdraw {
        asset_token: Option<String>,
        spec_amount: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    bond {
        staker_addr: Option<String>,
        asset_token: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {},
    pools {},
    reward_info {
        staker_addr: String,
        asset_token: Option<String>,
    },
    state {},
    reward_infos {
        start_after: Option<String> 
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolsResponse {
    pub pools: Vec<PoolItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolItem {
    pub asset_token: String,
    pub staking_token: String,
    pub total_bond_amount: Uint128,
    pub weight: u32,
    pub state_spec_share_index: Decimal,
    pub spec_share_index: Decimal,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: String,
    pub bond_amount: Uint128,
    pub pending_spec_reward: Uint128,
    pub spec_share: Uint128,
    pub spec_share_index: Decimal,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal,
    pub total_weight: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
