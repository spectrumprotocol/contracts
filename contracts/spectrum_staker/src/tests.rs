use crate::contract::{execute, instantiate};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage};
use cosmwasm_std::{to_binary, Coin, CosmosMsg, Decimal, OwnedDeps, StdError, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::staker::{ExecuteMsg, InstantiateMsg};
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::ExecuteMsg as TerraswapExecuteMsg;

const TOKEN: &str = "token";
const USER1: &str = "user1";
const TEST_CREATOR: &str = "creator";
const LP: &str = "lp_token";
const FARM: &str = "farm";
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
            contract_addr: "pair0001".to_string(),
            liquidity_token: "liquidity0001".to_string(),
        },
    )]);

    test_init(&mut deps);
    test_bond(&mut deps);
    test_zap(&mut deps);
}

fn test_init(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // test instantiate & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = InstantiateMsg {
        terraswap_factory: TERRA_SWAP.to_string(),
    };

    // success instantiate
    let res = instantiate(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
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
        contract: FARM.to_string(),
        asset_token: TOKEN.to_string(),
        staking_token: LP.to_string(),
        staker_addr: USER1.to_string(),
        prev_staking_token_amount: Uint128::zero(),
        compound_rate: Some(Decimal::percent(100u64)),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(res, Err(StdError::generic_err("unauthorized")));

    let msg = ExecuteMsg::bond {
        contract: FARM.to_string(),
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
        compound_rate: Some(Decimal::percent(100u64)),
        staker_addr: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
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
                    contract: FARM.to_string(),
                    asset_token: TOKEN.to_string(),
                    staking_token: "liquidity0001".to_string(),
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

fn test_zap(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
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
        contract: FARM.to_string(),
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
        slippage_tolerance: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_eq!(res, Err(StdError::generic_err("unauthorized")));

    let msg = ExecuteMsg::zap_to_bond {
        contract: FARM.to_string(),
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
        max_spread: Some(Decimal::percent(1u64)),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
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
                    contract: FARM.to_string(),
                    bond_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: Uint128::from(50_000_000u128),
                    },
                    asset_token: TOKEN.to_string(),
                    staker_addr: USER1.to_string(),
                    prev_asset_token_amount: Uint128::zero(),
                    slippage_tolerance: Some(Decimal::percent(1u64)),
                    compound_rate: Some(Decimal::percent(55u64)),
                })
                .unwrap(),
                funds: vec![],
            }),
        ]
    );
}
