use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HubExecuteMsg {
    WithdrawUnbonded {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HubCw20HookMsg {
    Unbond {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HubQueryMsg {
    State {},
    WithdrawableUnbonded {
        address: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    #[serde(default)] pub exchange_rate: Decimal,           // prism/stader
    #[serde(default)] pub bluna_exchange_rate: Decimal,     // bluna
    #[serde(default)] pub stluna_exchange_rate: Decimal,    // stluna
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct WithdrawableUnbondedResponse {
    pub withdrawable: Uint128,
}
