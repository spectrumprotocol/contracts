use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    from_binary, to_binary, CosmosMsg, Decimal, Extern, HumanAddr, Uint128, WasmMsg,
};
use mirror_protocol::gov::HandleMsg as MirrorGovHandleMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::mirror_farm::{
    ConfigInfo, HandleMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use std::fmt::Debug;

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
    pub staker_addr: HumanAddr,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: HumanAddr,
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

    test_config(&mut deps);
    test_register_asset(&mut deps);
    test_vote_unauthorized(&mut deps);
    test_vote_zero(&mut deps);
    test_vote(&mut deps);
}

fn test_config(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: HumanAddr::from(TEST_CREATOR),
        spectrum_gov: HumanAddr::from(SPEC_GOV),
        spectrum_token: HumanAddr::from(SPEC_TOKEN),
        mirror_gov: HumanAddr::from(MIR_GOV),
        mirror_token: HumanAddr::from(MIR_TOKEN),
        mirror_staking: HumanAddr::from(MIR_STAKING),
        terraswap_factory: HumanAddr::from(TERRA_SWAP),
        platform: Option::None,
        controller: Some(HumanAddr::from(TEST_CONTROLLER)),
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
        asset_token: HumanAddr::from(MIR_TOKEN),
        staking_token: HumanAddr::from(MIR_LP),
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
                asset_token: HumanAddr::from(MIR_TOKEN),
                staking_token: HumanAddr::from(MIR_LP),
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
        asset_token: HumanAddr::from(SPY_TOKEN),
        staking_token: HumanAddr::from(SPY_LP),
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

fn test_vote_unauthorized(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // vote err
    let env = mock_env(TEST_CREATOR, &[]);
    let msg = HandleMsg::cast_vote_to_mirror {
        poll_id: 180,
        amount: Uint128::from(1_000_000u128),
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());
}

fn test_vote_zero(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // vote zero
    let env = mock_env(TEST_CONTROLLER, &[]);
    let msg = HandleMsg::cast_vote_to_mirror {
        poll_id: 180,
        amount: Uint128::zero(),
    };
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(MIR_GOV),
            msg: to_binary(&MirrorGovHandleMsg::CastVote {
                poll_id: 180,
                vote: mirror_protocol::gov::VoteOption::Abstain,
                amount: Uint128::zero()
            })
            .unwrap(),
            send: vec![],
        }),]
    );
}

fn test_vote(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // vote
    let env = mock_env(TEST_CONTROLLER, &[]);
    let msg = HandleMsg::cast_vote_to_mirror {
        poll_id: 180,
        amount: Uint128::from(1_000_000u128),
    };
    let res = handle(deps, env.clone(), msg.clone()).unwrap();

    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(MIR_GOV),
            msg: to_binary(&MirrorGovHandleMsg::CastVote {
                poll_id: 180,
                vote: mirror_protocol::gov::VoteOption::Abstain,
                amount: Uint128::from(1_000_000u128)
            })
            .unwrap(),
            send: vec![],
        }),]
    );
}
