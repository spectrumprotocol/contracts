use crate::terraswap::AssetInfo;
use cosmwasm_std::Binary;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairType {
    Xyk {},
    Stable {},
    Custom(String),
}

// Copypasted from terraswap
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// CreatePair instantiates pair contract
    CreatePair {
        /// Type of pair contract
        pair_type: PairType,
        /// Asset infos
        asset_infos: [AssetInfo; 2],
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
    },
}
