use std::collections::HashMap;
use cosmwasm_std::{BalanceResponse, BankQuery, Binary, Coin, ContractResult, Decimal, Fraction, from_binary, from_slice, OwnedDeps, Querier, QuerierResult, QueryRequest, StdError, StdResult, SystemError, SystemResult, to_binary, Uint128, WasmQuery};
use cosmwasm_std::testing::{MockApi, MockStorage};
use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper};
use terraswap::pair::SimulationResponse;
use spectrum_protocol::gov::BalancePoolInfo;
use crate::hub::{AllHistoryResponse, CurrentBatchResponse, HubState, Parameters, UnbondHistoryResponse, WithdrawableUnbondedResponse};
use crate::oracle::{AssetInfoUnchecked, AssetPriceResponse};
use crate::stader::{BatchUndelegationRecord, GetFundsClaimRecord, QueryBatchUndelegationResponse, QueryConfigResponse, QueryStateResponse, StaderConfig, StaderState};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub fn mock_dependencies(
    hub_state: HubState,
    hub_parameters: Parameters,
    stader_config: StaderConfig,
    stader_state: StaderState,
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new(
        hub_state,
        hub_parameters,
        stader_config,
        stader_state,
    );

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    balances: HashMap<(String, String), Uint128>,
    prices: HashMap<String, Decimal>,
    hub_state: HubState,
    hub_batch_ids: HashMap<String, u64>,
    hub_parameters: Parameters,
    hub_histories: HashMap<String, AllHistoryResponse>,
    stader_config: StaderConfig,
    stader_state: StaderState,
    stader_batches: HashMap<u64, BatchUndelegationRecord>,
    stader_user_balances: HashMap<(String, u64), Uint128>,
}

impl WasmMockQuerier {
    pub fn new(
        hub_state: HubState,
        hub_parameters: Parameters,
        stader_config: StaderConfig,
        stader_state: StaderState,
    ) -> Self {
        let mut querier = WasmMockQuerier {
            balances: HashMap::new(),
            prices: HashMap::new(),
            hub_state,
            hub_batch_ids: HashMap::new(),
            hub_parameters,
            hub_histories: HashMap::new(),
            stader_config,
            stader_state,
            stader_batches: HashMap::new(),
            stader_user_balances: HashMap::new(),
        };
        querier.hub_batch_ids.insert("anchor_hub".to_string(), 1u64);
        querier.hub_batch_ids.insert("prism_hub".to_string(), 1u64);
        querier.hub_histories.insert("anchor_hub".to_string(), AllHistoryResponse {
            history: vec![],
        });
        querier.hub_histories.insert("prism_hub".to_string(), AllHistoryResponse {
            history: vec![],
        });
        querier
    }

    pub fn set_price(&mut self, token: String, price: Decimal) {
        self.prices.insert(token, price);
    }

    fn get_price(&self, token: String) -> Decimal {
        *self.prices.get(&token).unwrap_or(&Decimal::one())
    }

    pub fn set_balance(&mut self, token: String, addr: String, amount: Uint128) {
        self.balances.insert((token, addr), amount);
    }

    fn get_balance(&self, token: String, addr: String) -> Uint128 {
        *self.balances.get(&(token, addr)).unwrap_or(&Uint128::zero())
    }

    pub fn set_hub_history(&mut self, contract: String, history: UnbondHistoryResponse) {
        self.hub_batch_ids.insert(contract.clone(), history.batch_id + 1u64);
        self.hub_histories.get_mut(&contract).unwrap().history.push(history);
    }

    pub fn set_stader_batch(&mut self, batch_id: u64, batch: BatchUndelegationRecord) {
        self.stader_state.current_undelegation_batch_id = batch_id + 1u64;
        self.stader_batches.insert(batch_id, batch);
    }

    fn get_stader_batch(&self, batch_id: u64) -> Option<BatchUndelegationRecord> {
        self.stader_batches.get(&batch_id).map(|it| it.clone())
    }

    pub fn set_stader_user_balance(&mut self, user_addr: String, batch_id: u64, amount: Uint128) {
        self.stader_user_balances.insert((user_addr, batch_id), amount);
    }

    fn get_stader_user_balance(&self, user_addr: String, batch_id: u64) -> Uint128 {
        *self.stader_user_balances.get(&(user_addr, batch_id)).unwrap_or(&Uint128::zero())
    }

    fn execute_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        let result = match request {
            QueryRequest::Bank(BankQuery::Balance { address, denom }) => {
                let amount = self.get_balance(denom.clone(), address.clone());
                to_binary(&BalanceResponse {
                    amount: Coin { denom: denom.clone(), amount }
                })
            },
            QueryRequest::Custom(TerraQueryWrapper{ query_data, .. }) => {
                match query_data {
                    TerraQuery::TaxCap { .. } => to_binary(&TaxCapResponse {
                        cap: Uint128::from(1_400000u128)
                    }),
                    TerraQuery::TaxRate { } => to_binary(&TaxRateResponse {
                        rate: Decimal::permille(2)
                    }),
                    _ => return QuerierResult::Err(SystemError::Unknown {})
                }
            },
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg })
            => self.execute_wasm_query(contract_addr, msg),
            _ => return QuerierResult::Err(SystemError::Unknown {})
        };
        QuerierResult::Ok(ContractResult::from(result))
    }

    fn execute_wasm_query(&self, contract_addr: &String, msg: &Binary) -> StdResult<Binary> {
        match from_binary(msg)? {
            MockQueryMsg::Balance { address } => {
                let balance = self.get_balance(contract_addr.clone(), address);
                if contract_addr == "spec_gov" {
                    to_binary(&spectrum_protocol::gov::BalanceResponse {
                        balance,
                        share: balance,
                        locked_balance: vec![],
                        pools: vec![
                            BalancePoolInfo {
                                balance,
                                share: balance,
                                days: 30,
                                aust_index: Decimal::zero(),
                                pending_aust: Uint128::zero(),
                                unlock: 0,
                            },
                        ]
                    })
                } else {
                    to_binary(&cw20::BalanceResponse {
                        balance
                    })
                }
            },
            MockQueryMsg::Config { } => {
                if contract_addr == "stader" {
                    to_binary(&QueryConfigResponse {
                        config: self.stader_config.clone(),
                    })
                } else {
                    Err(StdError::not_found(contract_addr))
                }
            },
            MockQueryMsg::State { } => {
                if contract_addr == "stader" {
                    to_binary(&QueryStateResponse {
                        state: self.stader_state.clone(),
                    })
                } else {
                    to_binary(&self.hub_state)
                }
            },
            MockQueryMsg::CurrentBatch {} => to_binary(&CurrentBatchResponse {
                id: *self.hub_batch_ids.get(contract_addr).unwrap(),
            }),
            MockQueryMsg::Parameters {} => to_binary(&self.hub_parameters),
            MockQueryMsg::AllHistory { .. } => to_binary(&self.hub_histories.get(contract_addr).unwrap()),
            MockQueryMsg::WithdrawableUnbonded { address } => {
                let withdrawable = self.get_balance(contract_addr.clone(), address);
                to_binary(&WithdrawableUnbondedResponse {
                    withdrawable,
                })
            },
            MockQueryMsg::Simulation { offer_asset } => {
                let price = self.get_price(contract_addr.to_string());
                let amount = offer_asset.amount * price.inv().unwrap();
                let commission_amount = amount * Decimal::permille(3);
                let return_amount = amount.checked_sub(commission_amount)?;
                to_binary(&SimulationResponse {
                    return_amount,
                    commission_amount,
                    spread_amount: Uint128::zero(),
                })
            },
            MockQueryMsg::AssetPrice { asset_info, .. } => {
                let price = self.get_price(get_oracle_token(asset_info));
                to_binary(&AssetPriceResponse {
                    price,
                    display_price: price,
                })
            },
            MockQueryMsg::BatchUndelegation { batch_id } => {
                let batch = self.get_stader_batch(batch_id);
                to_binary(&QueryBatchUndelegationResponse {
                    batch,
                })
            },
            MockQueryMsg::GetUserUndelegationInfo { batch_id, user_addr } => {
                let balance = self.get_stader_user_balance(user_addr, batch_id);
                to_binary(&GetFundsClaimRecord {
                    user_withdrawal_amount: balance,
                    protocol_fee: Uint128::zero(),
                    undelegated_tokens: balance,
                })
            },
        }
    }
}

fn get_oracle_token(asset_info: AssetInfoUnchecked) -> String {
    match asset_info {
        AssetInfoUnchecked::Native { denom } => denom,
        AssetInfoUnchecked::Cw20 { contract_addr } => contract_addr,
    }
}

// fn get_mock_token(asset_info: &MockAssetInfo) -> String {
//     match asset_info {
//         MockAssetInfo::Token { contract_addr } => contract_addr.clone(),
//         MockAssetInfo::NativeToken { denom } => denom.clone(),
//         MockAssetInfo::Cw20(contract_addr) => contract_addr.clone(),
//         MockAssetInfo::Native(denom) => denom.clone(),
//     }
// }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
struct MockAsset {
    pub amount: Uint128,
    pub info: MockAssetInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MockAssetInfo {
    Token {
        contract_addr: String,
    },
    NativeToken {
        denom: String,
    },
    Cw20(String),
    Native(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MockQueryMsg {
    Balance {
        address: String,
    },
    Config {},
    State {},
    CurrentBatch {},
    Parameters {},
    AllHistory {
        start_from: Option<u64>,
        limit: Option<u32>,
    },
    WithdrawableUnbonded {
        address: String,
    },
    Simulation {
        offer_asset: MockAsset,
    },
    AssetPrice {
        asset_info: AssetInfoUnchecked,
        execute_mode: bool,
    },
    BatchUndelegation {
        batch_id: u64,
    },
    GetUserUndelegationInfo {
        user_addr: String,
        batch_id: u64,
    },
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.execute_query(&request)
    }
}
