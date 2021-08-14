use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR, mock_info};
use cosmwasm_std::{from_binary, to_binary, CosmosMsg, Decimal, Uint128, WasmMsg, OwnedDeps};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::gov::ExecuteMsg as GovExecuteMsg;
use spectrum_protocol::spec_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, PoolItem, PoolsResponse, QueryMsg, RewardInfoResponse,
    RewardInfoResponseItem, StateInfo,
};
use std::str::FromStr;

const GOV: &str = "gov";
const TOKEN: &str = "token";
const TEST_CREATOR: &str = "creator";
const USER1: &str = "user1";
const USER2: &str = "user2";
const LP: &str = "lp_token";

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_bond(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        spectrum_gov: GOV.to_string(),
        spectrum_token: TOKEN.to_string(),
        owner: TEST_CREATOR.to_string(),
        lock_start: 0u64,
        lock_end: 100u64,
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
            spec_share_index: Decimal::zero(),
            total_weight: 0u32,
        }
    );

    // alter config, validate owner
    let info = mock_info(GOV, &[]);
    let msg = ExecuteMsg::update_config {
        owner: Some(GOV.to_string()),
        lock_start: None,
        lock_end: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(TEST_CREATOR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    config.owner = GOV.to_string();
    assert_eq!(res, config);

    config
}

fn test_register_asset(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // no permission
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::register_asset {
        asset_token: TOKEN.to_string(),
        staking_token: LP.to_string(),
        weight: 1u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(GOV, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // query pool info
    let msg = QueryMsg::pools {};
    let res: PoolsResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PoolsResponse {
            pools: vec![PoolItem {
                asset_token: TOKEN.to_string(),
                staking_token: LP.to_string(),
                total_bond_amount: Uint128::zero(),
                state_spec_share_index: Decimal::zero(),
                spec_share_index: Decimal::zero(),
                weight: 1u32,
            }]
        }
    );

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 1u32);
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // bond err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::bond {
                staker_addr: None,
                asset_token: TOKEN.to_string(),
            })
            .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    deps.querier.with_token_balances(&[(
        &GOV.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::from(100u128),
        )],
    )]);

    // bond success
    let info = mock_info(LP, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[(
        &GOV.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::from(500u128),
        )],
    )]);

    // query balance
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
        height: 100u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: TOKEN.to_string(),
            spec_share: Uint128::from(500u128),
            pending_spec_reward: Uint128::from(500u128),
            bond_amount: Uint128::from(100u128),
            accum_spec_share: Uint128::from(500u128),
            locked_spec_reward: Uint128::zero(),
            locked_spec_share: Uint128::zero(),
            spec_share_index: Decimal::zero(),
        },]
    );

    // unbond
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unbond {
        asset_token: TOKEN.to_string(),
        amount: Uint128::from(20u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: LP.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: USER1.to_string(),
                amount: Uint128::from(20u128),
            })
            .unwrap(),
        })
    );

    // withdraw
    let msg = ExecuteMsg::withdraw { asset_token: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages.into_iter().map(|it| it.msg).collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: GOV.to_string(),
                funds: vec![],
                msg: to_binary(&GovExecuteMsg::withdraw {
                    amount: Some(Uint128::from(500u128)),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(500u128),
                })
                .unwrap(),
            }),
        ]
    );

    deps.querier.with_token_balances(&[(
        &GOV.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::from(450u128),
        )],
    )]);

    // bond user2
    let info = mock_info(LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(70u128),
        msg: to_binary(&Cw20HookMsg::bond {
                staker_addr: None,
                asset_token: TOKEN.to_string(),
            })
            .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[(
        &GOV.to_string(),
        &[(
            &MOCK_CONTRACT_ADDR.to_string(),
            &Uint128::from(600u128),
        )],
    )]);

    // query balance1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
        asset_token: None,
        height: 40u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: TOKEN.to_string(),
            spec_share: Uint128::from(530u128),
            pending_spec_reward: Uint128::from(530u128),
            bond_amount: Uint128::from(80u128),
            accum_spec_share: Uint128::from(1030u128),
            locked_spec_reward: Uint128::from(618u128),
            locked_spec_share: Uint128::from(618u128),
            spec_share_index: Decimal::from_str("5").unwrap(),
        },]
    );

    // query balance2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
        asset_token: None,
        height: 0u64,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: TOKEN.to_string(),
            spec_share: Uint128::from(70u128),
            pending_spec_reward: Uint128::from(70u128),
            bond_amount: Uint128::from(70u128),
            accum_spec_share: Uint128::from(70u128),
            locked_spec_reward: Uint128::from(70u128),
            locked_spec_share: Uint128::from(70u128),
            spec_share_index: Decimal::from_str("10.625").unwrap(),
        },]
    );
}
