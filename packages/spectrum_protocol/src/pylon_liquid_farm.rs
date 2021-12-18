use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub dp_token: String,
    pub reward_token: String,
    pub gov_proxy: Option<String>,
    pub spectrum_token: String,
    pub spectrum_gov: String,
    pub gateway_pool: String,
    pub platform: String,
    pub controller: String,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub deposit_fee: Decimal,
    pub anchor_market: String,
    pub aust_token: String,
    pub pair_contract: String,
    pub ust_pair_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    receive(Cw20ReceiveMsg), // Bond token
    // Update config
    update_config {
        owner: Option<String>,
        controller: Option<String>,
        community_fee: Option<Decimal>,
        platform_fee: Option<Decimal>,
        controller_fee: Option<Decimal>,
        deposit_fee: Option<Decimal>,
    },
    // Unbond lp token
    unbond {
        asset_token: String,
        amount: Uint128,
    },
    register_asset {
        dp_token: String,
        weight: u32,
    },
    // Withdraw rewards
    withdraw {
        // If the asset token is not given, then all rewards are withdrawn
        asset_token: Option<String>,
        spec_amount: Option<Uint128>,
        farm_amount: Option<Uint128>,
    },
    compound {},
    send_fee {},
    update_bond {
        asset_token: String,
        amount_to_stake: Uint128,
        amount_to_auto: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    bond {
        staker_addr: Option<String>,
        compound_rate: Option<Decimal>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {}, // get config
    // get all vault settings
    pools {},
    // get deposited balances
    reward_info {
        staker_addr: String,
    },
    state {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolsResponse {
    pub pools: Vec<PoolItem>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolItem {
    pub asset_token: String, // equivalent to DP Token, name asset_token for frontend easy coding
    pub total_auto_bond_share: Uint128, // share auto bond
    pub total_stake_bond_share: Uint128,
    pub total_stake_bond_amount: Uint128, // amount stake
    pub weight: u32,
    pub farm_share: Uint128, // MINE share
    pub state_spec_share_index: Decimal,
    pub farm_share_index: Decimal,       // per stake bond share
    pub stake_spec_share_index: Decimal, // per stake bond share
    pub auto_spec_share_index: Decimal,  // per auto bond share
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
    pub farm_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub auto_bond_amount: Uint128,
    pub stake_bond_amount: Uint128,
    pub farm_share: Uint128,
    pub spec_share: Uint128,
    pub auto_bond_share: Uint128,
    pub stake_bond_share: Uint128,
    pub pending_farm_reward: Uint128,
    pub pending_spec_reward: Uint128,
    pub deposit_amount: Option<Uint128>,
    pub deposit_time: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub previous_spec_share: Uint128,
    pub spec_share_index: Decimal, // per weight
    pub total_farm_share: Uint128,
    pub total_weight: u32,
    pub earning: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
}
