use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::{Extern, HumanAddr, Uint128, to_binary, from_binary};
use cosmwasm_std::testing::{MockStorage, MockApi, mock_env, MOCK_CONTRACT_ADDR};
use spectrum_protocol::wallet::{ConfigInfo, HandleMsg, Cw20HookMsg, BalanceResponse, QueryMsg};
use crate::contract::{init, handle, query};
use cw20::{Cw20ReceiveMsg};
use spectrum_protocol::gov::VoteOption;

const TEST_CREATOR: &str = "creator";
const SPEC_GOV: &str = "spec_gov";
const SPEC_TOKEN: &str = "token";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";

#[test]
fn test() {
    let mut deps = mock_dependencies(20, &[]);
    deps.querier.with_balance_percent(100);

    test_setup(&mut deps);
    test_deposit(&mut deps);
    test_stake(&mut deps);
    test_withdraw(&mut deps);
    test_reward(&mut deps);
}

fn test_setup(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // init
    let env = mock_env(TEST_CREATOR, &[]);
    let config = ConfigInfo {
        owner: HumanAddr::from(TEST_CREATOR),
        spectrum_gov: HumanAddr::from(SPEC_GOV),
        spectrum_token: HumanAddr::from(SPEC_TOKEN),
    };
    let res = init(deps, env.clone(), config);
    assert!(res.is_ok());

    // add share
    let msg = HandleMsg::upsert_share {
        address: HumanAddr::from(USER1),
        weight: 2u32,
        lock_start: Some(env.block.height + 10u64),
        lock_end: Some(env.block.height + 20u64),
        lock_amount: Some(Uint128::from(100u64)),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // add share2
    let msg = HandleMsg::upsert_share {
        address: HumanAddr::from(USER2),
        weight: 1u32,
        lock_start: Some(env.block.height + 10u64),
        lock_end: Some(env.block.height + 20u64),
        lock_amount: Some(Uint128::from(50u64)),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // non-owner cannot alter
    let env = mock_env(USER3, &[]);
    let msg = HandleMsg::upsert_share {
        address: HumanAddr::from(USER3),
        weight: 1u32,
        lock_start: Some(10u64),
        lock_end: Some(20u64),
        lock_amount: Some(Uint128::from(50u64)),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    let msg = HandleMsg::update_config {
        owner: Some(HumanAddr::from(USER3))
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());
}

fn test_deposit(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {

    // cannot deposit non-SPEC
    let env = mock_env("MIR", &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(USER1),
        amount: Uint128::from(100u64),
        msg: Some(to_binary(&Cw20HookMsg::deposit { }).unwrap()),
    });
    let res = handle(deps, env, msg);
    assert!(res.is_err());

    // not allow non-user in wallet
    let env = mock_env(SPEC_TOKEN, &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(USER3),
        amount: Uint128::from(100u64),
        msg: Some(to_binary(&Cw20HookMsg::deposit { }).unwrap()),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // deposit for user1
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(USER1),
        amount: Uint128::from(100u64),
        msg: Some(to_binary(&Cw20HookMsg::deposit { }).unwrap()),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // deposit for user2
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(USER2),
        amount: Uint128::from(50u64),
        msg: Some(to_binary(&Cw20HookMsg::deposit { }).unwrap()),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
}

fn test_stake(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // cannot stake more than you have
    let env = mock_env(USER1, &[]);
    let msg = HandleMsg::stake {
        amount: Uint128::from(150u64),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // stake
    let msg = HandleMsg::stake {
        amount: Uint128::from(100u64),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[
        (&HumanAddr::from(SPEC_GOV), &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128::from(100u128))])
    ]);

    // vote
    let msg = HandleMsg::poll_votes {
        poll_id: 1u64,
        amount: Uint128::from(100u64),
        vote: VoteOption::yes,
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_ok());

    // other user cannot vote
    let env = mock_env(USER3, &[]);
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // unstake more than you have
    let env = mock_env(USER1, &[]);
    let msg = HandleMsg::unstake {
        amount: Uint128::from(150u64),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    let msg = HandleMsg::unstake {
        amount: Uint128::from(20u64),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[
        (&HumanAddr::from(SPEC_GOV), &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128::from(80u128))])
    ]);

    let msg = QueryMsg::balance {
        address: HumanAddr::from(USER1),
        height: 0u64,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res, BalanceResponse {
        share: Uint128::from(80u128),
        staked_amount: Uint128::from(80u128),
        unstaked_amount: Uint128::from(20u128),
        locked_amount: Uint128::from(100u128),
    });
}

fn test_withdraw(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    // cannot withdraw because of lock
    let mut env = mock_env(USER1, &[]);
    let msg = HandleMsg::withdraw {
        amount: Some(Uint128::from(20u128))
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // can withdraw
    env.block.height = env.block.height + 12u64;
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_ok());
}

fn test_reward(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env(USER1, &[]);
    deps.querier.with_balance_percent(110u128);
    deps.querier.with_token_balances(&[
        (&HumanAddr::from(SPEC_GOV), &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128::from(121u128))])
    ]);
    let msg = QueryMsg::balance {
        address: HumanAddr::from(USER1),
        height: env.block.height + 12u64,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res, BalanceResponse {
        share: Uint128::from(100u128),
        staked_amount: Uint128::from(110u128),
        unstaked_amount: Uint128::from(0u128),
        locked_amount: Uint128::from(80u128),
    });
}
