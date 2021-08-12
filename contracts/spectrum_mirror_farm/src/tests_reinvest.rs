use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{pool_info_read, pool_info_store};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    from_binary, to_binary, Api, Coin, CosmosMsg, Decimal, Extern, String, Uint128, WasmMsg,
};
use cw20::{Cw20HandleMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::mirror_farm::{
    ConfigInfo, HandleMsg, PoolItem,
    PoolsResponse, QueryMsg, StateInfo,
};
use std::fmt::Debug;
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, HandleMsg as TerraswapHandleMsg};

const SPEC_GOV: &str = "spec_gov";
const SPEC_TOKEN: &str = "spec_token";
const MIR_GOV: &str = "mir_gov";
const MIR_TOKEN: &str = "mir_token";
const MIR_STAKING: &str = "mir_staking";
const TERRA_SWAP: &str = "terra_swap";
const TEST_CREATOR: &str = "creator";
const TEST_CONTROLLER: &str = "controller";
const MIR_LP: &str = "mir_lp";
const SPY_TOKEN: &str = "spy_token";
const SPY_LP: &str = "spy_lp";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: String,
    pub bond_amount: Uint128,
    pub auto_bond_amount: Uint128,
    pub stake_bond_amount: Uint128,
    pub pending_farm_reward: Uint128,
    pub pending_spec_reward: Uint128,
    pub accum_spec_share: Uint128,
}

#[test]
fn test() {
    let mut deps = mock_dependencies(20, &[]);
    deps.querier.with_balance_percent(100);
    deps.querier.with_terraswap_pairs(&[(
        &"uusdmir_token".to_string(),
        &PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: MIR_TOKEN.to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: "pair0000".to_string(),
            liquidity_token: "liquidity0001".to_string(),
        },
    )]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128(1500000u128))],
    );

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_reinvest_unauthorized(&mut deps);
    test_reinvest_invalid_pool(&mut deps);
    test_reinvest_zero(&mut deps);
    test_reinvest_mir(&mut deps);

    deps.querier.with_terraswap_pairs(&[(
        &"uusdspy_token".to_string(),
        &PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: SPY_TOKEN.to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: "pair0001".to_string(),
            liquidity_token: "liquidity0002".to_string(),
        },
    )]);

    test_reinvest_spy(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        mirror_gov: MIR_GOV.to_string(),
        mirror_token: MIR_TOKEN.to_string(),
        mirror_staking: MIR_STAKING.to_string(),
        terraswap_factory: TERRA_SWAP.to_string(),
        platform: Option::None,
        controller: Some(TEST_CONTROLLER.to_string()),
        base_denom: "uusd".to_string(),
        community_fee: Decimal::zero(),
        platform_fee: Decimal::zero(),
        controller_fee: Decimal::zero(),
        deposit_fee: Decimal::zero(),
        lock_start: 0u64,
        lock_end: 0u64,
    };

    // success init
    let res = init(deps, env.clone(), config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res, config.clone());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            previous_spec_share: Uint128::zero(),
            total_farm_share: Uint128::zero(),
            total_weight: 0u32,
            spec_share_index: Decimal::zero(),
        }
    );

    // alter config, validate owner
    let env = mock_env(SPEC_GOV, &[]);
    let msg = HandleMsg::update_config {
        owner: Some(SPEC_GOV.to_string()),
        platform: None,
        controller: None,
        community_fee: None,
        platform_fee: None,
        controller_fee: None,
        deposit_fee: None,
        lock_start: None,
        lock_end: None,
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // success
    let env = mock_env(TEST_CREATOR, &[]);
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    config.owner = SPEC_GOV.to_string();
    assert_eq!(res, config.clone());

    config
}

fn test_register_asset(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // no permission
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::register_asset {
        asset_token: MIR_TOKEN.to_string(),
        staking_token: MIR_LP.to_string(),
        weight: 1u32,
        auto_compound: true,
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // success
    let env = mock_env(SPEC_GOV, &[]);
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // query pool info
    let msg = QueryMsg::pools {};
    let res: PoolsResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PoolsResponse {
            pools: vec![PoolItem {
                asset_token: MIR_TOKEN.to_string(),
                staking_token: MIR_LP.to_string(),
                weight: 1u32,
                auto_compound: true,
                farm_share: Uint128::zero(),
                state_spec_share_index: Decimal::zero(),
                stake_spec_share_index: Decimal::zero(),
                auto_spec_share_index: Decimal::zero(),
                farm_share_index: Decimal::zero(),
                total_stake_bond_amount: Uint128::zero(),
                total_stake_bond_share: Uint128::zero(),
                total_auto_bond_share: Uint128::zero(),
                reinvest_allowance: Uint128::zero(),
            }]
        }
    );

    // vault2
    let msg = HandleMsg::register_asset {
        asset_token: SPY_TOKEN.to_string(),
        staking_token: SPY_LP.to_string(),
        weight: 2u32,
        auto_compound: true,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 3u32);
}

fn test_reinvest_unauthorized(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest err
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::re_invest {
        asset_token: MIR_TOKEN.to_string(),
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());
}

fn test_reinvest_invalid_pool(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest err
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::re_invest {
        asset_token: "invalid".to_string(),
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());
}

fn test_reinvest_zero(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest zero
    let env = mock_env(TEST_CONTROLLER, &[]);
    let msg = HandleMsg::re_invest {
        asset_token: MIR_TOKEN.to_string(),
    };
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: "pair0000".to_string(),
                    amount: Uint128::zero(),
                    msg: Some(
                        to_binary(&TerraswapCw20HookMsg::Swap {
                            max_spread: None,
                            belief_price: None,
                            to: None,
                        })
                        .unwrap()
                    ),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: "pair0000".to_string(),
                    amount: Uint128::zero(),
                    expires: None,
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair0000".to_string(),
                msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: MIR_TOKEN.to_string(),
                            },
                            amount: Uint128::zero(),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::zero(),
                        },
                    ],
                    slippage_tolerance: None,
                })
                .unwrap(),
                send: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::zero(),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address,
                msg: to_binary(&HandleMsg::stake {
                    asset_token: MIR_TOKEN.to_string(),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}

fn test_reinvest_mir(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest mir
    let env = mock_env(TEST_CONTROLLER, &[]);

    let asset_token_raw = deps
        .api
        .canonical_address(&MIR_TOKEN.to_string())
        .unwrap();
    let mut pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();
    pool_info.reinvest_allowance = Uint128::from(100_000_000u128);
    pool_info_store(&mut deps.storage)
        .save(asset_token_raw.as_slice(), &pool_info)
        .unwrap();

    let msg = HandleMsg::re_invest {
        asset_token: MIR_TOKEN.to_string(),
    };
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    let pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();

    assert_eq!(Uint128::from(1_132_243u128), pool_info.reinvest_allowance);

    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: "pair0000".to_string(),
                    amount: Uint128::from(50_000_000u128),
                    msg: Some(
                        to_binary(&TerraswapCw20HookMsg::Swap {
                            max_spread: None,
                            belief_price: None,
                            to: None,
                        })
                        .unwrap()
                    ),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: "pair0000".to_string(),
                    amount: Uint128::from(48_867_757u128),
                    expires: None,
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair0000".to_string(),
                msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: MIR_TOKEN.to_string(),
                            },
                            amount: Uint128::from(48_867_757u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(48_867_757u128),
                        },
                    ],
                    slippage_tolerance: None,
                })
                .unwrap(),
                send: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(48_867_757u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address,
                msg: to_binary(&HandleMsg::stake {
                    asset_token: MIR_TOKEN.to_string(),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}

fn test_reinvest_spy(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest SPY
    let env = mock_env(TEST_CONTROLLER, &[]);

    let asset_token_raw = deps
        .api
        .canonical_address(&SPY_TOKEN.to_string())
        .unwrap();
    let mut pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();
    pool_info.reinvest_allowance = Uint128::from(100_000_000u128);
    pool_info_store(&mut deps.storage)
        .save(asset_token_raw.as_slice(), &pool_info)
        .unwrap();

    let msg = HandleMsg::re_invest {
        asset_token: SPY_TOKEN.to_string(),
    };
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    let pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();

    assert_eq!(Uint128::from(150_000u128), pool_info.reinvest_allowance);

    let net_swap_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
        amount: Uint128::from(49_504_950u128),
    };

    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair0001".to_string(),
                msg: to_binary(&TerraswapHandleMsg::Swap {
                    offer_asset: net_swap_asset,
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })
                .unwrap(),
                send: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(49_504_950u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPY_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: "pair0001".to_string(),
                    amount: Uint128::from(49_356_436u128),
                    expires: None,
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair0001".to_string(),
                msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: SPY_TOKEN.to_string(),
                            },
                            amount: Uint128::from(49_356_436u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(49_356_435u128),
                        },
                    ],
                    slippage_tolerance: None,
                })
                .unwrap(),
                send: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(49_356_435u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address,
                msg: to_binary(&HandleMsg::stake {
                    asset_token: SPY_TOKEN.to_string(),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}
