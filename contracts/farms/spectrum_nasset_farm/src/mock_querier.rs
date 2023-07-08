#![allow(non_camel_case_types)]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Coin, ContractResult, Decimal, OwnedDeps, Querier,
    QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use spectrum_protocol::gov_proxy::StakerResponse;
use std::collections::HashMap;
use classic_bindings::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute};
use classic_terraswap::asset::{Asset, AssetInfo, PairInfo};
use classic_terraswap::pair::{PoolResponse, SimulationResponse};
use classic_terraswap::router::{SimulateSwapOperationsResponse, SwapOperation};
use basset_vault::nasset_token_rewards::{AccruedRewardsResponse};
use spectrum_protocol::gov::BalanceResponse as SpecBalanceResponse;


/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery> {
    let contract_addr = MOCK_CONTRACT_ADDR.to_string();
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(&contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
        custom_query_type: Default::default(),
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    token_querier: TokenQuerier,
    tax_querier: TaxQuerier,
    terraswap_factory_querier: TerraswapFactoryQuerier,
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // this lets us iterate over all pairs that match the first String
    balances: HashMap<String, HashMap<String, Uint128>>,
    balance_percent: u128,
}

impl TokenQuerier {
    pub fn new(balances: &[(&String, &[(&String, &Uint128)])], balance_percent: u128) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
            balance_percent,
        }
    }
}

pub(crate) fn balances_to_map(
    balances: &[(&String, &[(&String, &Uint128)])],
) -> HashMap<String, HashMap<String, Uint128>> {
    let mut balances_map: HashMap<String, HashMap<String, Uint128>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<String, Uint128> = HashMap::new();
        for (addr, balance) in balances.iter() {
            contract_balances_map.insert(addr.to_string(), **balance);
        }

        balances_map.insert(contract_addr.to_string(), contract_balances_map);
    }
    balances_map
}

#[derive(Clone, Default)]
pub struct TaxQuerier {
    rate: Decimal,
    // this lets us iterate over all pairs that match the first String
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
#[serde(rename_all = "snake_case")]
enum MockQueryMsg {
    Balance {
        address: String,
    },
    AccruedRewards {
        address: String
    },
    Pair {
        asset_infos: [AssetInfo; 2],
    },
    Simulation {
        offer_asset: Asset,
    },
    SimulateSwapOperations {
        offer_amount: Uint128,
        operations: Vec<SwapOperation>,
    },
    Pool {},
    Staker {
        address: String,
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
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom.as_str())
                                .copied()
                                .unwrap_or_default();
                            let res = TaxCapResponse { cap };
                            SystemResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                match from_binary(msg).unwrap() {
                    MockQueryMsg::Balance { address } => {
                        let balance = self.read_token_balance(contract_addr, address);
                        SystemResult::Ok(ContractResult::from(to_binary(&SpecBalanceResponse {
                            balance,
                            share: balance
                                .multiply_ratio(100u128, self.token_querier.balance_percent),
                            locked_balance: vec![],
                            pools: vec![],
                        })))
                    }
                    MockQueryMsg::AccruedRewards { address } => {
                        let balance = self.read_token_balance(contract_addr, address);
                        SystemResult::Ok(ContractResult::from(to_binary(&AccruedRewardsResponse {
                            rewards: balance,
                        })))
                    }
                    MockQueryMsg::Pair { asset_infos } => {
                        let key = asset_infos[0].to_string() + asset_infos[1].to_string().as_str();
                        match self.terraswap_factory_querier.pairs.get(&key) {
                            Some(v) => SystemResult::Ok(ContractResult::from(to_binary(&v))),
                            None => SystemResult::Err(SystemError::InvalidRequest {
                                error: "No pair info exists".to_string(),
                                request: msg.as_slice().into(),
                            }),
                        }
                    }
                    MockQueryMsg::Simulation { offer_asset } => {
                        let commission_amount = offer_asset.amount.multiply_ratio(3u128, 1000u128);
                        let return_amount = offer_asset.amount.checked_sub(commission_amount);
                        match return_amount {
                            Ok(amount) => SystemResult::Ok(ContractResult::from(to_binary(
                                &SimulationResponse {
                                    return_amount: amount,
                                    commission_amount,
                                    spread_amount: Uint128::from(100u128),
                                },
                            ))),
                            Err(_e) => SystemResult::Err(SystemError::Unknown {}),
                        }
                    }
                    MockQueryMsg::SimulateSwapOperations { offer_amount, operations: _ } => {
                        let commission_amount = offer_amount.multiply_ratio(3u128, 1000u128);
                        let return_amount = offer_amount.checked_sub(commission_amount);
                        match return_amount {
                            Ok(amount) => SystemResult::Ok(ContractResult::from(to_binary(
                                &SimulateSwapOperationsResponse {
                                    amount
                                },
                            ))),
                            Err(_e) => SystemResult::Err(SystemError::Unknown {}),
                        }
                    }
                    MockQueryMsg::Pool {} => {
                        let pair_info = self.terraswap_factory_querier.pairs.iter()
                            .map(|it| it.1)
                            .find(|pair| &pair.contract_addr == contract_addr)
                            .unwrap();
                        SystemResult::Ok(ContractResult::from(to_binary(
                            &PoolResponse {
                                assets: [
                                    Asset {
                                        info: pair_info.asset_infos[0].clone(),
                                        amount: Uint128::from(1_000_000_000000u128),
                                    },
                                    Asset {
                                        info: pair_info.asset_infos[1].clone(),
                                        amount: Uint128::from(1_000_000_000000u128),
                                    }
                                ],
                                total_share: Uint128::from(1_000_000_000000u128),
                            }
                        )))
                    }
                    MockQueryMsg::Staker { address } => {
                        let balance = self.read_token_balance(contract_addr, address);
                        SystemResult::Ok(ContractResult::from(to_binary(&StakerResponse {
                            balance
                        })))
                    }
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
            tax_querier: TaxQuerier::default(),
            terraswap_factory_querier: TerraswapFactoryQuerier::default(),
        }
    }

    pub fn with_balance_percent(&mut self, balance_percent: u128) {
        self.token_querier.balance_percent = balance_percent;
    }

    // configure the mint whitelist mock querier
    pub fn with_token_balances(&mut self, balances: &[(&String, &[(&String, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances, self.token_querier.balance_percent);
    }

    pub fn read_token_balance(&self, contract_addr: &str, address: String) -> Uint128 {
        let balances: &HashMap<String, Uint128> =
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
