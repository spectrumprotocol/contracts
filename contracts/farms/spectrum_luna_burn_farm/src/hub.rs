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
    Parameters {},
    WithdrawableUnbonded {
        address: String,
    },
    AllHistory {
        start_from: Option<u64>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct HubState {
    #[serde(default)] pub exchange_rate: Decimal,           // prism/stader
    #[serde(default)] pub bluna_exchange_rate: Decimal,     // bluna
    #[serde(default)] pub stluna_exchange_rate: Decimal,    // stluna
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Parameters {
    pub peg_recovery_fee: Decimal,
    pub er_threshold: Decimal,
    pub unbonding_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CurrentBatchResponse {
    pub id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct WithdrawableUnbondedResponse {
    pub withdrawable: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllHistoryResponse {
    pub history: Vec<UnbondHistoryResponse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondHistoryResponse {
    pub batch_id: u64,
    pub time: u64,
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

pub fn query_hub_parameters(querier: &QuerierWrapper, contract_addr: String) -> StdResult<Parameters> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&HubQueryMsg::Parameters {})?,
    }))
}

pub fn query_hub_histories(querier: &QuerierWrapper, contract_addr: String, batch_id: u64) -> StdResult<AllHistoryResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&HubQueryMsg::AllHistory {
            start_from: Some(batch_id - 1u64),
            limit: None,
        })?,
    }))
}

pub fn query_hub_claimable(querier: &QuerierWrapper, contract_addr: String, address: String) -> StdResult<WithdrawableUnbondedResponse> {
    querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&HubQueryMsg::WithdrawableUnbonded {
            address
        })?,
    }))
}
