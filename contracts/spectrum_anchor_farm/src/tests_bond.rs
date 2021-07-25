use crate::bond::deposit_farm_share;
use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::read_config;
use anchor_token::gov::{
    HandleMsg as AnchorGovHandleMsg,
};
use anchor_token::staking::{
    HandleMsg as AnchorStakingHandleMsg,
};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, CosmosMsg, Decimal, Extern, HumanAddr, Uint128, WasmMsg,
};
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::anchor_farm::{
    ConfigInfo, Cw20HookMsg, HandleMsg, PoolItem,
    PoolsResponse, QueryMsg, StateInfo,
};
use spectrum_protocol::gov::HandleMsg as GovHandleMsg;
use std::fmt::Debug;

const SPEC_GOV: &str = "spec_gov";
const SPEC_TOKEN: &str = "spec_token";
const ANC_GOV: &str = "anc_gov";
const ANC_TOKEN: &str = "anc_token";
const ANC_STAKING: &str = "anc_staking";
const TERRA_SWAP: &str = "terra_swap";
const TEST_CREATOR: &str = "creator";
const USER1: &str = "user1";
const USER2: &str = "user2";
const ANC_LP: &str = "anc_lp";
const SPY_TOKEN: &str = "spy_token";
const SPY_LP: &str = "spy_lp";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: HumanAddr,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: HumanAddr,
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

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_bond(&mut deps);
    // test_deposit_fee(&mut deps);
    // test_staked_reward(&mut deps);
}

fn test_config(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: HumanAddr::from(TEST_CREATOR),
        spectrum_gov: HumanAddr::from(SPEC_GOV),
        spectrum_token: HumanAddr::from(SPEC_TOKEN),
        anchor_gov: HumanAddr::from(ANC_GOV),
        anchor_token: HumanAddr::from(ANC_TOKEN),
        anchor_staking: HumanAddr::from(ANC_STAKING),
        terraswap_factory: HumanAddr::from(TERRA_SWAP),
        platform: Option::None,
        controller: Option::None,
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
        owner: Some(HumanAddr::from(SPEC_GOV)),
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
    config.owner = HumanAddr::from(SPEC_GOV);
    assert_eq!(res, config.clone());

    config
}

fn test_register_asset(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // no permission
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::register_asset {
        asset_token: HumanAddr::from(ANC_TOKEN),
        staking_token: HumanAddr::from(ANC_LP),
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
                asset_token: HumanAddr::from(ANC_TOKEN),
                staking_token: HumanAddr::from(ANC_LP),
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

    // register again should fail
    let msg = HandleMsg::register_asset {
        asset_token: HumanAddr::from(SPY_TOKEN),
        staking_token: HumanAddr::from(SPY_LP),
        weight: 1u32,
        auto_compound: true,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 1u32);
}

fn test_bond(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // bond err
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(USER1),
        amount: Uint128::from(10000u128),
        msg: Some(
            to_binary(&Cw20HookMsg::bond {
                staker_addr: None,
                asset_token: HumanAddr::from(ANC_TOKEN),
                compound_rate: Some(Decimal::percent(60)),
            })
            .unwrap(),
        ),
    });
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // bond success
    let env = mock_env(ANC_LP, &[]);
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    deposit_farm_share(deps, &config, Uint128::from(500u128)).unwrap();
    deps.querier.with_token_balances(&[
        (
            &HumanAddr::from(ANC_STAKING),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128::from(10000u128),
            )],
        ),
        (
            &HumanAddr::from(ANC_GOV),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128::from(1000u128),
            )],
        ),
        (
            &HumanAddr::from(SPEC_GOV),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128::from(2700u128),
            )],
        ),
    ]);

    // query balance
    let msg = QueryMsg::reward_info {
        staker_addr: HumanAddr::from(USER1),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: HumanAddr::from(ANC_TOKEN),
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

    // // bond SPY
    // let env = mock_env(SPY_LP, &[]);
    // let msg = HandleMsg::receive(Cw20ReceiveMsg {
    //     sender: HumanAddr::from(USER1),
    //     amount: Uint128::from(4000u128),
    //     msg: Some(
    //         to_binary(&Cw20HookMsg::bond {
    //             staker_addr: None,
    //             asset_token: HumanAddr::from(SPY_TOKEN),
    //             compound_rate: Some(Decimal::percent(50)),
    //         })
    //         .unwrap(),
    //     ),
    // });
    // let res = handle(deps, env.clone(), msg.clone());
    // assert!(res.is_ok());

    // unbond
    let env = mock_env(USER1, &[]);
    let msg = HandleMsg::unbond {
        asset_token: HumanAddr::from(ANC_TOKEN),
        amount: Uint128::from(3000u128),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages,
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(ANC_STAKING),
                send: vec![],
                msg: to_binary(&AnchorStakingHandleMsg::Unbond {
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(ANC_LP),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: HumanAddr::from(USER1),
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
        ]
    );

    // withdraw
    let msg = HandleMsg::withdraw { asset_token: None };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(SPEC_GOV),
                send: vec![],
                msg: to_binary(&GovHandleMsg::withdraw {
                    amount: Some(Uint128::from(2700u128)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(SPEC_TOKEN),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: HumanAddr::from(USER1),
                    amount: Uint128::from(2700u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(ANC_GOV),
                send: vec![],
                msg: to_binary(&AnchorGovHandleMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(1000u128)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(ANC_TOKEN),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: HumanAddr::from(USER1),
                    amount: Uint128::from(1000u128),
                })
                .unwrap(),
            }),
        ]
    );

    // deposit_farm_share(
    //     deps,
    //     &config,
    //     vec![
    //         (HumanAddr::from(MIR_TOKEN), Uint128::from(500u128)),
    //         (HumanAddr::from(SPY_TOKEN), Uint128::from(1000u128)),
    //     ],
    // )
    // .unwrap();
    // deps.querier.with_token_balances(&[
    //     (
    //         &HumanAddr::from(MIR_STAKING),
    //         &[
    //             (&HumanAddr::from(MIR_TOKEN), &Uint128::from(10000u128)),
    //             (&HumanAddr::from(SPY_TOKEN), &Uint128::from(6000u128)),
    //         ],
    //     ),
    //     (
    //         &HumanAddr::from(MIR_GOV),
    //         &[(
    //             &HumanAddr::from(MOCK_CONTRACT_ADDR),
    //             &Uint128::from(3000u128),
    //         )],
    //     ),
    //     (
    //         &HumanAddr::from(SPEC_GOV),
    //         &[(
    //             &HumanAddr::from(MOCK_CONTRACT_ADDR),
    //             &Uint128::from(1800u128),
    //         )],
    //     ),
    // ]);

    deps.querier.with_token_balances(&[
        (
            &HumanAddr::from(ANC_STAKING),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128::from(7000u128),
            )],
        ),
        (
            &HumanAddr::from(ANC_GOV),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128::from(0u128))],
        ),
        (
            &HumanAddr::from(SPEC_GOV),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128::from(0u128))],
        ),
    ]);

    // query balance
    let msg = QueryMsg::reward_info {
        staker_addr: HumanAddr::from(USER2),
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![]
    );

        // query balance
        let msg = QueryMsg::reward_info {
            staker_addr: HumanAddr::from(USER1),
            height: 0u64,
        };
        let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
        assert_eq!(
            res.reward_infos,
            vec![RewardInfoResponseItem {
                asset_token: HumanAddr::from(ANC_TOKEN),
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
    

    // // bond user2
    // let env = mock_env(MIR_LP, &[]);
    // let msg = HandleMsg::receive(Cw20ReceiveMsg {
    //     sender: HumanAddr::from(USER2),
    //     amount: Uint128::from(5000u128),
    //     msg: Some(
    //         to_binary(&Cw20HookMsg::bond {
    //             staker_addr: None,
    //             asset_token: HumanAddr::from(MIR_TOKEN),
    //             compound_rate: None,
    //         })
    //         .unwrap(),
    //     ),
    // });
    // let res = handle(deps, env.clone(), msg);
    // assert!(res.is_ok());

    // let env = mock_env(SPY_LP, &[]);
    // let msg = HandleMsg::receive(Cw20ReceiveMsg {
    //     sender: HumanAddr::from(USER2),
    //     amount: Uint128::from(4000u128),
    //     msg: Some(
    //         to_binary(&Cw20HookMsg::bond {
    //             staker_addr: None,
    //             asset_token: HumanAddr::from(SPY_TOKEN),
    //             compound_rate: None,
    //         })
    //         .unwrap(),
    //     ),
    // });
    // let res = handle(deps, env.clone(), msg);
    // assert!(res.is_ok());

    // deposit_farm_share(
    //     deps,
    //     &config,
    //     vec![
    //         (HumanAddr::from(MIR_TOKEN), Uint128::from(4000u128)),
    //         (HumanAddr::from(SPY_TOKEN), Uint128::from(7200u128)),
    //     ],
    // )
    // .unwrap();
    // deps.querier.with_token_balances(&[
    //     (
    //         &HumanAddr::from(MIR_STAKING),
    //         &[
    //             (&HumanAddr::from(MIR_TOKEN), &Uint128::from(16000u128)),
    //             (&HumanAddr::from(SPY_TOKEN), &Uint128::from(12000u128)),
    //         ],
    //     ),
    //     (
    //         &HumanAddr::from(MIR_GOV),
    //         &[(
    //             &HumanAddr::from(MOCK_CONTRACT_ADDR),
    //             &Uint128::from(14200u128),
    //         )],
    //     ),
    //     (
    //         &HumanAddr::from(SPEC_GOV),
    //         &[(
    //             &HumanAddr::from(MOCK_CONTRACT_ADDR),
    //             &Uint128::from(16200u128),
    //         )],
    //     ),
    // ]);

    // /*
    //     UNIT: 100
    //     MIR balance: 142 = existing MIR: 30 + new MIR: 112
    //     new MIR: 112 (MIR pool = 40, SPY pool = 72)
    //     new MIR on MIR pool: 40 (USER1 = 15, USER2 = 25) from stake USER1 = 30 & USER2 = 50
    //     new MIR on SPY pool: 72 (USER1 = 24, USER2 = 48) from stake USER1 = 20 & USER2 = 40
    //     USER1 MIR total: 25 (existing MIR = 10, new on MIR pool = 15)
    //     USER1 SPY total: 44 (existing MIR = 20, new on SPY pool = 24)
    //     USER2 MIR total: 25 (new on MIR pool = 25)
    //     USER2 SPY total: 48 (new on SPY pool = 48)

    //     existing SPEC: 18 (MIR pool = 6, SPY pool = 12)
    //     SPEC balance: 162 - existing SPEC: 18 = new SPEC: 144
    //     new SPEC: 144 (MIR pool = 48, SPY pool = 96) from weight MIR pool = 1 & SPY pool = 2
    //     new SPEC on MIR pool: 48 (USER1 = 33, USER2 = 15) from bond USER1 = 110 & USER2 = 50
    //     new SPEC on SPY pool: 96 (USER1 = 64, USER2 = 32) from bond USER1 = 80 & USER2 = 40
    //     USER1 MIR total: 39 (existing MIR = 6, new on MIR pool = 33)
    //     USER1 SPY total: 76 (existing MIR = 12, new on SPY pool = 64)
    //     USER2 MIR total: 15 (new on MIR pool = 15)
    //     USER2 SPY total: 32 (new on SPY pool = 32)
    // */

    // // query balance1
    // let msg = QueryMsg::reward_info {
    //     staker_addr: HumanAddr::from(USER1),
    //     asset_token: None,
    //     height: 0u64,
    // };
    // let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    // assert_eq!(
    //     res.reward_infos,
    //     vec![
    //         RewardInfoResponseItem {
    //             asset_token: HumanAddr::from(MIR_TOKEN),
    //             pending_farm_reward: Uint128::from(2498u128),
    //             pending_spec_reward: Uint128::from(3899u128),
    //             bond_amount: Uint128::from(11000u128),
    //             auto_bond_amount: Uint128::from(8000u128),
    //             stake_bond_amount: Uint128::from(3000u128),
    //             accum_spec_share: Uint128::from(4799u128),
    //         },
    //         RewardInfoResponseItem {
    //             asset_token: HumanAddr::from(SPY_TOKEN),
    //             pending_farm_reward: Uint128::from(4400u128),
    //             pending_spec_reward: Uint128::from(7600u128),
    //             bond_amount: Uint128::from(8000u128),
    //             auto_bond_amount: Uint128::from(6000u128),
    //             stake_bond_amount: Uint128::from(2000u128),
    //             accum_spec_share: Uint128::from(9400u128),
    //         },
    //     ]
    // );

    // // query balance2
    // let msg = QueryMsg::reward_info {
    //     staker_addr: HumanAddr::from(USER2),
    //     asset_token: None,
    //     height: 0u64,
    // };
    // let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    // assert_eq!(
    //     res.reward_infos,
    //     vec![
    //         RewardInfoResponseItem {
    //             asset_token: HumanAddr::from(MIR_TOKEN),
    //             pending_farm_reward: Uint128::from(2500u128),
    //             pending_spec_reward: Uint128::from(1500u128),
    //             bond_amount: Uint128::from(5000u128),
    //             auto_bond_amount: Uint128::from(0u128),
    //             stake_bond_amount: Uint128::from(5000u128),
    //             accum_spec_share: Uint128::from(1500u128),
    //         },
    //         RewardInfoResponseItem {
    //             asset_token: HumanAddr::from(SPY_TOKEN),
    //             pending_farm_reward: Uint128::from(4800u128),
    //             pending_spec_reward: Uint128::from(3200u128),
    //             bond_amount: Uint128::from(4000u128),
    //             auto_bond_amount: Uint128::from(0u128),
    //             stake_bond_amount: Uint128::from(4000u128),
    //             accum_spec_share: Uint128::from(3200u128),
    //         },
    //     ]
    // );
}

// fn test_deposit_fee(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
//     // alter config, deposit fee
//     let env = mock_env(SPEC_GOV, &[]);
//     let msg = HandleMsg::update_config {
//         owner: None,
//         platform: None,
//         controller: None,
//         community_fee: None,
//         platform_fee: None,
//         controller_fee: None,
//         deposit_fee: Some(Decimal::percent(20)),
//         lock_start: None,
//         lock_end: None,
//     };
//     let res = handle(deps, env.clone(), msg.clone());
//     assert!(res.is_ok());

//     // bond user3
//     let env = mock_env(MIR_LP, &[]);
//     let msg = HandleMsg::receive(Cw20ReceiveMsg {
//         sender: HumanAddr::from(USER3),
//         amount: Uint128::from(80000u128),
//         msg: Some(
//             to_binary(&Cw20HookMsg::bond {
//                 staker_addr: None,
//                 asset_token: HumanAddr::from(MIR_TOKEN),
//                 compound_rate: Some(Decimal::percent(50)),
//             })
//             .unwrap(),
//         ),
//     });
//     let res = handle(deps, env.clone(), msg);
//     assert!(res.is_ok());

//     let env = mock_env(SPY_LP, &[]);
//     let msg = HandleMsg::receive(Cw20ReceiveMsg {
//         sender: HumanAddr::from(USER3),
//         amount: Uint128::from(60000u128),
//         msg: Some(
//             to_binary(&Cw20HookMsg::bond {
//                 staker_addr: None,
//                 asset_token: HumanAddr::from(SPY_TOKEN),
//                 compound_rate: Some(Decimal::percent(50)),
//             })
//             .unwrap(),
//         ),
//     });
//     let res = handle(deps, env.clone(), msg);
//     assert!(res.is_ok());

//     deps.querier.with_token_balances(&[
//         (
//             &HumanAddr::from(MIR_STAKING),
//             &[
//                 (&HumanAddr::from(MIR_TOKEN), &Uint128::from(96000u128)),
//                 (&HumanAddr::from(SPY_TOKEN), &Uint128::from(72000u128)),
//             ],
//         ),
//         (
//             &HumanAddr::from(MIR_GOV),
//             &[(
//                 &HumanAddr::from(MOCK_CONTRACT_ADDR),
//                 &Uint128::from(14200u128),
//             )],
//         ),
//         (
//             &HumanAddr::from(SPEC_GOV),
//             &[(
//                 &HumanAddr::from(MOCK_CONTRACT_ADDR),
//                 &Uint128::from(16200u128),
//             )],
//         ),
//     ]);

//     // query balance1
//     let msg = QueryMsg::reward_info {
//         staker_addr: HumanAddr::from(USER1),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(MIR_TOKEN),
//                 pending_farm_reward: Uint128::from(2498u128),
//                 pending_spec_reward: Uint128::from(3899u128),
//                 bond_amount: Uint128::from(13200u128),
//                 auto_bond_amount: Uint128::from(9600u128),
//                 stake_bond_amount: Uint128::from(3600u128),
//                 accum_spec_share: Uint128::from(4799u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(SPY_TOKEN),
//                 pending_farm_reward: Uint128::from(4400u128),
//                 pending_spec_reward: Uint128::from(7600u128),
//                 bond_amount: Uint128::from(9600u128),
//                 auto_bond_amount: Uint128::from(7200u128),
//                 stake_bond_amount: Uint128::from(2400u128),
//                 accum_spec_share: Uint128::from(9400u128),
//             },
//         ]
//     );

//     // query balance2
//     let msg = QueryMsg::reward_info {
//         staker_addr: HumanAddr::from(USER2),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(MIR_TOKEN),
//                 pending_farm_reward: Uint128::from(2500u128),
//                 pending_spec_reward: Uint128::from(1500u128),
//                 bond_amount: Uint128::from(6000u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(6000u128),
//                 accum_spec_share: Uint128::from(1500u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(SPY_TOKEN),
//                 pending_farm_reward: Uint128::from(4800u128),
//                 pending_spec_reward: Uint128::from(3200u128),
//                 bond_amount: Uint128::from(4800u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(4800u128),
//                 accum_spec_share: Uint128::from(3200u128),
//             },
//         ]
//     );

//     // query balance3
//     let msg = QueryMsg::reward_info {
//         staker_addr: HumanAddr::from(USER3),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(MIR_TOKEN),
//                 pending_farm_reward: Uint128::zero(),
//                 pending_spec_reward: Uint128::zero(),
//                 bond_amount: Uint128::from(76800u128),
//                 auto_bond_amount: Uint128::from(38400u128),
//                 stake_bond_amount: Uint128::from(38400u128),
//                 accum_spec_share: Uint128::zero(),
//             },
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(SPY_TOKEN),
//                 pending_farm_reward: Uint128::zero(),
//                 pending_spec_reward: Uint128::zero(),
//                 bond_amount: Uint128::from(57600u128),
//                 auto_bond_amount: Uint128::from(28800u128),
//                 stake_bond_amount: Uint128::from(28800u128),
//                 accum_spec_share: Uint128::zero(),
//             },
//         ]
//     );
// }

// fn test_staked_reward(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
//     // unbond user1
//     let env = mock_env(USER1, &[]);
//     let msg = HandleMsg::unbond {
//         asset_token: HumanAddr::from(MIR_TOKEN),
//         amount: Uint128::from(13200u128),
//     };
//     let res = handle(deps, env.clone(), msg);
//     assert!(res.is_ok());
//     assert_eq!(
//         res.unwrap().messages,
//         [
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: HumanAddr::from(MIR_STAKING),
//                 send: vec![],
//                 msg: to_binary(&MirrorStakingHandleMsg::unbond {
//                     amount: Uint128::from(13200u128),
//                     asset_token: HumanAddr::from(MIR_TOKEN),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: HumanAddr::from(MIR_LP),
//                 send: vec![],
//                 msg: to_binary(&Cw20HandleMsg::Transfer {
//                     recipient: HumanAddr::from(USER1),
//                     amount: Uint128::from(13200u128),
//                 })
//                 .unwrap(),
//             }),
//         ]
//     );

//     // withdraw for user2
//     let env = mock_env(USER2, &[]);
//     let msg = HandleMsg::withdraw { asset_token: None };
//     let res = handle(deps, env.clone(), msg);
//     assert!(res.is_ok());
//     assert_eq!(
//         res.unwrap().messages,
//         vec![
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: HumanAddr::from(SPEC_GOV),
//                 send: vec![],
//                 msg: to_binary(&GovHandleMsg::withdraw {
//                     amount: Some(Uint128::from(4700u128)),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: HumanAddr::from(SPEC_TOKEN),
//                 send: vec![],
//                 msg: to_binary(&Cw20HandleMsg::Transfer {
//                     recipient: HumanAddr::from(USER2),
//                     amount: Uint128::from(4700u128),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: HumanAddr::from(MIR_GOV),
//                 send: vec![],
//                 msg: to_binary(&MirrorGovHandleMsg::WithdrawVotingTokens {
//                     amount: Some(Uint128::from(7300u128)),
//                 })
//                 .unwrap(),
//             }),
//             CosmosMsg::Wasm(WasmMsg::Execute {
//                 contract_addr: HumanAddr::from(MIR_TOKEN),
//                 send: vec![],
//                 msg: to_binary(&Cw20HandleMsg::Transfer {
//                     recipient: HumanAddr::from(USER2),
//                     amount: Uint128::from(7300u128),
//                 })
//                 .unwrap(),
//             }),
//         ]
//     );
//     deps.querier.with_balance_percent(120);
//     deps.querier.with_token_balances(&[
//         (
//             &HumanAddr::from(MIR_STAKING),
//             &[
//                 (&HumanAddr::from(MIR_TOKEN), &Uint128::from(90000u128)),
//                 (&HumanAddr::from(SPY_TOKEN), &Uint128::from(72000u128)),
//             ],
//         ),
//         (
//             &HumanAddr::from(MIR_GOV),
//             &[(
//                 &HumanAddr::from(MOCK_CONTRACT_ADDR),
//                 &Uint128::from(9200u128),
//             )],
//         ),
//         (
//             &HumanAddr::from(SPEC_GOV),
//             &[(
//                 &HumanAddr::from(MOCK_CONTRACT_ADDR),
//                 &Uint128::from(24600u128), //+9000 +20%
//             )],
//         ),
//     ]);

//     // query balance1 (still earn gov income even there is no bond)
//     let msg = QueryMsg::reward_info {
//         staker_addr: HumanAddr::from(USER1),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(MIR_TOKEN),
//                 pending_farm_reward: Uint128::from(3330u128), //+33%
//                 pending_spec_reward: Uint128::from(4678u128), //+20%
//                 bond_amount: Uint128::from(0u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(0u128),
//                 accum_spec_share: Uint128::from(4799u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(SPY_TOKEN),
//                 pending_farm_reward: Uint128::from(5866u128), //+33%
//                 pending_spec_reward: Uint128::from(10080u128), //+800+20%
//                 bond_amount: Uint128::from(9600u128),
//                 auto_bond_amount: Uint128::from(7200u128),
//                 stake_bond_amount: Uint128::from(2400u128),
//                 accum_spec_share: Uint128::from(10200u128),
//             },
//         ]
//     );

//     // query balance2
//     let msg = QueryMsg::reward_info {
//         staker_addr: HumanAddr::from(USER2),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(MIR_TOKEN),
//                 pending_farm_reward: Uint128::from(0u128),
//                 pending_spec_reward: Uint128::from(240u128), //+200+20%
//                 bond_amount: Uint128::from(6000u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(6000u128),
//                 accum_spec_share: Uint128::from(1700u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(SPY_TOKEN),
//                 pending_farm_reward: Uint128::from(0u128),
//                 pending_spec_reward: Uint128::from(480u128), //+400+20%
//                 bond_amount: Uint128::from(4800u128),
//                 auto_bond_amount: Uint128::from(0u128),
//                 stake_bond_amount: Uint128::from(4800u128),
//                 accum_spec_share: Uint128::from(3600u128),
//             },
//         ]
//     );

//     // query balance3
//     let msg = QueryMsg::reward_info {
//         staker_addr: HumanAddr::from(USER3),
//         asset_token: None,
//         height: 0u64,
//     };
//     let res: RewardInfoResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
//     assert_eq!(
//         res.reward_infos,
//         vec![
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(MIR_TOKEN),
//                 pending_farm_reward: Uint128::zero(),
//                 pending_spec_reward: Uint128::from(3358u128), //+2799+20%
//                 bond_amount: Uint128::from(84000u128),
//                 auto_bond_amount: Uint128::from(45600u128),
//                 stake_bond_amount: Uint128::from(38400u128),
//                 accum_spec_share: Uint128::from(2799u128),
//             },
//             RewardInfoResponseItem {
//                 asset_token: HumanAddr::from(SPY_TOKEN),
//                 pending_farm_reward: Uint128::zero(),
//                 pending_spec_reward: Uint128::from(5760u128), //+4800+20%
//                 bond_amount: Uint128::from(57600u128),
//                 auto_bond_amount: Uint128::from(28800u128),
//                 stake_bond_amount: Uint128::from(28800u128),
//                 accum_spec_share: Uint128::from(4800u128),
//             },
//         ]
//     );
// }
