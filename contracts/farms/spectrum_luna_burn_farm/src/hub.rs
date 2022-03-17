use cosmwasm_std::{Decimal, QuerierWrapper, QueryRequest, StdResult, to_binary, Uint128, WasmQuery};
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
    CurrentBatch {},
    WithdrawableUnbonded {
        address: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct HubState {
    #[serde(default)] pub exchange_rate: Decimal,           // prism/stader
    #[serde(default)] pub bluna_exchange_rate: Decimal,     // bluna
    #[serde(default)] pub stluna_exchange_rate: Decimal,    // stluna
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CurrentBatchResponse {
    pub id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct WithdrawableUnbondedResponse {
    pub withdrawable: Uint128,
}

pub fn query_hub_state(querier: &QuerierWrapper, contract_addr: String) -> StdResult<HubState> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&HubQueryMsg::State {})?,
    }))
}

pub fn query_hub_current_batch(querier: &QuerierWrapper, contract_addr: String) -> StdResult<CurrentBatchResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&HubQueryMsg::CurrentBatch {})?,
    }))
}
