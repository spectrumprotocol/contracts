use crate::bond::deposit_farm_share;
use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{pool_info_read, pool_info_store, read_config};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, CosmosMsg, Decimal, Extern, String, Uint128, WasmMsg,
};
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use pylon_token::gov::HandleMsg as PylonGovHandleMsg;
use pylon_token::staking::HandleMsg as PylonStakingHandleMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::HandleMsg as GovHandleMsg;
use spectrum_protocol::pylon_farm::{
    ConfigInfo, Cw20HookMsg, HandleMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use std::fmt::Debug;

const SPEC_GOV: &str = "spec_gov";
const SPEC_TOKEN: &str = "spec_token";
const MINE_GOV: &str = "mine_gov";
const MINE_TOKEN: &str = "mine_token";
const MINE_STAKING: &str = "mine_staking";
const TERRA_SWAP: &str = "terra_swap";
const TEST_CREATOR: &str = "creator";
const USER1: &str = "user1";
const USER2: &str = "user2";
const MINE_LP: &str = "mine_lp";
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

    // register again should fail
    let msg = HandleMsg::register_asset {
        asset_token: SPY_TOKEN.to_string(),
        staking_token: SPY_LP.to_string(),
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
