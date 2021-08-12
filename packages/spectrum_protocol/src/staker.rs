use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use terraswap::asset::Asset;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub terraswap_factory: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum ExecuteMsg {
    bond {
        contract: String,
        assets: [Asset; 2],
        slippage_tolerance: Option<Decimal>,
        compound_rate: Option<Decimal>,
    },
    bond_hook {
        contract: String,
        asset_token: String,
        staking_token: String,
        staker_addr: String,
        prev_staking_token_amount: Uint128,
        compound_rate: Option<Decimal>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct QueryMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
