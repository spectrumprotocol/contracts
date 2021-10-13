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
        belief_price: Option<Decimal>,
        max_spread: Decimal,
        compound_rate: Option<Decimal>,
    },
    zap_to_bond_hook {
        contract: String,
        bond_asset: Asset,
        asset_token: String,
        staker_addr: String,
        prev_asset_token_amount: Uint128,
        slippage_tolerance: Decimal,
        compound_rate: Option<Decimal>,
    },
    update_config {
        insert_allowlist: Option<Vec<String>>,
        remove_allowlist: Option<Vec<String>>,
    },
    zap_to_unbond_hook {
        staker_addr: String,
        prev_sell_asset: Asset,
        prev_target_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum Cw20HookMsg {
    zap_to_unbond {
        sell_asset: AssetInfo,
        target_asset: AssetInfo,
        belief_price: Option<Decimal>,
        max_spread: Decimal,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
