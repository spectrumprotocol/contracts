use std::collections::HashSet;
use std::iter::FromIterator;

use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage};
use cosmwasm_std::{
    from_binary, to_binary, Coin, CosmosMsg, Decimal, OwnedDeps, StdError, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::staker::{ConfigInfo, Cw20HookMsg, ExecuteMsg, QueryMsg};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg};

const TOKEN: &str = "token";
const USER1: &str = "user1";
const TEST_CREATOR: &str = "creator";
const LP: &str = "lp_token";
const PAIR: &str = "pair0001";
const FARM1: &str = "farm1";
const FARM2: &str = "farm2";
const FARM3: &str = "farm3";
const FARM4: &str = "farm4";
const TERRA_SWAP: &str = "terra_swap";

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);
    deps.querier.with_terraswap_pairs(&[(
        &"uusdtoken".to_string(),
        &PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: TOKEN.to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            contract_addr: PAIR.to_string(),
            liquidity_token: LP.to_string(),
        },
    )]);

    test_config(&mut deps);
    test_bond(&mut deps);
    test_zap_bond(&mut deps);
    test_zap_unbond(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // test instantiate & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        terraswap_factory: TERRA_SWAP.to_string(),
        allowlist: vec![FARM3.to_string()],
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
        }
    );

    // alter config, allowlist
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::update_config {
        insert_allowlist: Some(vec![FARM1.to_string(), FARM2.to_string()]),
        remove_allowlist: Some(vec![FARM4.to_string()]),
    };

    // success
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        HashSet::<String>::from_iter(res.allowlist),
        HashSet::from_iter(vec![FARM1.to_string(), FARM2.to_string()])
    );
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(
        USER1,
        &vec![Coin {
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
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
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
                    spender: "pair0001".to_string(),
                    amount: Uint128::from(50_000_000u128),
                    expires: None,
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair0001".to_string(),
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
                    receiver: None
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
    };

    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_ok())
}

fn test_zap_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(
        USER1,
        &vec![Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(100_000_000u128),
        }],
    );

    // unauthorized
    let msg = ExecuteMsg::zap_to_bond_hook {
        contract: FARM1.to_string(),
        asset_token: TOKEN.to_string(),
        staker_addr: USER1.to_string(),
        compound_rate: Some(Decimal::percent(100u64)),
        bond_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(1_000_000u128),
        },
        prev_asset_token_amount: Uint128::zero(),
        slippage_tolerance: Decimal::percent(1u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(res, Err(StdError::generic_err("unauthorized")));

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
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(51u64),
    };
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
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("not support provide_asset as token"))
    );

    // pair_asset as native coin
    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM1.to_string(),
        compound_rate: Some(Decimal::percent(55u64)),
        provide_asset: Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(100_000_000u128),
        },
        pair_asset: AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("not support pair_asset as native coin"))
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
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
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
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair0001".to_string(),
                msg: to_binary(&TerraswapExecuteMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: Uint128::from(50_000_000u128),
                    },
                    max_spread: Some(Decimal::percent(1u64)),
                    belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                    to: None,
                })
                .unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(50_000_000u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_bond_hook {
                    contract: FARM1.to_string(),
                    bond_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: Uint128::from(50_000_000u128),
                    },
                    asset_token: TOKEN.to_string(),
                    staker_addr: USER1.to_string(),
                    prev_asset_token_amount: Uint128::zero(),
                    slippage_tolerance: Decimal::percent(1u64),
                    compound_rate: Some(Decimal::percent(55u64)),
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
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
    };

    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_ok())
}

fn test_zap_unbond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(USER1, &vec![]);

    // unauthorized
    let msg = ExecuteMsg::zap_to_unbond_hook {
        staker_addr: USER1.to_string(),
        prev_sell_asset: Asset {
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
        belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
        max_spread: Decimal::percent(1u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
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
        })
        .unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("invalid lp token"))
    );

    let info = mock_info(LP, &vec![]);

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
        })
        .unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("Slippage tolerance must be 0 to 0.5"))
    );

    // target_asset as token
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::zap_to_unbond {
            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            max_spread: Decimal::percent(1u64),
            sell_asset: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
            target_asset: AssetInfo::Token {
                contract_addr: TOKEN.to_string(),
            },
        })
        .unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("not support target_asset as token"))
    );

    // target_asset as token
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::zap_to_unbond {
            belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
            max_spread: Decimal::percent(1u64),
            sell_asset: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            target_asset: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        })
        .unwrap(),
    });

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res,
        Err(StdError::generic_err("not support sell_asset as native coin"))
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
                    msg: to_binary(&TerraswapCw20HookMsg::WithdrawLiquidity {}).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                    staker_addr: USER1.to_string(),
                    prev_sell_asset: Asset {
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
                    belief_price: Some(Decimal::from_ratio(1u128, 1u128)),
                    max_spread: Decimal::percent(1u64),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );
}
