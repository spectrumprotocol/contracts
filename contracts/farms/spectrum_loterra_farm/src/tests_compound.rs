use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{pool_info_read, pool_info_store, read_config, read_state, state_store};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, Coin, CosmosMsg, Decimal, OwnedDeps, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use loterra::staking::ExecuteMsg as LoterraStakingExecuteMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use spectrum_protocol::loterra_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use std::fmt::Debug;
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg};
use crate::bond::deposit_farm_share;

const SPEC_GOV: &str = "SPEC_GOV";
const SPEC_PLATFORM: &str = "spec_platform";
const SPEC_TOKEN: &str = "spec_token";
const SPEC_LP: &str = "spec_lp";
const SPEC_POOL: &str = "spec_pool";
const LOTA_TOKEN: &str = "lota_token";
const LOTA_STAKING: &str = "lota_staking";
const LOTA_LP: &str = "lota_lp";
const LOTA_POOL: &str = "lota_pool";
const LOTA_GOV: &str = "lota_gov";
const TEST_CREATOR: &str = "creator";
const TEST_CONTROLLER: &str = "controller";
const FAIL_TOKEN: &str = "fail_token";
const FAIL_LP: &str = "fail_lp";
const USER1: &str = "user1";
const USER2: &str = "user2";
const ANC_MARKET: &str = "anc_market";
const AUST_TOKEN: &str = "aust_token";

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
}

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);
    deps.querier.with_terraswap_pairs(&[
        (
            &"uusdlota_token".to_string(),
            &PairInfo {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: LOTA_TOKEN.to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                contract_addr: LOTA_POOL.to_string(),
                liquidity_token: LOTA_LP.to_string(),
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
        &[(&"uusd".to_string(), &Uint128::from(1500000u128))],
    );

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_compound_unauthorized(&mut deps);
    test_compound_zero(&mut deps);
    test_compound_lota_from_allowance(&mut deps);
    test_bond(&mut deps);
    test_compound_lota(&mut deps);
    test_compound_lota_with_fees(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        loterra_token: LOTA_TOKEN.to_string(),
        loterra_staking: LOTA_STAKING.to_string(),
        loterra_gov: LOTA_GOV.to_string(),
        platform: SPEC_PLATFORM.to_string(),
        controller: TEST_CONTROLLER.to_string(),
        base_denom: "uusd".to_string(),
        community_fee: Decimal::zero(),
        platform_fee: Decimal::zero(),
        controller_fee: Decimal::zero(),
        deposit_fee: Decimal::zero(),
        anchor_market: ANC_MARKET.to_string(),
        aust_token: AUST_TOKEN.to_string(),
        pair_contract: LOTA_POOL.to_string(),
    };

    // success init
    let res = instantiate(deps.as_mut(), env.clone(), info, config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, config);

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            previous_spec_share: Uint128::zero(),
            total_farm_share: Uint128::zero(),
            total_weight: 0u32,
            spec_share_index: Decimal::zero(),
            earning: Uint128::zero(),
        }
    );

    // alter config, validate owner
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::update_config {
        owner: Some(SPEC_GOV.to_string()),
        controller: None,
        community_fee: None,
        platform_fee: None,
        controller_fee: None,
        deposit_fee: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(TEST_CREATOR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    config.owner = SPEC_GOV.to_string();
    assert_eq!(res, config);

    config
}

fn test_register_asset(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // no permission
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::register_asset {
        asset_token: LOTA_TOKEN.to_string(),
        staking_token: LOTA_LP.to_string(),
        weight: 1u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(SPEC_GOV, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query pool info
    let msg = QueryMsg::pools {};
    let res: PoolsResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PoolsResponse {
            pools: vec![PoolItem {
                asset_token: LOTA_TOKEN.to_string(),
                staking_token: LOTA_LP.to_string(),
                weight: 1u32,
                farm_share: Uint128::zero(),
                state_spec_share_index: Decimal::zero(),
                stake_spec_share_index: Decimal::zero(),
                auto_spec_share_index: Decimal::zero(),
                farm_share_index: Decimal::zero(),
                total_stake_bond_amount: Uint128::zero(),
                total_stake_bond_share: Uint128::zero(),
                total_auto_bond_share: Uint128::zero(),
            }]
        }
    );

    // test register fail
    let msg = ExecuteMsg::register_asset {
        asset_token: FAIL_TOKEN.to_string(),
        staking_token: FAIL_LP.to_string(),
        weight: 2u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 1u32);
}

fn test_compound_unauthorized(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_err());
}

fn test_compound_zero(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest zero
    let env = mock_env();
    let info = mock_info(TEST_CONTROLLER, &[]);
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&LoterraStakingExecuteMsg::ClaimRewards { recipient: None }).unwrap(),
            }),
        ]
    );
}

fn test_compound_lota_from_allowance(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(TEST_CONTROLLER, &[]);

    deps.querier.with_token_balances(&[
        (&LOTA_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100_000_000u128))])
    ]);

    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&LoterraStakingExecuteMsg::ClaimRewards { recipient: None }).unwrap(),
            }), //ok
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: LOTA_POOL.to_string(),
                    amount: Uint128::from(50_000_000u128),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        max_spread: Some(Decimal::percent(50)),
                        belief_price: None,
                        to: None,
                    })
                    .unwrap()
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: LOTA_POOL.to_string(),
                    amount: Uint128::from(48_872_636u128),
                    expires: None
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_POOL.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: LOTA_TOKEN.to_string(),
                            },
                            amount: Uint128::from(48_872_636u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(48_867_757u128),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(100)),
                    receiver: None
                })
                .unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(48_867_757u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::stake {
                    asset_token: LOTA_TOKEN.to_string(),
                })
                .unwrap(),
                funds: vec![],
            }),
        ]
    );
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // bond err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(10000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: LOTA_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(100)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // bond success user1 1000 LOTA-LP
    let info = mock_info(LOTA_LP, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let config = read_config(deps_ref.storage).unwrap();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.loterra_token.as_slice())
        .unwrap();
        deposit_farm_share(
            deps_ref,
            &env,
            &mut state,
            &mut pool_info,
            &config,
            Uint128::from(500u128),
            Some(env.block.time.seconds())
        )
        .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.loterra_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &LOTA_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(2700u128))],
        ),
    ]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: LOTA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(2700u128),
            bond_amount: Uint128::from(10000u128),
            auto_bond_amount: Uint128::from(10000u128),
            stake_bond_amount: Uint128::from(0u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::zero(),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(2700u128),
            auto_bond_share: Uint128::from(10000u128),
            stake_bond_share: Uint128::from(0u128),
        },]
    );

    // unbond 3000 LOTA-LP
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unbond {
        asset_token: LOTA_TOKEN.to_string(),
        amount: Uint128::from(3000u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap()
            .messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&LoterraStakingExecuteMsg::UnbondStake {
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&LoterraStakingExecuteMsg::WithdrawStake {})
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_LP.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
        ]
    );

    // withdraw rewards
    let msg = ExecuteMsg::withdraw { asset_token: None, spec_amount: None, farm_amount: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap()
            .messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                funds: vec![],
                msg: to_binary(&GovExecuteMsg::withdraw {
                    amount: Some(Uint128::from(2700u128)),
                    days: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(2700u128),
                })
                .unwrap(),
            })
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &LOTA_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(7000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(0u128))],
        ),
    ]);

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.reward_infos, vec![]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: LOTA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(0u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(7000u128),
            stake_bond_amount: Uint128::from(0u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(0u128),
            auto_bond_share: Uint128::from(7000u128),
            stake_bond_share: Uint128::from(0u128),
        },]
    );

    // bond user2 5000 LOTA-LP auto-compound
    let info = mock_info(LOTA_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(5000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: LOTA_TOKEN.to_string(),
            compound_rate: Some(Decimal::one()),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.loterra_token.as_slice())
        .unwrap();
        deposit_farm_share(
            deps_ref,
            &env,
            &mut state,
            &mut pool_info,
            &config,
            Uint128::from(500u128),
            Some(env.block.time.seconds())
        )
        .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.loterra_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &LOTA_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    /*
        USER1 7000 (auto 7000, stake 0)
        USER2 5000 (auto 5000, stake 0)
        Total lp 12000
        Total farm share 0
        SPEC reward +1000
        USER1 SPEC reward ~ 583
        USER2 SPEC reward ~ 416
    */

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: LOTA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(583u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(7000u128),
            stake_bond_amount: Uint128::from(0u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(583u128),
            auto_bond_share: Uint128::from(7000u128),
            stake_bond_share: Uint128::from(0u128),
        },]
    );

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: LOTA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(416u128),
            bond_amount: Uint128::from(5000u128),
            auto_bond_amount: Uint128::from(5000u128),
            stake_bond_amount: Uint128::from(0u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(416u128),
            auto_bond_share: Uint128::from(5000u128),
            stake_bond_share: Uint128::from(0u128),
        },]
    );
}

fn test_compound_lota(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(TEST_CONTROLLER, &[]);

    deps.querier.with_token_balances(&[
        (
            &LOTA_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::zero())]
        ),
        (
            &LOTA_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12000u128))],
        ),
        (
            &LOTA_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(5000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    /*
    pending rewards 12000 LOTA
    USER1 7000 (auto 7000, stake 0)
    USER2 5000 (auto 5000, stake 0)
    total 12000
    auto 12000 / 12000 * 12000 = 12000
    stake 0 / 12000 * 12000 = 0
    swap amount 6000 LOTA -> 5982 UST
    provide UST = 5863
    provide LOTA = 5863
    remaining = 137
    */
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&LoterraStakingExecuteMsg::ClaimRewards { recipient: None }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: LOTA_POOL.to_string(),
                    amount: Uint128::from(6000u128),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        max_spread: Some(Decimal::percent(50)),
                        belief_price: None,
                        to: None,
                    })
                    .unwrap()
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: LOTA_POOL.to_string(),
                    amount: Uint128::from(5863u128),
                    expires: None,
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_POOL.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: LOTA_TOKEN.to_string(),
                            },
                            amount: Uint128::from(5863u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(5863u128),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(100)),
                    receiver: None
                })
                .unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(5863u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::stake {
                    asset_token: LOTA_TOKEN.to_string(),
                })
                .unwrap(),
                funds: vec![],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &LOTA_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12100u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: LOTA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(583u128),
            bond_amount: Uint128::from(7058u128),
            auto_bond_amount: Uint128::from(7058u128),
            stake_bond_amount: Uint128::from(0u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(583u128),
            auto_bond_share: Uint128::from(7000u128),
            stake_bond_share: Uint128::from(0u128),
        },]
    );

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: LOTA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(416u128),
            bond_amount: Uint128::from(5041u128),
            auto_bond_amount: Uint128::from(5041u128),
            stake_bond_amount: Uint128::from(0u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(416u128),
            auto_bond_share: Uint128::from(5000u128),
            stake_bond_share: Uint128::from(0u128),
        },]
    );
}

fn test_compound_lota_with_fees(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // update fees
    let env = mock_env();
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::update_config {
        owner: None,
        controller: None,
        community_fee: Some(Decimal::percent(3u64)),
        platform_fee: Some(Decimal::percent(1u64)),
        controller_fee: Some(Decimal::percent(1u64)),
        deposit_fee: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let info = mock_info(TEST_CONTROLLER, &[]);

    deps.querier.with_token_balances(&[
        (
            &LOTA_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::zero())]
        ),
        (
            &LOTA_STAKING.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12100u128))],
        ),
        (
            &LOTA_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12800u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    /*
    pending rewards 12100 LOTA
    USER1 7058 (auto 7058, stake 0)
    USER2 5041 (auto 5041, stake 0)
    total 12099
    total fee = 605
    remaining = 11494
    auto 12099 / 12100 * 11494 = 11493
    stake 0 / 12100 * 11495 = 0
    swap amount 6532 LOTA (12100 / 2 + 605) -> 6512 UST
    provide UST = 5616
    provide LOTA = 5616
    remaining = 311
    fee swap amount 605 LOTA -> 591 UST -> 590 SPEC
    community fee = 363 / 605 * 590 = 354
    platform fee = 121 / 605 * 590 = 118
    controller fee = 121 / 605 * 590 = 118
    total swap amount 6532 LOTA
    */
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&LoterraStakingExecuteMsg::ClaimRewards { recipient: None }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: LOTA_POOL.to_string(),
                    amount: Uint128::from(6352u128),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        max_spread: Some(Decimal::percent(50)),
                        belief_price: None,
                        to: None,
                    }).unwrap()
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ANC_MARKET.to_string(),
                msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {}).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(591u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                msg: to_binary(&spectrum_protocol::gov::ExecuteMsg::mint {}).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::send_fee {}).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: LOTA_POOL.to_string(),
                    amount: Uint128::from(5616u128),
                    expires: None
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: LOTA_POOL.to_string(),
                msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: LOTA_TOKEN.to_string(),
                            },
                            amount: Uint128::from(5616u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(5616u128),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(100)),
                    receiver: None
                }).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(5616u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::stake {
                    asset_token: LOTA_TOKEN.to_string(),
                })
                    .unwrap(),
                funds: vec![],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &AUST_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(590u128))]
        ),
    ]);

    // cannot call send fee from others
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::send_fee {};
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: AUST_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: SPEC_GOV.to_string(),
                    amount: Uint128::from(354u128),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: AUST_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: SPEC_PLATFORM.to_string(),
                    amount: Uint128::from(118u128),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: AUST_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: TEST_CONTROLLER.to_string(),
                    amount: Uint128::from(118u128),
                }).unwrap(),
                funds: vec![],
            }),
        ]);
}
