use std::collections::HashSet;
use std::iter::FromIterator;
use std::str::FromStr;
use classic_bindings::TerraQuery;

use crate::contract::{execute, compute_swap_amount, instantiate, query, PairInfo, PairType};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{MOCK_CONTRACT_ADDR, mock_env, mock_info, MockApi, MockStorage};
use cosmwasm_std::{from_binary, to_binary, Coin, CosmosMsg, Decimal, OwnedDeps, StdError, Uint128, WasmMsg, Addr, BankMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::staker::{ConfigInfo, Cw20HookMsg, ExecuteMsg, QueryMsg};
use classic_terraswap::asset::{Asset, AssetInfo};
use classic_terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg, PoolResponse};
use spectrum_protocol::staker_single_asset::SwapOperation;

const TOKEN: &str = "token";
const USER1: &str = "user1";
const TEST_CREATOR: &str = "creator";
const LP: &str = "lp_token";
const PAIR: &str = "pair0001";
const TOKEN_B: &str = "token_b";
const LP_B: &str = "lp_token_b";
const PAIR_B: &str = "pair_b";
const LP_U: &str = "lp_token_u";
const PAIR_U: &str = "pair_u";
const FARM1: &str = "farm1";
const FARM2: &str = "farm2";
const FARM3: &str = "farm3";
const FARM4: &str = "farm4";
const TERRA_SWAP: &str = "terra_swap";

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);
    deps.querier.with_terraswap_factory(&[
        (&"uusdtoken".to_string(),
        &PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: Addr::unchecked(PAIR),
            liquidity_token: Addr::unchecked(LP),
            pair_type: None,
        }),
        (&"tokentoken_b".to_string(),
         &PairInfo {
             asset_infos: [
                 AssetInfo::Token {
                     contract_addr: TOKEN.to_string(),
                 },
                 AssetInfo::Token {
                     contract_addr: TOKEN_B.to_string(),
                 },
             ],
             contract_addr: Addr::unchecked(PAIR_B),
             liquidity_token: Addr::unchecked(LP_B),
             pair_type: None,
         }),
        (&"uusduluna".to_string(),
         &PairInfo {
             asset_infos: [
                 AssetInfo::NativeToken {
                     denom: "uusd".to_string(),
                 },
                 AssetInfo::NativeToken {
                     denom: "uluna".to_string(),
                 },
             ],
             contract_addr: Addr::unchecked(PAIR_U),
             liquidity_token: Addr::unchecked(LP_U),
             pair_type: Some(PairType::Xyk {})
         }),
        (&"ulunatoken".to_string(),
         &PairInfo {
             asset_infos: [
                 AssetInfo::Token {
                     contract_addr: TOKEN.to_string(),
                 },
                 AssetInfo::NativeToken {
                     denom: "uluna".to_string(),
                 },
             ],
             contract_addr: Addr::unchecked(PAIR),
             liquidity_token: Addr::unchecked(LP),
             pair_type: Some(PairType::Stable {})
         }),
    ]);
    deps.querier.with_terraswap_pairs(&[
        (&PAIR.to_string(),
        &PoolResponse {
            total_share: Uint128::from(500_000_000u128),
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: TOKEN.to_string(),
                    },
                    amount: Uint128::from(500_000_000u128),
                },
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    amount: Uint128::from(500_000_000u128),
                },
            ]
        }),
        (&PAIR_B.to_string(),
         &PoolResponse {
             total_share: Uint128::from(550_000_000u128),
             assets: [
                 Asset {
                     info: AssetInfo::Token {
                         contract_addr: TOKEN.to_string(),
                     },
                     amount: Uint128::from(500_000_000u128),
                 },
                 Asset {
                     info: AssetInfo::Token {
                         contract_addr: TOKEN_B.to_string(),
                     },
                     amount: Uint128::from(600_000_000u128),
                 },
             ]
         }),
    ]);

    test_config(&mut deps);
    test_bond(&mut deps);
    test_bond_2(&mut deps);
    test_zap_bond(&mut deps);
    test_zap_bond2(&mut deps);
    test_zap_unbond(&mut deps);
    test_zap_unbond2(&mut deps);
    test_zap_unbond3(&mut deps);
    test_native_assets(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    // test instantiate & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        terraswap_factory: TERRA_SWAP.to_string(),
        allowlist: vec![FARM3.to_string()],
        allow_all: false
    };

    // success instantiate
    let res = instantiate(deps.as_mut(), env.clone(), info, config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, config);

    // alter config, validate owner
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::update_config {
        insert_allowlist: Some(vec![FARM1.to_string()]),
        remove_allowlist: Some(vec![FARM3.to_string()]),
        allow_all: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_eq!(res, Err(StdError::generic_err("unauthorized")));

    // success
    let info = mock_info(TEST_CREATOR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    config.owner = TEST_CREATOR.to_string();
    assert_eq!(
        res,
        ConfigInfo {
            owner: TEST_CREATOR.to_string(),
            terraswap_factory: TERRA_SWAP.to_string(),
            allowlist: vec![FARM1.to_string()],
            allow_all: false
        }
    );

    // alter config, allowlist
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::update_config {
        insert_allowlist: Some(vec![FARM1.to_string(), FARM2.to_string()]),
        remove_allowlist: Some(vec![FARM4.to_string()]),
        allow_all: None,
    };

    // success
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        HashSet::<String>::from_iter(res.allowlist),
        HashSet::from_iter(vec![FARM1.to_string(), FARM2.to_string()])
    );
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(
        USER1,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(50_000_000u128),
        }],
    );

    // unauthorized
    let msg = ExecuteMsg::bond_hook {
        contract: FARM1.to_string(),
        asset_token: TOKEN.to_string(),
        staking_token: LP.to_string(),
        staker_addr: USER1.to_string(),
        prev_staking_token_amount: Uint128::zero(),
        compound_rate: Some(Decimal::percent(100u64)),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(res, Err(StdError::generic_err("unauthorized")));

    // slippage too high
    let msg = ExecuteMsg::bond {
        contract: FARM1.to_string(),
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
        ],
        slippage_tolerance: Decimal::percent(51u64),
        compound_rate: Some(Decimal::percent(100u64)),
        staker_addr: None,
        asset_token: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("Slippage tolerance must be 0 to 0.5"))
    );

    // contract not in allowlist
    let msg = ExecuteMsg::bond {
        contract: FARM3.to_string(),
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
        ],
        slippage_tolerance: Decimal::percent(1u64),
        compound_rate: Some(Decimal::percent(100u64)),
        staker_addr: None,
        asset_token: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(res, Err(StdError::generic_err("not allowed")));

    let msg = ExecuteMsg::bond {
        contract: FARM1.to_string(),
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
        ],
        slippage_tolerance: Decimal::percent(1u64),
        compound_rate: Some(Decimal::percent(100u64)),
        staker_addr: None,
        asset_token: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: USER1.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: Uint128::from(50_000_000u128),
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: PAIR.to_string(),
                    amount: Uint128::from(50_000_000u128),
                    expires: None,
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(50_000_000u128),
                        },
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN.to_string(),
                            },
                            amount: Uint128::from(50_000_000u128),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(1u64)),
                    receiver: None,
                    deadline: None,
                })
                .unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(50_000_000u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::bond_hook {
                    contract: FARM1.to_string(),
                    asset_token: TOKEN.to_string(),
                    staking_token: LP.to_string(),
                    staker_addr: USER1.to_string(),
                    prev_staking_token_amount: Uint128::zero(),
                    compound_rate: Some(Decimal::percent(100u64)),
                })
                .unwrap(),
                funds: vec![],
            }),
        ]
    );

    let msg = ExecuteMsg::bond {
        contract: FARM2.to_string(),
        assets: [
            Asset {
                info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
        ],
        slippage_tolerance: Decimal::percent(1u64),
        compound_rate: Some(Decimal::percent(100u64)),
        staker_addr: None,
        asset_token: None,
    };

    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_ok())
}

fn test_bond_2(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(
        USER1,
        &[],
    );

    let msg = ExecuteMsg::bond {
        contract: FARM1.to_string(),
        assets: [
            Asset {
                info: AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                amount: Uint128::from(50_000_000u128),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: TOKEN_B.to_string(),
                },
                amount: Uint128::from(60_000_000u128),
            },
        ],
        slippage_tolerance: Decimal::percent(1u64),
        compound_rate: Some(Decimal::percent(100u64)),
        staker_addr: None,
        asset_token: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: USER1.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: Uint128::from(50_000_000u128),
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: PAIR_B.to_string(),
                    amount: Uint128::from(50_000_000u128),
                    expires: None,
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN_B.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: USER1.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: Uint128::from(60_000_000u128),
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN_B.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: PAIR_B.to_string(),
                    amount: Uint128::from(60_000_000u128),
                    expires: None,
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR_B.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN.to_string(),
                            },
                            amount: Uint128::from(50_000_000u128),
                        },
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN_B.to_string(),
                            },
                            amount: Uint128::from(60_000_000u128),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(1u64)),
                    receiver: None,
                    deadline: None,
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::bond_hook {
                    contract: FARM1.to_string(),
                    asset_token: TOKEN_B.to_string(),
                    staking_token: LP_B.to_string(),
                    staker_addr: USER1.to_string(),
                    prev_staking_token_amount: Uint128::zero(),
                    compound_rate: Some(Decimal::percent(100u64)),
                })
                    .unwrap(),
                funds: vec![],
            }),
        ]
    );
}

fn test_zap_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(
        USER1,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100_000_000u128),
        }],
    );

    // slippage too high
    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        },
        pair_asset_b: None,
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: None,
        max_spread: Decimal::percent(51u64),
        asset_token: None,
        swap_hints: None,
    };
    // let bin = Binary::from_base64("eyJ6YXBfdG9fYm9uZCI6eyJjb250cmFjdCI6ImZhcm0xIiwicHJvdmlkZV9hc3NldCI6eyJpbmZvIjp7Im5hdGl2ZV90b2tlbiI6eyJkZW5vbSI6InV1c2QifX0sImFtb3VudCI6IjEwMDAwMDAwMCJ9LCJwYWlyX2Fzc2V0Ijp7InRva2VuIjp7ImNvbnRyYWN0X2FkZHIiOiJ0b2tlbiJ9fSwicGFpcl9hc3NldF9iIjpudWxsLCJiZWxpZWZfcHJpY2UiOiIxIiwiYmVsaWVmX3ByaWNlX2IiOm51bGwsIm1heF9zcHJlYWQiOiIwLjUxIiwiY29tcG91bmRfcmF0ZSI6IjAuNTUiLCJhc3NldF90b2tlbiI6bnVsbH19").unwrap();
    // let test: ExecuteMsg = from_binary(&bin).unwrap();
    // println!("{:?}", test);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("Slippage tolerance must be 0 to 0.5"))
    );

    // provide_asset as token
    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        },
        pair_asset_b: None,
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: None,
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("not support provide_asset as token"))
    );

    // contract not in allowlist
    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM3.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        },
        pair_asset_b: None,
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: None,
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(res, Err(StdError::generic_err("not allowed")));

    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        },
        pair_asset_b: None,
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: None,
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: Uint128::from(47801096u128),
                    },
                    max_spread: Some(Decimal::percent(1u64)),
                    belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                    to: None,
                    deadline: None,
                })
                .unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(47801096u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::bond {
                    contract: FARM1.to_string(),
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN.to_string(),
                            },
                            amount: Uint128::from(47657693u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(52198904u128),
                        },
                    ],
                    staker_addr: Some(USER1.to_string()),
                    slippage_tolerance: Decimal::percent(1u64),
                    compound_rate: Some(Decimal::percent(55u64)),
                    asset_token: None,
                })
                .unwrap(),
                funds: vec![],
            }),
        ]
    );

    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM2.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        },
        pair_asset_b: None,
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: None,
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: None,
    };

    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_ok())
}

fn test_zap_bond2(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(
        USER1,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100_000_000u128),
        }],
    );

    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        },
        pair_asset_b: Some(AssetInfo::Token {
            contract_addr: TOKEN_B.to_string(),
        }),
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: Uint128::from(100000000u128),
                    },
                    max_spread: Some(Decimal::percent(1u64)),
                    belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                    to: None,
                    deadline: None,
                }).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(100000000u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: PAIR_B.to_string(),
                    amount: Uint128::from(47663904u128),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        max_spread: Some(Decimal::percent(1u64)),
                        belief_price: Some(Decimal::from_ratio(6u128, 5u128)),
                        to: None,
                        deadline: None,
                    }).unwrap()
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::bond {
                    contract: FARM1.to_string(),
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN_B.to_string(),
                            },
                            amount: Uint128::from(47520913u128),
                        },
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN.to_string(),
                            },
                            amount: Uint128::from(52036096u128),
                        },
                    ],
                    staker_addr: Some(USER1.to_string()),
                    slippage_tolerance: Decimal::percent(1u64),
                    compound_rate: Some(Decimal::percent(55u64)),
                    asset_token: Some(TOKEN_B.to_string()),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

    // swap hints
    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        },
        pair_asset_b: Some(AssetInfo::Token {
            contract_addr: TOKEN_B.to_string(),
        }),
        belief_price: None,
        belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: Some(vec![
            SwapOperation {
                pair_contract: PAIR_U.to_string(),
                asset_info: AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            },
            SwapOperation {
                pair_contract: PAIR.to_string(),
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
            },
        ]),
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR_U.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: Uint128::from(100000000u128),
                    },
                    max_spread: Some(Decimal::percent(1u64)),
                    belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                    to: None,
                    deadline: None,
                }).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(100000000u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        },
                        amount: Uint128::from(99700000u128),
                    },
                    max_spread: Some(Decimal::percent(1u64)),
                    belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
                    to: None,
                    deadline: None,
                }).unwrap(),
                funds: vec![Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(99700000u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: PAIR_B.to_string(),
                    amount: Uint128::from(47527088u128),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        max_spread: Some(Decimal::percent(1u64)),
                        belief_price: Some(Decimal::from_ratio(6u128, 5u128)),
                        to: None,
                        deadline: None,
                    }).unwrap()
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::bond {
                    contract: FARM1.to_string(),
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN_B.to_string(),
                            },
                            amount: Uint128::from(47384507u128),
                        },
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: TOKEN.to_string(),
                            },
                            amount: Uint128::from(51873812u128),
                        },
                    ],
                    staker_addr: Some(USER1.to_string()),
                    slippage_tolerance: Decimal::percent(1u64),
                    compound_rate: Some(Decimal::percent(55u64)),
                    asset_token: Some(TOKEN_B.to_string()),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

}

fn test_zap_unbond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(USER1, &[]);

    // unauthorized
    let msg = ExecuteMsg::zap_to_unbond_hook {
        staker_addr: USER1.to_string(),
        prev_asset_a: Asset {
            info: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            amount: Uint128::from(1_000_000u128),
        },
        prev_target_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(1_000_000u128),
        },
        belief_price_a: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
        prev_asset_b: None,
        belief_price_b: None,
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(res, Err(StdError::generic_err("unauthorized")));

    // slippage too high
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::zap_to_unbond {
            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            max_spread: Decimal::percent(1u64),
            sell_asset: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            target_asset: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            sell_asset_b: None,
            belief_price_b: None,
            swap_hints: None,
        })
        .unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("invalid lp token"))
    );

    let info = mock_info(LP, &[]);

    // slippage too high
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::zap_to_unbond {
            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            max_spread: Decimal::percent(51u64),
            sell_asset: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            target_asset: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            sell_asset_b: None,
            belief_price_b: None,
            swap_hints: None,
        })
        .unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("Slippage tolerance must be 0 to 0.5"))
    );

    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(124u128),
        msg: to_binary(&Cw20HookMsg::zap_to_unbond {
            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            max_spread: Decimal::percent(1u64),
            sell_asset:  AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            target_asset: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            sell_asset_b: None,
            belief_price_b: None,
            swap_hints: None,
        })
        .unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LP.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(124u128),
                    contract: PAIR.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::WithdrawLiquidity { min_assets: None, deadline: None }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                    staker_addr: USER1.to_string(),
                    prev_asset_a: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::Token {
                            contract_addr: TOKEN.to_string(),
                        },
                    },
                    prev_target_asset: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                    },
                    belief_price_a: Some(Decimal::from_ratio(1u128, 1u128)),
                    max_spread: Decimal::percent(1u64),
                    prev_asset_b: None,
                    belief_price_b: None,
                    swap_hints: None,
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (&TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(123u128))]),
        (&"uusd".to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(456u128))]),
    ]);
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let msg = ExecuteMsg::zap_to_unbond_hook {
        staker_addr: USER1.to_string(),
        prev_asset_a: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
        },
        prev_target_asset: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
        belief_price_a: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
        prev_asset_b: None,
        belief_price_b: None,
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Bank(BankMsg::Send {
                to_address: USER1.to_string(),
                amount: vec![
                    Coin { denom: "uusd".to_string(), amount: Uint128::from(456u128) },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(123u128),
                    contract: PAIR.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        belief_price: Some(Decimal::one()),
                        max_spread: Some(Decimal::percent(1u64)),
                        to: Some(USER1.to_string()),
                        deadline: None,
                    }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );
}

fn test_zap_unbond2(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(LP_B, &[]);

    deps.querier.with_token_balances(&[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(124u128),
        msg: to_binary(&Cw20HookMsg::zap_to_unbond {
            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            max_spread: Decimal::percent(1u64),
            sell_asset: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            sell_asset_b: Some(AssetInfo::Token {
                contract_addr: TOKEN_B.to_string(),
            }),
            target_asset: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
            swap_hints: None,
        }).unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LP_B.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(124u128),
                    contract: PAIR_B.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::WithdrawLiquidity { min_assets: None, deadline: None }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                    staker_addr: USER1.to_string(),
                    prev_asset_a: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::Token {
                            contract_addr: TOKEN.to_string(),
                        },
                    },
                    prev_target_asset: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                    },
                    belief_price_a: Some(Decimal::from_ratio(1u128, 1u128)),
                    max_spread: Decimal::percent(1u64),
                    belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
                    prev_asset_b: Some(Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::Token {
                            contract_addr: TOKEN_B.to_string(),
                        },
                    }),
                    swap_hints: None,
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (&TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(123u128))]),
        (&"uusd".to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(456u128))]),
        (&TOKEN_B.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(789u128))]),
    ]);
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let msg = ExecuteMsg::zap_to_unbond_hook {
        staker_addr: USER1.to_string(),
        prev_asset_a: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
        },
        prev_target_asset: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
        belief_price_a: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
        belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
        prev_asset_b: Some(Asset {
            amount: Uint128::zero(),
            info: AssetInfo::Token {
                contract_addr: TOKEN_B.to_string(),
            },
        }),
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN_B.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(789u128),
                    contract: PAIR_B.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        belief_price: Some(Decimal::percent(120)),
                        max_spread: Some(Decimal::percent(1u64)),
                        to: None,
                        deadline: None,
                    }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                    staker_addr: USER1.to_string(),
                    prev_asset_a: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::Token {
                            contract_addr: TOKEN.to_string(),
                        },
                    },
                    prev_target_asset: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                    },
                    belief_price_a: Some(Decimal::from_ratio(1u128, 1u128)),
                    max_spread: Decimal::percent(1u64),
                    belief_price_b: None,
                    prev_asset_b: None,
                    swap_hints: None,
                }).unwrap(),
                funds: vec![],
            })
        ]
    );

    let msg = ExecuteMsg::zap_to_unbond_hook {
        staker_addr: USER1.to_string(),
        prev_asset_a: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
        },
        prev_target_asset: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
        belief_price_a: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
        belief_price_b: None,
        prev_asset_b: None,
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Bank(BankMsg::Send {
                to_address: USER1.to_string(),
                amount: vec![
                    Coin { denom: "uusd".to_string(), amount: Uint128::from(456u128) },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(123u128),
                    contract: PAIR.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        belief_price: Some(Decimal::one()),
                        max_spread: Some(Decimal::percent(1u64)),
                        to: Some(USER1.to_string()),
                        deadline: None,
                    }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );
}

fn test_zap_unbond3(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(LP_B, &[]);

    deps.querier.with_token_balances(&[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(124u128),
        msg: to_binary(&Cw20HookMsg::zap_to_unbond {
            belief_price: None,
            max_spread: Decimal::percent(1u64),
            sell_asset: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            sell_asset_b: Some(AssetInfo::Token {
                contract_addr: TOKEN_B.to_string(),
            }),
            target_asset: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
            swap_hints: Some(vec![
                SwapOperation {
                    pair_contract: PAIR.to_string(),
                    asset_info: AssetInfo::Token {
                        contract_addr: TOKEN.to_string(),
                    },
                    belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                },
                SwapOperation {
                    pair_contract: PAIR_U.to_string(),
                    asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
                },
            ]),
        }).unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LP_B.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(124u128),
                    contract: PAIR_B.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::WithdrawLiquidity { min_assets: None, deadline: None }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                    staker_addr: USER1.to_string(),
                    prev_asset_a: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::Token {
                            contract_addr: TOKEN.to_string(),
                        },
                    },
                    prev_target_asset: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                    },
                    belief_price_a: None,
                    max_spread: Decimal::percent(1u64),
                    belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
                    prev_asset_b: Some(Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::Token {
                            contract_addr: TOKEN_B.to_string(),
                        },
                    }),
                    swap_hints: Some(vec![
                        SwapOperation {
                            pair_contract: PAIR.to_string(),
                            asset_info: AssetInfo::Token {
                                contract_addr: TOKEN.to_string(),
                            },
                            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                        },
                        SwapOperation {
                            pair_contract: PAIR_U.to_string(),
                            asset_info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                            belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
                        },
                    ]),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (&TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(123u128))]),
        (&"uusd".to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(456u128))]),
        (&TOKEN_B.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(789u128))]),
    ]);
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let msg = ExecuteMsg::zap_to_unbond_hook {
        staker_addr: USER1.to_string(),
        prev_asset_a: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
        },
        prev_target_asset: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
        belief_price_a: None,
        max_spread: Decimal::percent(1u64),
        belief_price_b: Some(Decimal::from_ratio(6u128, 5u128)),
        prev_asset_b: Some(Asset {
            amount: Uint128::zero(),
            info: AssetInfo::Token {
                contract_addr: TOKEN_B.to_string(),
            },
        }),
        swap_hints: Some(vec![
            SwapOperation {
                pair_contract: PAIR.to_string(),
                asset_info: AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            },
            SwapOperation {
                pair_contract: PAIR_U.to_string(),
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
            },
        ]),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN_B.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(789u128),
                    contract: PAIR_B.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        belief_price: Some(Decimal::percent(120)),
                        max_spread: Some(Decimal::percent(1u64)),
                        to: None,
                        deadline: None,
                    }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                    staker_addr: USER1.to_string(),
                    prev_asset_a: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::Token {
                            contract_addr: TOKEN.to_string(),
                        },
                    },
                    prev_target_asset: Asset {
                        amount: Uint128::zero(),
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                    },
                    belief_price_a: None,
                    max_spread: Decimal::percent(1u64),
                    belief_price_b: None,
                    prev_asset_b: None,
                    swap_hints: Some(vec![
                        SwapOperation {
                            pair_contract: PAIR.to_string(),
                            asset_info: AssetInfo::Token {
                                contract_addr: TOKEN.to_string(),
                            },
                            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                        },
                        SwapOperation {
                            pair_contract: PAIR_U.to_string(),
                            asset_info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                            belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
                        },
                    ]),
                }).unwrap(),
                funds: vec![],
            })
        ]
    );

    let msg = ExecuteMsg::zap_to_unbond_hook {
        staker_addr: USER1.to_string(),
        prev_asset_a: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
        },
        prev_target_asset: Asset {
            amount: Uint128::zero(),
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        },
        belief_price_a: None,
        max_spread: Decimal::percent(1u64),
        belief_price_b: None,
        prev_asset_b: None,
        swap_hints: Some(vec![
            SwapOperation {
                pair_contract: PAIR.to_string(),
                asset_info: AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            },
            SwapOperation {
                pair_contract: PAIR_U.to_string(),
                asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
            },
        ]),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Bank(BankMsg::Send {
                to_address: USER1.to_string(),
                amount: vec![
                    Coin { denom: "uusd".to_string(), amount: Uint128::from(456u128) },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    amount: Uint128::from(123u128),
                    contract: PAIR.to_string(),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        belief_price: Some(Decimal::one()),
                        max_spread: Some(Decimal::percent(1u64)),
                        to: None,
                        deadline: None,
                    }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR_U.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken { denom: "uluna".to_string() },
                        amount: Uint128::from(123u128),
                    },
                    belief_price: Some(Decimal::from_ratio(1u128, 2u128)),
                    max_spread: Some(Decimal::percent(1u64)),
                    to: Some(USER1.to_string()),
                    deadline: None,
                }).unwrap(),
                funds: vec![
                    Coin { denom: "uluna".to_string(), amount: Uint128::from(123u128) }
                ],
            }),
        ]
    );
}

#[test]
fn test_get_swap_amount() {
    let pool_a = Uint128::from(12000_000000u128);
    let pool_b = Uint128::from(520_000000u128);
    let amount_a = Uint128::from(2500_000000u128);

    let swap_a = compute_swap_amount(amount_a, Uint128::zero(), pool_a, pool_b);
    assert_eq!(swap_a, Uint128::from(1192_872692u128));
}

fn test_native_assets(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[Coin {
        denom: "uusd".to_string(),
        amount: Uint128::from(100_000_000u128),
    }]);

    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(100u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::NativeToken {
            denom: "uluna".to_string(),
        },
        pair_asset_b: Some(AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        }),
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR_U.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken { denom: "uusd".to_string() },
                        amount: Uint128::from(100_000_000u128),
                    },
                    belief_price: Some(Decimal::from_str("1").unwrap()),
                    max_spread: Some(Decimal::percent(1)),
                    to: None,
                    deadline: None,
                }).unwrap(),
                funds: vec![
                    Coin { denom: "uusd".to_string(), amount: Uint128::from(100_000_000u128) }
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::bond {
                    contract: FARM1.to_string(),
                    assets: [
                        Asset {
                            info: AssetInfo::Token { contract_addr: TOKEN.to_string() },
                            amount: Uint128::zero(),
                        },
                        Asset {
                            info: AssetInfo::NativeToken { denom: "uluna".to_string() },
                            amount: Uint128::from(99700000u128),
                        },
                    ],
                    slippage_tolerance: Decimal::percent(1),
                    compound_rate: Some(Decimal::one()),
                    staker_addr: Some(TEST_CREATOR.to_string()),
                    asset_token: Some(TOKEN.to_string()),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

    // skip stable swap
    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(100u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::NativeToken {
            denom: "uluna".to_string(),
        },
        pair_asset_b: Some(AssetInfo::Token {
            contract_addr: TOKEN.to_string(),
        }),
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        belief_price_b: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
        asset_token: None,
        swap_hints: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR_U.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken { denom: "uusd".to_string() },
                        amount: Uint128::from(100_000_000u128),
                    },
                    belief_price: Some(Decimal::from_str("1").unwrap()),
                    max_spread: Some(Decimal::percent(1)),
                    to: None,
                    deadline: None,
                }).unwrap(),
                funds: vec![
                    Coin { denom: "uusd".to_string(), amount: Uint128::from(100_000_000u128) }
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::bond {
                    contract: FARM1.to_string(),
                    assets: [
                        Asset {
                            info: AssetInfo::Token { contract_addr: TOKEN.to_string() },
                            amount: Uint128::zero(),
                        },
                        Asset {
                            info: AssetInfo::NativeToken { denom: "uluna".to_string() },
                            amount: Uint128::from(99700000u128),
                        },
                    ],
                    slippage_tolerance: Decimal::percent(1),
                    compound_rate: Some(Decimal::one()),
                    staker_addr: Some(TEST_CREATOR.to_string()),
                    asset_token: Some(TOKEN.to_string()),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );
}
