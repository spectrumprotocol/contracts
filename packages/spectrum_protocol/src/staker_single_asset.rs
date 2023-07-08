use cosmwasm_std::{Decimal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use classic_terraswap::asset::{Asset, AssetInfo};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub allowlist: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SwapOperation {
    pub pair_contract: String,
    pub asset_info: AssetInfo,
    pub belief_price: Option<Decimal>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    zap_to_bond {
        contract: String,
        provide_asset: Asset,
        swap_operations: Vec<SwapOperation>,
        max_spread: Decimal,
        compound_rate: Option<Decimal>,
    },
    zap_to_bond_hook {
        contract: String,
        prev_asset: Asset,
        staker_addr: String,
        swap_operations: Vec<SwapOperation>,
        max_spread: Decimal,
        compound_rate: Option<Decimal>,
    },
    update_config {
        insert_allowlist: Option<Vec<String>>,
        remove_allowlist: Option<Vec<String>>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum QueryMsg {
    config {}
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
