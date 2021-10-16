use crate::bond::deposit_farm_share;
use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::read_config;
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, CosmosMsg, Decimal, OwnedDeps, StdError, Uint128, WasmMsg, Storage};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use mirror_protocol::gov::ExecuteMsg as MirrorGovExecuteMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::ExecuteMsg as GovExecuteMsg;
use spectrum_protocol::mirror_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, ExecuteMsg as MirrorStakingExecuteMsg, PoolItem,
    PoolsResponse, QueryMsg, StateInfo,
};
use std::fmt::Debug;

const SPEC_GOV: &str = "spec_gov";
const SPEC_TOKEN: &str = "spec_token";
const MIR_GOV: &str = "mir_gov";
const MIR_TOKEN: &str = "mir_token";
const MIR_STAKING: &str = "mir_staking";
const TERRA_SWAP: &str = "terra_swap";
const TEST_CREATOR: &str = "creator";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";
const MIR_LP: &str = "mir_lp";
const SPY_TOKEN: &str = "spy_token";
const SPY_LP: &str = "spy_lp";
const INVALID_LP: &str = "invalid_lp";

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
}

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_bond(&mut deps);
    test_deposit_fee(&mut deps);
    test_staked_reward(&mut deps);
    test_reallocate(&mut deps);
    test_partial_withdraw(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test instantiate & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        mirror_gov: MIR_GOV.to_string(),
        mirror_token: MIR_TOKEN.to_string(),
        mirror_staking: MIR_STAKING.to_string(),
        terraswap_factory: TERRA_SWAP.to_string(),
        platform: TEST_CREATOR.to_string(),
        controller: TEST_CREATOR.to_string(),
        base_denom: "uusd".to_string(),
        community_fee: Decimal::zero(),
        platform_fee: Decimal::zero(),
        controller_fee: Decimal::zero(),
        deposit_fee: Decimal::zero(),
    };

    // success instantiate
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
            earning_spec: Uint128::zero(),
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
        asset_token: MIR_TOKEN.to_string(),
        staking_token: INVALID_LP.to_string(),
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
                asset_token: MIR_TOKEN.to_string(),
                staking_token: INVALID_LP.to_string(),
                weight: 1u32,
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

    // update staking token
    let msg = ExecuteMsg::register_asset {
        asset_token: MIR_TOKEN.to_string(),
        staking_token: MIR_LP.to_string(),
        weight: 1u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query pool info
    let msg = QueryMsg::pools {};
    let res: PoolsResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PoolsResponse {
            pools: vec![PoolItem {
                asset_token: MIR_TOKEN.to_string(),
                staking_token: MIR_LP.to_string(),
                weight: 1u32,
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
    let msg = ExecuteMsg::register_asset {
        asset_token: SPY_TOKEN.to_string(),
        staking_token: SPY_LP.to_string(),
        weight: 2u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 3u32);
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
            asset_token: MIR_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(60)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // bond success
    let info = mock_info(MIR_LP, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let config = read_config(deps.as_ref().storage).unwrap();
    deposit_farm_share(
        deps.as_mut(),
        &config,
        vec![(MIR_TOKEN.to_string(), Uint128::from(1000u128))],
    )
    .unwrap();
    deps.querier.with_token_balances(&[
        (
            &MIR_STAKING.to_string(),
            &[
                (&MIR_TOKEN.to_string(), &Uint128::from(12000u128)),
                (&SPY_TOKEN.to_string(), &Uint128::from(5000u128)),
            ],
        ),
        (
            &MIR_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(2700u128))],
        ),
    ]);

    // query balance
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: MIR_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(1000u128),
            pending_spec_reward: Uint128::from(900u128),
            bond_amount: Uint128::from(12000u128),
            auto_bond_amount: Uint128::from(8000u128),
            stake_bond_amount: Uint128::from(4000u128),
        },]
    );

    // update staking token
    let msg = ExecuteMsg::register_asset {
        asset_token: MIR_TOKEN.to_string(),
        staking_token: INVALID_LP.to_string(),
        weight: 1u32,
    };
    let info = mock_info(SPEC_GOV, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert_eq!(res, Err(StdError::generic_err("pool is not empty")));

    // bond SPY
    let info = mock_info(SPY_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(4000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: SPY_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(50)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // unbond
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unbond {
        asset_token: MIR_TOKEN.to_string(),
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
                contract_addr: MIR_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&MirrorStakingExecuteMsg::unbond {
                    amount: Uint128::from(3000u128),
                    asset_token: MIR_TOKEN.to_string(),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_LP.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
        ]
    );

    // withdraw
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
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_GOV.to_string(),
                funds: vec![],
                msg: to_binary(&MirrorGovExecuteMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(1000u128)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(1000u128),
                })
                .unwrap(),
            }),
        ]
    );

    deposit_farm_share(
        deps.as_mut(),
        &config,
        vec![
            (MIR_TOKEN.to_string(), Uint128::from(500u128)),
            (SPY_TOKEN.to_string(), Uint128::from(1000u128)),
        ],
    )
    .unwrap();
    deps.querier.with_token_balances(&[
        (
            &MIR_STAKING.to_string(),
            &[
                (&MIR_TOKEN.to_string(), &Uint128::from(10000u128)),
                (&SPY_TOKEN.to_string(), &Uint128::from(6000u128)),
            ],
        ),
        (
            &MIR_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(3000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1800u128))],
        ),
    ]);

    // query balance
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(2000u128),
                pending_spec_reward: Uint128::from(1200u128),
                bond_amount: Uint128::from(6000u128),
                auto_bond_amount: Uint128::from(4000u128),
                stake_bond_amount: Uint128::from(2000u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(998u128),
                pending_spec_reward: Uint128::from(599u128),
                bond_amount: Uint128::from(10000u128),
                auto_bond_amount: Uint128::from(7000u128),
                stake_bond_amount: Uint128::from(3000u128),
            },
        ]
    );

    // bond user2
    let info = mock_info(MIR_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(5000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: MIR_TOKEN.to_string(),
            compound_rate: None,
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let info = mock_info(SPY_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(4000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: SPY_TOKEN.to_string(),
            compound_rate: None,
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    deposit_farm_share(
        deps.as_mut(),
        &config,
        vec![
            (MIR_TOKEN.to_string(), Uint128::from(4000u128)),
            (SPY_TOKEN.to_string(), Uint128::from(7200u128)),
        ],
    )
    .unwrap();
    deps.querier.with_token_balances(&[
        (
            &MIR_STAKING.to_string(),
            &[
                (&MIR_TOKEN.to_string(), &Uint128::from(16000u128)),
                (&SPY_TOKEN.to_string(), &Uint128::from(12000u128)),
            ],
        ),
        (
            &MIR_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(14200u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(16200u128))],
        ),
    ]);

    /*
        UNIT: 100
        MIR balance: 142 = existing MIR: 30 + new MIR: 112
        new MIR: 112 (MIR pool = 40, SPY pool = 72)
        new MIR on MIR pool: 40 (USER1 = 15, USER2 = 25) from stake USER1 = 30 & USER2 = 50
        new MIR on SPY pool: 72 (USER1 = 24, USER2 = 48) from stake USER1 = 20 & USER2 = 40
        USER1 MIR total: 25 (existing MIR = 10, new on MIR pool = 15)
        USER1 SPY total: 44 (existing MIR = 20, new on SPY pool = 24)
        USER2 MIR total: 25 (new on MIR pool = 25)
        USER2 SPY total: 48 (new on SPY pool = 48)

        existing SPEC: 18 (MIR pool = 6, SPY pool = 12)
        SPEC balance: 162 - existing SPEC: 18 = new SPEC: 144
        new SPEC: 144 (MIR pool = 48, SPY pool = 96) from weight MIR pool = 1 & SPY pool = 2
        new SPEC on MIR pool: 48 (USER1 = 33, USER2 = 15) from bond USER1 = 110 & USER2 = 50
        new SPEC on SPY pool: 96 (USER1 = 64, USER2 = 32) from bond USER1 = 80 & USER2 = 40
        USER1 MIR total: 39 (existing MIR = 6, new on MIR pool = 33)
        USER1 SPY total: 76 (existing MIR = 12, new on SPY pool = 64)
        USER2 MIR total: 15 (new on MIR pool = 15)
        USER2 SPY total: 32 (new on SPY pool = 32)
    */

    // query balance1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(4400u128),
                pending_spec_reward: Uint128::from(7600u128),
                bond_amount: Uint128::from(8000u128),
                auto_bond_amount: Uint128::from(6000u128),
                stake_bond_amount: Uint128::from(2000u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(2498u128),
                pending_spec_reward: Uint128::from(3899u128),
                bond_amount: Uint128::from(11000u128),
                auto_bond_amount: Uint128::from(8000u128),
                stake_bond_amount: Uint128::from(3000u128),
            },
        ]
    );

    // query balance2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(4800u128),
                pending_spec_reward: Uint128::from(3200u128),
                bond_amount: Uint128::from(4000u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(4000u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(2500u128),
                pending_spec_reward: Uint128::from(1500u128),
                bond_amount: Uint128::from(5000u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(5000u128),
            },
        ]
    );
}

fn test_deposit_fee(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // alter config, deposit fee
    let env = mock_env();
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::update_config {
        owner: None,
        controller: None,
        community_fee: None,
        platform_fee: None,
        controller_fee: None,
        deposit_fee: Some(Decimal::percent(20)),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // bond user3
    let info = mock_info(MIR_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER3.to_string(),
        amount: Uint128::from(80000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: MIR_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(50)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let info = mock_info(SPY_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER3.to_string(),
        amount: Uint128::from(60000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: SPY_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(50)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[
        (
            &MIR_STAKING.to_string(),
            &[
                (&MIR_TOKEN.to_string(), &Uint128::from(96000u128)),
                (&SPY_TOKEN.to_string(), &Uint128::from(72000u128)),
            ],
        ),
        (
            &MIR_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(14200u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(16200u128))],
        ),
    ]);

    // query balance1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(4400u128),
                pending_spec_reward: Uint128::from(7600u128),
                bond_amount: Uint128::from(9600u128),
                auto_bond_amount: Uint128::from(7200u128),
                stake_bond_amount: Uint128::from(2400u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(2498u128),
                pending_spec_reward: Uint128::from(3899u128),
                bond_amount: Uint128::from(13200u128),
                auto_bond_amount: Uint128::from(9600u128),
                stake_bond_amount: Uint128::from(3600u128),
            },
        ]
    );

    // query balance2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(4800u128),
                pending_spec_reward: Uint128::from(3200u128),
                bond_amount: Uint128::from(4800u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(4800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(2500u128),
                pending_spec_reward: Uint128::from(1500u128),
                bond_amount: Uint128::from(6000u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(6000u128),
            },
        ]
    );

    // query balance3
    let msg = QueryMsg::reward_info {
        staker_addr: USER3.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::zero(),
                pending_spec_reward: Uint128::zero(),
                bond_amount: Uint128::from(57600u128),
                auto_bond_amount: Uint128::from(28800u128),
                stake_bond_amount: Uint128::from(28800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::zero(),
                pending_spec_reward: Uint128::zero(),
                bond_amount: Uint128::from(76800u128),
                auto_bond_amount: Uint128::from(38400u128),
                stake_bond_amount: Uint128::from(38400u128),
            },
        ]
    );
}

fn test_staked_reward(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // unbond user1
    let env = mock_env();
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unbond {
        asset_token: MIR_TOKEN.to_string(),
        amount: Uint128::from(13199u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap()
            .messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&MirrorStakingExecuteMsg::unbond {
                    amount: Uint128::from(13199u128),
                    asset_token: MIR_TOKEN.to_string(),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_LP.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(13199u128),
                })
                .unwrap(),
            }),
        ]
    );

    // withdraw for user2
    let info = mock_info(USER2, &[]);
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
                    amount: Some(Uint128::from(4700u128)),
                    days: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER2.to_string(),
                    amount: Uint128::from(4700u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_GOV.to_string(),
                funds: vec![],
                msg: to_binary(&MirrorGovExecuteMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(7300u128)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER2.to_string(),
                    amount: Uint128::from(7300u128),
                })
                .unwrap(),
            }),
        ]
    );
    deps.querier.with_balance_percent(120);
    deps.querier.with_token_balances(&[
        (
            &MIR_STAKING.to_string(),
            &[
                (&MIR_TOKEN.to_string(), &Uint128::from(90000u128)),
                (&SPY_TOKEN.to_string(), &Uint128::from(72000u128)),
            ],
        ),
        (
            &MIR_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(9200u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(24600u128), //+9000 +20%
            )],
        ),
    ]);

    // query balance1 (still earn gov income even there is no bond)
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(5866u128), //+33%
                pending_spec_reward: Uint128::from(10080u128), //+800+20%
                bond_amount: Uint128::from(9600u128),
                auto_bond_amount: Uint128::from(7200u128),
                stake_bond_amount: Uint128::from(2400u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(3330u128), //+33%
                pending_spec_reward: Uint128::from(4678u128), //+20%
                bond_amount: Uint128::from(0u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(0u128),
            },
        ]
    );

    // query balance2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(0u128),
                pending_spec_reward: Uint128::from(480u128), //+400+20%
                bond_amount: Uint128::from(4800u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(4800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(0u128),
                pending_spec_reward: Uint128::from(240u128), //+200+20%
                bond_amount: Uint128::from(6000u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(6000u128),
            },
        ]
    );

    // query balance3
    let msg = QueryMsg::reward_info {
        staker_addr: USER3.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::zero(),
                pending_spec_reward: Uint128::from(5760u128), //+4800+20%
                bond_amount: Uint128::from(57600u128),
                auto_bond_amount: Uint128::from(28800u128),
                stake_bond_amount: Uint128::from(28800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::zero(),
                pending_spec_reward: Uint128::from(3358u128), //+2799+20%
                bond_amount: Uint128::from(84000u128),
                auto_bond_amount: Uint128::from(45600u128),
                stake_bond_amount: Uint128::from(38400u128),
            },
        ]
    );
}

fn test_reallocate(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // unbond user1
    let env = mock_env();
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::update_bond {
        asset_token: SPY_TOKEN.to_string(),
        amount_to_auto: Uint128::from(4801u128),
        amount_to_stake: Uint128::from(4800u128),
    };

    // cannot reallocate more than user have
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // reallocate half/half
    let msg = ExecuteMsg::update_bond {
        asset_token: SPY_TOKEN.to_string(),
        amount_to_auto: Uint128::from(1200u128),
        amount_to_stake: Uint128::from(3600u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // query balance1 (still earn gov income even there is no bond)
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(5866u128), //+33%
                pending_spec_reward: Uint128::from(10080u128), //+800+20%
                bond_amount: Uint128::from(9598u128),
                auto_bond_amount: Uint128::from(4798u128),
                stake_bond_amount: Uint128::from(4800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(3330u128), //+33%
                pending_spec_reward: Uint128::from(4678u128), //+20%
                bond_amount: Uint128::from(0u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(0u128),
            },
        ]
    );

    // query balance2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(0u128),
                pending_spec_reward: Uint128::from(480u128), //+400+20%
                bond_amount: Uint128::from(4800u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(4800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(0u128),
                pending_spec_reward: Uint128::from(240u128), //+200+20%
                bond_amount: Uint128::from(6000u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(6000u128),
            },
        ]
    );

    // query balance3
    let msg = QueryMsg::reward_info {
        staker_addr: USER3.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::zero(),
                pending_spec_reward: Uint128::from(5760u128), //+4800+20%
                bond_amount: Uint128::from(57601u128),
                auto_bond_amount: Uint128::from(28801u128),
                stake_bond_amount: Uint128::from(28800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::zero(),
                pending_spec_reward: Uint128::from(3358u128), //+2799+20%
                bond_amount: Uint128::from(84000u128),
                auto_bond_amount: Uint128::from(45600u128),
                stake_bond_amount: Uint128::from(38400u128),
            },
        ]
    );
}

fn clone_storage(storage: &MockStorage) -> MockStorage {
    let range = storage.range(None, None, cosmwasm_std::Order::Ascending);
    let mut cloned = MockStorage::new();
    for item in range {
        cloned.set(item.0.as_slice(), item.1.as_slice());
    }
    cloned
}

fn test_partial_withdraw(mut deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(USER1, &[]);

    // withdraw more than available
    let old_storage = clone_storage(&deps.storage);
    let msg = ExecuteMsg::withdraw {
        asset_token: None,
        farm_amount: Some(Uint128::from(9197u128)),
        spec_amount: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(res, Err(StdError::generic_err("Cannot withdraw more than remaining amount")));
    deps.storage = old_storage;

    // withdraw more than available2
    let old_storage = clone_storage(&deps.storage);
    let msg = ExecuteMsg::withdraw {
        asset_token: None,
        farm_amount: None,
        spec_amount: Some(Uint128::from(14759u128)),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(res, Err(StdError::generic_err("Cannot withdraw more than remaining amount")));
    deps.storage = old_storage;

    // withdraw partial
    let msg = ExecuteMsg::withdraw {
        asset_token: None,
        farm_amount: Some(Uint128::from(9000u128)),
        spec_amount: Some(Uint128::zero()),
    };
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
                contract_addr: MIR_GOV.to_string(),
                funds: vec![],
                msg: to_binary(&MirrorGovExecuteMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(9000u128)),
                })
                    .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(9000u128),
                })
                    .unwrap(),
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &MIR_STAKING.to_string(),
            &[
                (&MIR_TOKEN.to_string(), &Uint128::from(90000u128)),
                (&SPY_TOKEN.to_string(), &Uint128::from(72000u128)),
            ],
        ),
        (
            &MIR_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(199u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(
                &MOCK_CONTRACT_ADDR.to_string(),
                &Uint128::from(24600u128), //+9000 +20%
            )],
        ),
    ]);

    // query balance1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![
            RewardInfoResponseItem {
                asset_token: SPY_TOKEN.to_string(),
                pending_farm_reward: Uint128::zero(),
                pending_spec_reward: Uint128::from(10080u128),
                bond_amount: Uint128::from(9598u128),
                auto_bond_amount: Uint128::from(4798u128),
                stake_bond_amount: Uint128::from(4800u128),
            },
            RewardInfoResponseItem {
                asset_token: MIR_TOKEN.to_string(),
                pending_farm_reward: Uint128::from(196u128),
                pending_spec_reward: Uint128::from(4678u128),
                bond_amount: Uint128::from(0u128),
                auto_bond_amount: Uint128::from(0u128),
                stake_bond_amount: Uint128::from(0u128),
            },
        ]
    );
}
