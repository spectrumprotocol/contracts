#![allow(non_camel_case_types)]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Api, CanonicalAddr, Coin, Decimal, Extern, HumanAddr,
    Querier, QuerierResult, QueryRequest, SystemError, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use std::collections::HashMap;
use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::SimulationResponse;

use anchor_token::gov::StakerResponse as AnchorStakerResponse;
use anchor_token::staking::StakerInfoResponse as AnchorStakerInfoResponse;
use spectrum_protocol::gov::BalanceResponse as SpecBalanceResponse;

const ANC_STAKING: &str = "anc_staking";

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn mock_dependencies(
    canonical_length: usize,
    contract_balance: &[Coin],
) -> Extern<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new(
        MockQuerier::new(&[(&contract_addr, contract_balance)]),
        canonical_length,
        MockApi::new(canonical_length),
    );

    Extern {
        storage: MockStorage::default(),
        api: MockApi::new(canonical_length),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    token_querier: TokenQuerier,
    tax_querier: TaxQuerier,
    terraswap_factory_querier: TerraswapFactoryQuerier,
    canonical_length: usize,
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // this lets us iterate over all pairs that match the first string
    balances: HashMap<HumanAddr, HashMap<HumanAddr, Uint128>>,
    balance_percent: u128,
}

impl TokenQuerier {
    pub fn new(
        balances: &[(&HumanAddr, &[(&HumanAddr, &Uint128)])],
        balance_percent: u128,
    ) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
            balance_percent,
        }
    }
}

pub(crate) fn balances_to_map(
    balances: &[(&HumanAddr, &[(&HumanAddr, &Uint128)])],
) -> HashMap<HumanAddr, HashMap<HumanAddr, Uint128>> {
    let mut balances_map: HashMap<HumanAddr, HashMap<HumanAddr, Uint128>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<HumanAddr, Uint128> = HashMap::new();
        for (addr, balance) in balances.iter() {
            contract_balances_map.insert(HumanAddr::from(addr), **balance);
        }

        balances_map.insert(HumanAddr::from(contract_addr), contract_balances_map);
    }
    balances_map
}

#[derive(Clone, Default)]
pub struct TaxQuerier {
    rate: Decimal,
    // this lets us iterate over all pairs that match the first string
    caps: HashMap<String, Uint128>,
}

impl TaxQuerier {
    pub fn new(rate: Decimal, caps: &[(&String, &Uint128)]) -> Self {
        TaxQuerier {
            rate,
            caps: caps_to_map(caps),
        }
    }
}

pub(crate) fn caps_to_map(caps: &[(&String, &Uint128)]) -> HashMap<String, Uint128> {
    let mut owner_map: HashMap<String, Uint128> = HashMap::new();
    for (denom, cap) in caps.iter() {
        owner_map.insert(denom.to_string(), **cap);
    }
    owner_map
}

#[derive(Clone, Default)]
pub struct TerraswapFactoryQuerier {
    pairs: HashMap<String, PairInfo>,
}

impl TerraswapFactoryQuerier {
    pub fn new(pairs: &[(&String, &PairInfo)]) -> Self {
        TerraswapFactoryQuerier {
            pairs: pairs_to_map(pairs),
        }
    }
}

pub(crate) fn pairs_to_map(pairs: &[(&String, &PairInfo)]) -> HashMap<String, PairInfo> {
    let mut pairs_map: HashMap<String, PairInfo> = HashMap::new();
    for (key, pair) in pairs.iter() {
        pairs_map.insert(key.to_string(), (*pair).clone());
    }
    pairs_map
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MockQueryMsg {
    balance {
        address: HumanAddr,
        height: u64,
    },
    Staker {
        address: HumanAddr,
    },
    StakerInfo {
        staker: HumanAddr,
        block_height: Option<u64>,
    },
    Pair {
        asset_infos: [AssetInfo; 2],
    },
    Simulation {
        offer_asset: Asset,
    },
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse {
                                rate: self.tax_querier.rate,
                            };
                            Ok(to_binary(&res))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom)
                                .copied()
                                .unwrap_or_default();
                            let res = TaxCapResponse { cap };
                            Ok(to_binary(&res))
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                match from_binary(&msg).unwrap() {
                    MockQueryMsg::balance { address, height: _ } => {
                        let balance = self.read_token_balance(contract_addr, address);
                        Ok(to_binary(&SpecBalanceResponse {
                            balance,
                            share: balance
                                .multiply_ratio(100u128, self.token_querier.balance_percent),
                            locked_balance: vec![],
                        }))
                    }
                    MockQueryMsg::Staker { address } => {
                        let balance = self.read_token_balance(contract_addr, address);
                        Ok(to_binary(&AnchorStakerResponse {
                            balance,
                            share: balance
                                .multiply_ratio(100u128, self.token_querier.balance_percent),
                            locked_balance: vec![],
                        }))
                    }
                    MockQueryMsg::StakerInfo {
                        staker,
                        block_height: _,
                    } => {
                        let contract_addr = &HumanAddr::from(ANC_STAKING);
                        let balance = self.read_token_balance(contract_addr, staker.clone());
                        Ok(to_binary(&AnchorStakerInfoResponse {
                            staker,
                            reward_index: Decimal::zero(),
                            bond_amount: balance,
                            pending_reward: balance,
                        }))
                    }
                    MockQueryMsg::Pair { asset_infos } => {
                        let key = asset_infos[0].to_string() + asset_infos[1].to_string().as_str();
                        match self.terraswap_factory_querier.pairs.get(&key) {
                            Some(v) => Ok(to_binary(&v)),
                            None => Err(SystemError::InvalidRequest {
                                error: "No pair info exists".to_string(),
                                request: msg.as_slice().into(),
                            }),
                        }
                    }
                    MockQueryMsg::Simulation { offer_asset } => {
                        let commission_amount = offer_asset.amount.multiply_ratio(3u128, 1000u128);
                        let return_amount = offer_asset.amount - commission_amount;
                        match return_amount.into() {
                            Ok(amount) => Ok(to_binary(&SimulationResponse {
                                return_amount: amount,
                                commission_amount,
                                spread_amount: Uint128::from(100u128),
                            })),
                            Err(_e) => return Err(SystemError::Unknown {}),
                        }
                    }
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                let key = key.as_slice();
                let prefix_balance = to_length_prefixed(b"balance").to_vec();
                if key[..prefix_balance.len()].to_vec() != prefix_balance {
                    panic!("DO NOT ENTER HERE");
                }
                let key_address = &key[prefix_balance.len()..];
                let address_raw = CanonicalAddr::from(key_address);
                let api = MockApi::new(self.canonical_length);
                let address = api.human_address(&address_raw).unwrap();

                Ok(to_binary(
                    &to_binary(&self.read_token_balance(contract_addr, address)).unwrap(),
                ))
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new<A: Api>(
        base: MockQuerier<TerraQueryWrapper>,
        canonical_length: usize,
        _api: A,
    ) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
            tax_querier: TaxQuerier::default(),
            terraswap_factory_querier: TerraswapFactoryQuerier::default(),
            canonical_length,
        }
    }

    pub fn with_balance_percent(&mut self, balance_percent: u128) {
        self.token_querier.balance_percent = balance_percent;
    }

    // configure the mint whitelist mock querier
    pub fn with_token_balances(&mut self, balances: &[(&HumanAddr, &[(&HumanAddr, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances, self.token_querier.balance_percent);
    }

    pub fn read_token_balance(&self, contract_addr: &HumanAddr, address: HumanAddr) -> Uint128 {
        let balances: &HashMap<HumanAddr, Uint128> =
            match self.token_querier.balances.get(contract_addr) {
                Some(balances) => balances,
                None => return Uint128::zero(),
            };

        match balances.get(&address) {
            Some(v) => *v,
            None => Uint128::zero(),
        }
    }

    // configure the token owner mock querier
    pub fn with_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        self.tax_querier = TaxQuerier::new(rate, caps);
    }

    // configure the terraswap pair
    pub fn with_terraswap_pairs(&mut self, pairs: &[(&String, &PairInfo)]) {
        self.terraswap_factory_querier = TerraswapFactoryQuerier::new(pairs);
    }
}
