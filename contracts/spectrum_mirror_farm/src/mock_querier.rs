#![allow(non_camel_case_types)]
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Api, CanonicalAddr, Coin, Empty, Extern, HumanAddr,
    Querier, QuerierResult, QueryRequest, SystemError, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use std::collections::HashMap;

use mirror_protocol::gov::StakerResponse as MirrorStakerResponse;
use mirror_protocol::staking::{
    RewardInfoResponse as MirrorRewardInfoResponse,
    RewardInfoResponseItem as MirrorRewardInfoResponseItem,
};
use spectrum_protocol::gov::BalanceResponse as SpecBalanceResponse;

const MIR_STAKING: &str = "mir_staking";

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
    base: MockQuerier<Empty>,
    token_querier: TokenQuerier,
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

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
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
    RewardInfo {
        staker_addr: HumanAddr,
        asset_token: Option<HumanAddr>,
    },
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
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
                        Ok(to_binary(&MirrorStakerResponse {
                            balance,
                            share: balance
                                .multiply_ratio(100u128, self.token_querier.balance_percent),
                            locked_balance: vec![],
                            pending_voting_rewards: Uint128::zero(),
                            withdrawable_polls: vec![],
                        }))
                    }
                    MockQueryMsg::RewardInfo {
                        staker_addr,
                        asset_token,
                    } => {
                        let contract_addr = &HumanAddr::from(MIR_STAKING);
                        match asset_token {
                            Some(asset_token) => {
                                let balance =
                                    self.read_token_balance(contract_addr, asset_token.clone());
                                Ok(to_binary(&MirrorRewardInfoResponse {
                                    staker_addr,
                                    reward_infos: vec![MirrorRewardInfoResponseItem {
                                        asset_token: asset_token.clone(),
                                        pending_reward: balance,
                                        bond_amount: balance,
                                        is_short: false,
                                    }],
                                }))
                            }
                            None => Ok(to_binary(&MirrorRewardInfoResponse {
                                staker_addr: staker_addr.clone(),
                                reward_infos: self
                                    .token_querier
                                    .balances
                                    .get(contract_addr)
                                    .unwrap_or(&HashMap::new())
                                    .into_iter()
                                    .map(|(asset_token, balance)| MirrorRewardInfoResponseItem {
                                        asset_token: asset_token.clone(),
                                        pending_reward: *balance,
                                        bond_amount: *balance,
                                        is_short: false,
                                    })
                                    .collect(),
                            })),
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
    pub fn new<A: Api>(base: MockQuerier<Empty>, canonical_length: usize, _api: A) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
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
}
