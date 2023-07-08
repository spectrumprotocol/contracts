use crate::bond::deposit_farm_share;
use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::read_config;
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, Coin, CosmosMsg, Decimal, OwnedDeps, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use mirror_protocol::gov::{
    Cw20HookMsg as MirrorGovCw20HookMsg, ExecuteMsg as MirrorGovExecuteMsg,
};
use mirror_protocol::staking::ExecuteMsg as MirrorStakingExecuteMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use spectrum_protocol::mirror_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use std::fmt::Debug;
use classic_bindings::TerraQuery;
use classic_terraswap::asset::{AssetInfo, PairInfo};
use classic_terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg};

const SPEC_GOV: &str = "spec_gov";
const SPEC_TOKEN: &str = "spec_token";
const MIR_GOV: &str = "mir_gov";
const MIR_TOKEN: &str = "mir_token";
const MIR_STAKING: &str = "mir_staking";
const TERRA_SWAP: &str = "terra_swap";
const TEST_CREATOR: &str = "creator";
const TEST_CONTROLLER: &str = "controller";
const TEST_PLATFORM: &str = "platform";
const MIR_LP: &str = "mir_lp";
const MIR_PAIR_INFO: &str = "mir_pair_info";
const SPY_TOKEN: &str = "spy_token";
const SPY_LP: &str = "spy_lp";
const SPEC_LP: &str = "spec_lp";
const SPEC_PAIR_INFO: &str = "spec_pair_info";
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
    deps.querier.with_terraswap_pairs(&[
        (
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
                contract_addr: MIR_PAIR_INFO.to_string(),
                liquidity_token: MIR_LP.to_string(),
                asset_decimals: [6, 6],
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
                contract_addr: SPEC_PAIR_INFO.to_string(),
                liquidity_token: SPEC_LP.to_string(),
                asset_decimals: [6, 6],
            },
        ),
    ]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1500000u128))],
    );

    test_config(&mut deps);
    test_register_asset(&mut deps);
    test_bond(&mut deps);
    test_harvest_unauthorized(&mut deps);
    test_harvest_all(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) -> ConfigInfo {
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
        platform: TEST_PLATFORM.to_string(),
        controller: TEST_CONTROLLER.to_string(),
        base_denom: "uusd".to_string(),
        community_fee: Decimal::percent(3u64),
        platform_fee: Decimal::percent(1u64),
        controller_fee: Decimal::percent(1u64),
        deposit_fee: Decimal::zero(),
        anchor_market: ANC_MARKET.to_string(),
        aust_token: AUST_TOKEN.to_string(),
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

fn test_register_asset(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    // no permission
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::register_asset {
        asset_token: MIR_TOKEN.to_string(),
        staking_token: MIR_LP.to_string(),
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

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
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
        &env,
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
                msg: to_binary(&ExecuteMsg::unbond {
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
    let msg = ExecuteMsg::withdraw {
        asset_token: None,
        spec_amount: None,
        farm_amount: None,
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
        &env,
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
        &env,
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

fn test_harvest_unauthorized(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    // harvest err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::harvest_all {};
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_err());
}

fn test_harvest_all(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, TerraQuery>) {
    // harvest all
    let env = mock_env();
    let info = mock_info(TEST_CONTROLLER, &[]);
    let msg = ExecuteMsg::harvest_all {};
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_STAKING.to_string(),
                funds: vec![],
                msg: to_binary(&MirrorStakingExecuteMsg::Withdraw { asset_token: None }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: MIR_PAIR_INFO.to_string(),
                    amount: Uint128::from(7100u128),
                    msg: to_binary(&TerraswapCw20HookMsg::Swap {
                        max_spread: None,
                        belief_price: None,
                        to: None,
                        deadline: None,
                    }).unwrap()
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ANC_MARKET.to_string(),
                msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {}).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(1367u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                msg: to_binary(&GovExecuteMsg::mint {}).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::send_fee {}).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MIR_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: MIR_GOV.to_string(),
                    amount: Uint128::from(13300u128),
                    msg: to_binary(&MirrorGovCw20HookMsg::StakeVotingTokens {}).unwrap(),
                })
                .unwrap(),
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
                    recipient: TEST_PLATFORM.to_string(),
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
