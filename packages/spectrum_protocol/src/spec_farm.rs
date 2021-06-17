use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: HumanAddr,
    pub spectrum_token: HumanAddr,
    pub spectrum_gov: HumanAddr,
    pub lock_start: u64,
    pub lock_end: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum HandleMsg {
    receive(Cw20ReceiveMsg),
    register_asset {
        asset_token: HumanAddr,
        staking_token: HumanAddr,
        weight: u32,
    },
    unbond {
        asset_token: HumanAddr,
        amount: Uint128,
    },
    update_config {
        owner: Option<HumanAddr>,
        lock_start: Option<u64>,
        lock_end: Option<u64>,
    },
    withdraw {
        asset_token: Option<HumanAddr>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    bond {
        staker_addr: Option<HumanAddr>,
        asset_token: HumanAddr,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {},
    pools {},
    reward_info {
        staker_addr: HumanAddr,
        asset_token: Option<HumanAddr>,
        height: u64,
    },
    state {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolsResponse {
    pub pools: Vec<PoolItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolItem {
    pub asset_token: HumanAddr,
    pub staking_token: HumanAddr,
    pub total_bond_amount: Uint128,
    pub weight: u32,
    pub state_spec_share_index: Decimal,
    pub spec_share_index: Decimal,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: HumanAddr,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: HumanAddr,
    pub bond_amount: Uint128,
    pub pending_spec_reward: Uint128,
    pub spec_share: Uint128,
    pub accum_spec_share: Uint128,
    pub locked_spec_reward: Uint128,
    pub locked_spec_share: Uint128,
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
