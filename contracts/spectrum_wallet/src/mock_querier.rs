use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Coin, ContractResult, Empty, OwnedDeps, Querier,
    QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use spectrum_protocol::gov_v2::{BalanceResponse, StateInfo, StatePoolInfo, BalancePoolInfo};
use std::collections::HashMap;

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(&MOCK_CONTRACT_ADDR, contract_balance)]));

    OwnedDeps {
        api: MockApi::default(),
        storage: MockStorage::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
    token_querier: TokenQuerier,
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // this lets us iterate over all pairs that match the first string
    balances: HashMap<String, HashMap<String, (Uint128, Uint128)>>,
    balance_percent: u128,
}

impl TokenQuerier {
    pub fn new(balances: &[(&String, &[(&String, &Uint128, &Uint128)])], balance_percent: u128) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
            balance_percent,
        }
    }
}

pub(crate) fn balances_to_map(
    balances: &[(&String, &[(&String, &Uint128, &Uint128)])],
) -> HashMap<String, HashMap<String, (Uint128, Uint128)>> {
    let mut balances_map: HashMap<String, HashMap<String, (Uint128, Uint128)>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<String, (Uint128, Uint128)> = HashMap::new();
        for (addr, balance1, balance2) in balances.iter() {
            contract_balances_map.insert(addr.to_string(), (**balance1, **balance2));
        }

        balances_map.insert(contract_addr.to_string(), contract_balances_map);
    }
    balances_map
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[allow(non_camel_case_types)]
pub enum MockQueryMsg {
    balance {
        address: String,
    },
    state {},
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                match from_binary(&msg).unwrap() {
                    MockQueryMsg::balance { address } => {
                        let (balance1, balance2) = self.read_token_balance(contract_addr, address);
                        SystemResult::Ok(ContractResult::from(to_binary(&BalanceResponse {
                            balance: balance1,
                            share: balance1
                                .multiply_ratio(100u128, self.token_querier.balance_percent),
                            locked_balance: vec![],
                            pools: vec![
                                BalancePoolInfo {
                                    days: 0u64,
                                    balance: balance1,
                                    share: balance1
                                        .multiply_ratio(100u128, self.token_querier.balance_percent),
                                    unlock: 0u64,
                                },
                                BalancePoolInfo {
                                    days: 30u64,
                                    balance: balance2,
                                    share: balance2
                                        .multiply_ratio(100u128, self.token_querier.balance_percent),
                                    unlock: 0u64,
                                },
                            ],
                        })))
                    }
                    MockQueryMsg::state { } => {
                        SystemResult::Ok(ContractResult::from(to_binary(&StateInfo {
                            poll_count: 0u64,
                            last_mint: 0u64,
                            poll_deposit: Uint128::zero(),
                            total_weight: 0u32,
                            total_staked: Uint128::from(100u128),
                            prev_balance: Uint128::zero(),
                            pools: vec![
                                StatePoolInfo {
                                    days: 0u64,
                                    total_balance: Uint128::from(self.token_querier.balance_percent),
                                    total_share: Uint128::from(100u128),
                                    active: true,
                                },
                                StatePoolInfo {
                                    days: 30u64,
                                    total_balance: Uint128::from(self.token_querier.balance_percent),
                                    total_share: Uint128::from(100u128),
                                    active: true,
                                },
                            ]
                        })))
                    }
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
        }
    }

    pub fn with_balance_percent(&mut self, balance_percent: u128) {
        self.token_querier.balance_percent = balance_percent;
    }

    // configure the mint whitelist mock querier
    pub fn with_token_balances(&mut self, balances: &[(&String, &[(&String, &Uint128, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances, self.token_querier.balance_percent);
    }

    pub fn read_token_balance(&self, contract_addr: &str, address: String) -> (Uint128, Uint128) {
        let balances: &HashMap<String, (Uint128, Uint128)> =
            match self.token_querier.balances.get(contract_addr) {
                Some(balances) => balances,
                None => return (Uint128::zero(), Uint128::zero()),
            };

        match balances.get(&address) {
            Some(v) => *v,
            None => (Uint128::zero(), Uint128::zero()),
        }
    }
}
