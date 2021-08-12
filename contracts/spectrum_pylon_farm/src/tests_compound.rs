use crate::bond::deposit_farm_share;
use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{pool_info_read, pool_info_store, read_config};
use pylon_token::gov::Cw20HookMsg as PylonGovCw20HookMsg;
use pylon_token::gov::HandleMsg as PylonGovHandleMsg;
use pylon_token::staking::HandleMsg as PylonStakingHandleMsg;
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, Api, Coin, CosmosMsg, Decimal, Extern, String, Uint128, WasmMsg,
};
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::pylon_farm::{
    ConfigInfo, Cw20HookMsg, HandleMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, HandleMsg as GovHandleMsg};
use std::fmt::Debug;
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, HandleMsg as TerraswapHandleMsg};

const SPEC_GOV: &str = "spec_gov";
const SPEC_PLATFORM: &str = "spec_platform";
const SPEC_TOKEN: &str = "spec_token";
const SPEC_LP: &str = "spec_lp";
const SPEC_POOL: &str = "spec_pool";
const MINE_GOV: &str = "mine_gov";
const MINE_TOKEN: &str = "mine_token";
const MINE_STAKING: &str = "mine_staking";
const MINE_LP: &str = "mine_lp";
const MINE_POOL: &str = "mine_pool";
const TERRA_SWAP: &str = "terra_swap";
const TEST_CREATOR: &str = "creator";
const TEST_CONTROLLER: &str = "controller";
const FAIL_TOKEN: &str = "fail_token";
const FAIL_LP: &str = "fail_lp";
const USER1: &str = "user1";
const USER2: &str = "user2";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: String,
    pub farm_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub auto_bond_amount: Uint128,
    pub stake_bond_amount: Uint128,
    pub farm_share: Uint128,
    pub spec_share: Uint128,
    pub auto_bond_share: Uint128,
    pub stake_bond_share: Uint128,
    pub pending_farm_reward: Uint128,
    pub pending_spec_reward: Uint128,
    pub accum_spec_share: Uint128,
    pub locked_spec_share: Uint128,
    pub locked_spec_reward: Uint128,
}

#[test]
fn test() {
    let mut deps = mock_dependencies(20, &[]);
    deps.querier.with_balance_percent(100);
    deps.querier.with_terraswap_pairs(&[
        (
            &"uusdmine_token".to_string(),
            &PairInfo {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: MINE_TOKEN.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                contract_addr: MINE_POOL.to_string(),
                liquidity_token: MINE_LP.to_string(),
            },
        ),
        (
            &"uusdspec_token".to_string(),
            &PairInfo {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: SPEC_TOKEN.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                contract_addr: SPEC_POOL.to_string(),
                liquidity_token: SPEC_LP.to_string(),
            },
        ),
    ]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128(1500000u128))],
    );

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_compound_unauthorized(&mut deps);
    test_compound_zero(&mut deps);
    test_compound_mine_from_allowance(&mut deps);
    test_bond(&mut deps);
    test_compound_mine(&mut deps);
    test_compound_mine_with_fees(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        pylon_gov: MINE_GOV.to_string(),
        pylon_token: MINE_TOKEN.to_string(),
        pylon_staking: MINE_STAKING.to_string(),
        terraswap_factory: TERRA_SWAP.to_string(),
        platform: Some(SPEC_PLATFORM.to_string()),
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
        asset_token: MINE_TOKEN.to_string(),
        staking_token: MINE_LP.to_string(),
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
                asset_token: MINE_TOKEN.to_string(),
                staking_token: MINE_LP.to_string(),
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

    // test register fail
    let msg = HandleMsg::register_asset {
        asset_token: FAIL_TOKEN.to_string(),
        staking_token: FAIL_LP.to_string(),
        weight: 2u32,
        auto_compound: true,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 1u32);
}

fn test_compound_unauthorized(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest err
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::compound {};
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());
}

fn test_compound_zero(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest zero
    let env = mock_env(TEST_CONTROLLER, &[]);
    let msg = HandleMsg::compound {};
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_STAKING.to_string(),
                send: vec![],
                msg: to_binary(&PylonStakingHandleMsg::Withdraw {}).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: MINE_POOL.to_string(),
                    amount: Uint128::zero(),
                    expires: None,
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_POOL.to_string(),
                msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: MINE_TOKEN.to_string(),
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
                    asset_token: MINE_TOKEN.to_string(),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}

fn test_compound_mine_from_allowance(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env(TEST_CONTROLLER, &[]);

    let asset_token_raw = deps
        .api
        .canonical_address(&MINE_TOKEN.to_string())
        .unwrap();
    let mut pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();
    pool_info.reinvest_allowance = Uint128::from(100_000_000u128);
    pool_info_store(&mut deps.storage)
        .save(asset_token_raw.as_slice(), &pool_info)
        .unwrap();

    let msg = HandleMsg::compound {};
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    let pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();

    assert_eq!(Uint128::from(1_132_243u128), pool_info.reinvest_allowance);

    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_STAKING.to_string(),
                send: vec![],
                msg: to_binary(&PylonStakingHandleMsg::Withdraw {}).unwrap(),
            }), //ok
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: MINE_POOL.to_string(),
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
                contract_addr: MINE_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: MINE_POOL.to_string(),
                    amount: Uint128::from(48_867_757u128),
                    expires: None
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_POOL.to_string(),
                msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: MINE_TOKEN.to_string(),
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
                    asset_token: MINE_TOKEN.to_string(),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // bond err
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(10000u128),
        msg: Some(
            to_binary(&Cw20HookMsg::bond {
                staker_addr: None,
                asset_token: MINE_TOKEN.to_string(),
                compound_rate: Some(Decimal::percent(60)),
            })
            .unwrap(),
        ),
    });
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // bond success user1 1000 MINE-LP
    let env = mock_env(MINE_LP, &[]);
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    let mut pool_info = pool_info_read(&deps.storage)
        .load(config.pylon_token.as_slice())
        .unwrap();
    deposit_farm_share(deps, &mut pool_info, &config, Uint128::from(500u128)).unwrap();
    pool_info_store(&mut deps.storage)
        .save(config.pylon_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &MINE_STAKING.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(10000u128),
            )],
        ),
        (
            &MINE_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(1000u128),
            )],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(2700u128),
            )],
        ),
    ]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(1000u128),
            pending_spec_reward: Uint128::from(2700u128),
            bond_amount: Uint128::from(10000u128),
            auto_bond_amount: Uint128::from(6000u128),
            stake_bond_amount: Uint128::from(4000u128),
            accum_spec_share: Uint128::from(2700u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::zero(),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(500u128),
            spec_share: Uint128::from(2700u128),
            auto_bond_share: Uint128::from(6000u128),
            stake_bond_share: Uint128::from(4000u128),
            locked_spec_share: Uint128::zero(),
            locked_spec_reward: Uint128::zero(),
        },]
    );

    // unbond 3000 MINE-LP
    let env = mock_env(USER1, &[]);
    let msg = HandleMsg::unbond {
        asset_token: MINE_TOKEN.to_string(),
        amount: Uint128::from(3000u128),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages,
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_STAKING.to_string(),
                send: vec![],
                msg: to_binary(&PylonStakingHandleMsg::Unbond {
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_LP.to_string(),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
        ]
    );

    // withdraw rewards
    let msg = HandleMsg::withdraw { asset_token: None };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                send: vec![],
                msg: to_binary(&GovHandleMsg::withdraw {
                    amount: Some(Uint128::from(2700u128)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(2700u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_GOV.to_string(),
                send: vec![],
                msg: to_binary(&PylonGovHandleMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(1000u128)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(1000u128),
                })
                .unwrap(),
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &MINE_STAKING.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(7000u128),
            )],
        ),
        (
            &MINE_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(0u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(0u128))],
        ),
    ]);

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.reward_infos, vec![]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(0u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(4200u128),
            stake_bond_amount: Uint128::from(2800u128),
            accum_spec_share: Uint128::from(2700u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(0u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
            locked_spec_share: Uint128::zero(),
            locked_spec_reward: Uint128::zero(),
        },]
    );

    // bond user2 5000 MINE-LP auto-stake
    let env = mock_env(MINE_LP, &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(5000u128),
        msg: Some(
            to_binary(&Cw20HookMsg::bond {
                staker_addr: None,
                asset_token: MINE_TOKEN.to_string(),
                compound_rate: None,
            })
            .unwrap(),
        ),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let mut pool_info = pool_info_read(&deps.storage)
        .load(config.pylon_token.as_slice())
        .unwrap();
    deposit_farm_share(deps, &mut pool_info, &config, Uint128::from(10000u128)).unwrap();
    pool_info_store(&mut deps.storage)
        .save(config.pylon_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &MINE_STAKING.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(12000u128),
            )],
        ),
        (
            &MINE_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(5000u128),
            )],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(1000u128),
            )],
        ),
    ]);

    /*
        USER1 7000 (auto 4200, stake 2800)
        USER2 5000 (auto 0, stake 5000)
        Total lp 12000
        Total farm share 7800
        Farm share +10000
        USER1 Farm share = 28/78 * 10000 = 3589
        USER2 Farm share = 50/78 * 10000 = 6410
        Farm reward 5000
        USER1 Farm reward = 28/78 * 5000 = 1794
        USER2 Farm reward = 50/78 * 5000 = 3205
        SPEC reward +1000
        USER1 SPEC reward ~ 582
        USER2 SPEC reward ~ 416
    */

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(1794u128),
            pending_spec_reward: Uint128::from(582u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(4200u128),
            stake_bond_amount: Uint128::from(2800u128),
            accum_spec_share: Uint128::from(3282u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(3589u128),
            spec_share: Uint128::from(582u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
            locked_spec_share: Uint128::zero(),
            locked_spec_reward: Uint128::zero(),
        },]
    );

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(3205u128),
            pending_spec_reward: Uint128::from(416u128),
            bond_amount: Uint128::from(5000u128),
            auto_bond_amount: Uint128::from(0u128),
            stake_bond_amount: Uint128::from(5000u128),
            accum_spec_share: Uint128::from(416u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(6410u128),
            spec_share: Uint128::from(416u128),
            auto_bond_share: Uint128::from(0u128),
            stake_bond_share: Uint128::from(5000u128),
            locked_spec_share: Uint128::zero(),
            locked_spec_reward: Uint128::zero(),
        },]
    );
}

fn test_compound_mine(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env(TEST_CONTROLLER, &[]);

    let asset_token_raw = deps
        .api
        .canonical_address(&MINE_TOKEN.to_string())
        .unwrap();
    let mut pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();
    pool_info.reinvest_allowance = Uint128::from(0u128);
    pool_info_store(&mut deps.storage)
        .save(asset_token_raw.as_slice(), &pool_info)
        .unwrap();

    /*
    pending rewards 12000 MINE
    USER1 7000 (auto 4200, stake 2800)
    USER2 5000 (auto 0, stake 5000)
    total 12000
    auto 4200 / 12000 * 12000 = 4200
    stake 7800 / 12000 * 12000 = 7800
    swap amount 2100 MINE -> 2073 UST
    provide UST = 2052
    provide MINE = 2052
    remaining = 48
    */
    let msg = HandleMsg::compound {};
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    let pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();

    assert_eq!(Uint128::from(48u128), pool_info.reinvest_allowance);

    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_STAKING.to_string(),
                send: vec![],
                msg: to_binary(&PylonStakingHandleMsg::Withdraw {}).unwrap(),
            }), //ok
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: MINE_POOL.to_string(),
                    amount: Uint128::from(2100u128),
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
                contract_addr: MINE_TOKEN.to_string(),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: MINE_GOV.to_string(),
                    amount: Uint128::from(7800u128),
                    msg: Some(to_binary(&PylonGovCw20HookMsg::StakeVotingTokens {}).unwrap()),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: MINE_POOL.to_string(),
                    amount: Uint128::from(2052u128),
                    expires: None
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_POOL.to_string(),
                msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: MINE_TOKEN.to_string(),
                            },
                            amount: Uint128::from(2052u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(2052u128),
                        },
                    ],
                    slippage_tolerance: None,
                })
                .unwrap(),
                send: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(2052u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address,
                msg: to_binary(&HandleMsg::stake {
                    asset_token: MINE_TOKEN.to_string(),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &MINE_STAKING.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(12100u128),
            )],
        ),
        (
            &MINE_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(12800u128),
            )],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(1000u128),
            )],
        ),
    ]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(4594u128),
            pending_spec_reward: Uint128::from(586u128),
            bond_amount: Uint128::from(7100u128),
            auto_bond_amount: Uint128::from(4300u128),
            stake_bond_amount: Uint128::from(2800u128),
            accum_spec_share: Uint128::from(3286u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(9189u128),
            spec_share: Uint128::from(586u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
            locked_spec_share: Uint128::zero(),
            locked_spec_reward: Uint128::zero(),
        },]
    );

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MINE_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(8205u128),
            pending_spec_reward: Uint128::from(413u128),
            bond_amount: Uint128::from(5000u128),
            auto_bond_amount: Uint128::from(0u128),
            stake_bond_amount: Uint128::from(5000u128),
            accum_spec_share: Uint128::from(413u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(16410u128),
            spec_share: Uint128::from(413u128),
            auto_bond_share: Uint128::from(0u128),
            stake_bond_share: Uint128::from(5000u128),
            locked_spec_share: Uint128::zero(),
            locked_spec_reward: Uint128::zero(),
        },]
    );
}

fn test_compound_mine_with_fees(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // update fees
    let env = mock_env(SPEC_GOV, &[]);
    let msg = HandleMsg::update_config {
        owner: Some(SPEC_GOV.to_string()),
        platform: None,
        controller: None,
        community_fee: Some(Decimal::percent(3u64)),
        platform_fee: Some(Decimal::percent(1u64)),
        controller_fee: Some(Decimal::percent(1u64)),
        deposit_fee: None,
        lock_start: None,
        lock_end: None,
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_ok());

    let env = mock_env(TEST_CONTROLLER, &[]);
    let asset_token_raw = deps
        .api
        .canonical_address(&MINE_TOKEN.to_string())
        .unwrap();
    let mut pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();
    pool_info.reinvest_allowance = Uint128::from(0u128);
    pool_info_store(&mut deps.storage)
        .save(asset_token_raw.as_slice(), &pool_info)
        .unwrap();

    /*
    pending rewards 12100 MINE
    USER1 7100 (auto 4300, stake 2800)
    USER2 5000 (auto 0, stake 5000)
    total 12100
    total fee = 605
    remaining = 11495
    auto 4300 / 12100 * 11495 = 4085
    stake 7800 / 12100 * 11495 = 7410
    swap amount 2042 MINE -> 2016 UST
    provide UST = 1996
    provide MINE = 1996
    remaining = 46
    fee swap amount 605 MINE -> 591 UST -> 590 SPEC
    community fee = 363 / 605 * 590 = 354
    platform fee = 121 / 605 * 590 = 118
    controller fee = 121 / 605 * 590 = 118
    total swap amount 2647 MINE
    */
    let msg = HandleMsg::compound {};
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    let pool_info = pool_info_read(&deps.storage)
        .load(asset_token_raw.as_slice())
        .unwrap();

    assert_eq!(Uint128::from(46u128), pool_info.reinvest_allowance);

    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_STAKING.to_string(),
                send: vec![],
                msg: to_binary(&PylonStakingHandleMsg::Withdraw {}).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: MINE_POOL.to_string(),
                    amount: Uint128::from(2647u128),
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
                contract_addr: SPEC_POOL.to_string(),
                msg: to_binary(&TerraswapHandleMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string(),
                        },
                        amount: Uint128::from(591u128),
                    },
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })
                .unwrap(),
                send: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(591u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                msg: to_binary(&GovHandleMsg::mint {}).unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: SPEC_GOV.to_string(),
                    amount: Uint128::from(354u128),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: SPEC_GOV.to_string(),
                    amount: Uint128::from(118u128),
                    msg: Some(
                        to_binary(&GovCw20HookMsg::stake_tokens {
                            staker_addr: Some(SPEC_PLATFORM.to_string()),
                        })
                        .unwrap()
                    ),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: SPEC_GOV.to_string(),
                    amount: Uint128::from(118u128),
                    msg: Some(
                        to_binary(&GovCw20HookMsg::stake_tokens {
                            staker_addr: Some(TEST_CONTROLLER.to_string()),
                        })
                        .unwrap()
                    ),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: MINE_GOV.to_string(),
                    amount: Uint128::from(7410u128),
                    msg: Some(to_binary(&PylonGovCw20HookMsg::StakeVotingTokens {}).unwrap()),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_TOKEN.to_string(),
                msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
                    spender: MINE_POOL.to_string(),
                    amount: Uint128::from(1996u128),
                    expires: None
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MINE_POOL.to_string(),
                msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: MINE_TOKEN.to_string(),
                            },
                            amount: Uint128::from(1996u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(1996u128),
                        },
                    ],
                    slippage_tolerance: None,
                })
                .unwrap(),
                send: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(1996u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address,
                msg: to_binary(&HandleMsg::stake {
                    asset_token: MINE_TOKEN.to_string(),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}
