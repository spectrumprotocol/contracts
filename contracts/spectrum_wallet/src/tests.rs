use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, OwnedDeps, Uint128};
use cw20::Cw20ReceiveMsg;
use spectrum_protocol::gov::VoteOption;
use spectrum_protocol::wallet::{BalanceResponse, ConfigInfo, Cw20HookMsg, ExecuteMsg, QueryMsg};

const TEST_CREATOR: &str = "creator";
const SPEC_GOV: &str = "spec_gov";
const SPEC_TOKEN: &str = "token";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);

    test_setup(&mut deps);
    test_deposit(&mut deps);
    test_stake(&mut deps);
    test_withdraw(&mut deps);
    test_reward(&mut deps);
}

fn test_setup(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // init
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);

    let config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
    };
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), config);
    assert!(res.is_ok());

    // add share
    let msg = ExecuteMsg::upsert_share {
        address: USER1.to_string(),
        weight: 2u32,
        lock_start: Some(env.block.height + 10u64),
        lock_end: Some(env.block.height + 20u64),
        lock_amount: Some(Uint128::from(100u64)),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // add share2
    let msg = ExecuteMsg::upsert_share {
        address: USER2.to_string(),
        weight: 1u32,
        lock_start: Some(env.block.height + 10u64),
        lock_end: Some(env.block.height + 20u64),
        lock_amount: Some(Uint128::from(50u64)),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // non-owner cannot alter
    let info = mock_info(USER3, &[]);
    let msg = ExecuteMsg::upsert_share {
        address: USER3.to_string(),
        weight: 1u32,
        lock_start: Some(10u64),
        lock_end: Some(20u64),
        lock_amount: Some(Uint128::from(50u64)),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    let msg = ExecuteMsg::update_config {
        owner: Some(USER3.to_string()),
    };
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_err());
}

fn test_deposit(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // cannot deposit non-SPEC
    let env = mock_env();
    let info = mock_info("MIR", &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(100u64),
        msg: to_binary(&Cw20HookMsg::deposit {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // not allow non-user in wallet
    let info = mock_info(SPEC_TOKEN, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER3.to_string(),
        amount: Uint128::from(100u64),
        msg: to_binary(&Cw20HookMsg::deposit {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // deposit for user1
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(100u64),
        msg: to_binary(&Cw20HookMsg::deposit {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // deposit for user2
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(50u64),
        msg: to_binary(&Cw20HookMsg::deposit {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_ok());
}

fn test_stake(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // cannot stake more than you have
    let env = mock_env();
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::stake {
        amount: Uint128::from(150u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // stake
    let msg = ExecuteMsg::stake {
        amount: Uint128::from(100u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[(
        &SPEC_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100u128))],
    )]);

    // vote
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1u64,
        amount: Uint128::from(100u64),
        vote: VoteOption::yes,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    // other user cannot vote
    let info = mock_info(USER3, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // unstake more than you have
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unstake {
        amount: Uint128::from(150u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    let msg = ExecuteMsg::unstake {
        amount: Uint128::from(20u64),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[(
        &SPEC_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(80u128))],
    )]);

    let msg = QueryMsg::balance {
        address: USER1.to_string(),
    };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        BalanceResponse {
            share: Uint128::from(80u128),
            staked_amount: Uint128::from(80u128),
            unstaked_amount: Uint128::from(20u128),
            locked_amount: Uint128::from(100u128),
        }
    );
}

fn test_withdraw(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // cannot withdraw because of lock
    let mut env = mock_env();
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::withdraw {
        amount: Some(Uint128::from(20u128)),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    // can withdraw
    env.block.height += 12u64;
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_ok());
}

fn test_reward(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let mut env = mock_env();
    deps.querier.with_balance_percent(110u128);
    deps.querier.with_token_balances(&[(
        &SPEC_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(121u128))],
    )]);

    env.block.height += 12u64;
    let msg = QueryMsg::balance {
        address: USER1.to_string(),
    };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        BalanceResponse {
            share: Uint128::from(100u128),
            staked_amount: Uint128::from(110u128),
            unstaked_amount: Uint128::from(0u128),
            locked_amount: Uint128::from(80u128),
        }
    );
}
