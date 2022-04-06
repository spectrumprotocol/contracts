use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use terraswap::asset::{Asset, AssetInfo};
use cw20::Cw20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub terraswap_factory: String,
    pub allowlist: Vec<String>,
    pub allow_all: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    receive(Cw20ReceiveMsg),
    bond {
        contract: String,
        assets: [Asset; 2],
        slippage_tolerance: Decimal,
        compound_rate: Option<Decimal>,
        staker_addr: Option<String>,
    },
    bond_hook {
        contract: String,
        asset_token: String,
        staking_token: String,
        staker_addr: String,
        prev_staking_token_amount: Uint128,
        compound_rate: Option<Decimal>,
    },
    zap_to_bond {
        contract: String,
        provide_asset: Asset,
        pair_asset: AssetInfo,
        pair_asset_b: Option<AssetInfo>,
        belief_price: Option<Decimal>,
        belief_price_b: Option<Decimal>,
        max_spread: Decimal,
        compound_rate: Option<Decimal>,
    },
    update_config {
        insert_allowlist: Option<Vec<String>>,
        remove_allowlist: Option<Vec<String>>,
        allow_all: Option<bool>,
    },
    zap_to_unbond_hook {
        staker_addr: String,
        prev_asset_a: Asset,
        prev_asset_b: Option<Asset>,
        prev_target_asset: Asset,
        belief_price_a: Option<Decimal>,
        belief_price_b: Option<Decimal>,
        max_spread: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulateZapToBondResponse {
    pub lp_amount: Uint128,
    pub belief_price: Decimal,
    pub belief_price_b: Option<Decimal>,
    pub swap_ust: Uint128,
    pub receive_a: Uint128,
    pub swap_a: Option<Uint128>,
    pub provide_a: Uint128,
    pub provide_b: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    zap_to_unbond {
        sell_asset: AssetInfo,
        sell_asset_b: Option<AssetInfo>,
        target_asset: AssetInfo,
        belief_price: Option<Decimal>,
        belief_price_b: Option<Decimal>,
        max_spread: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {},
    simulate_zap_to_bond {
        provide_asset: Asset,
        pair_asset: AssetInfo,
        pair_asset_b: Option<AssetInfo>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
