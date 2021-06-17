use cosmwasm_std::{
    from_binary, to_binary, Coin, CosmosMsg, Decimal, HandleResponse, HandleResult, HumanAddr,
    InitResponse, StdError, Uint128, WasmMsg,
};

use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use spectrum_protocol::mirror_farm::{
    ConfigInfo, Cw20HookMsg, HandleMsg, InitMsg, PoolInfoResponse, QueryMsg, RewardInfoResponse,
    RewardInfoResponseItem,
};

use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};

const DEFAULT_GAS_LIMIT: u64 = 500_000;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        owner: HumanAddr("owner0000".to_string()),
        terraswap_factory: HumanAddr("terraswap0000".to_string()),
        spectrum_token: HumanAddr("reward0000".to_string()),
        mirror_token: HumanAddr("mirror0001".to_string()),
        mirror_staking: HumanAddr("mirror0000".to_string()),
        base_denom: "uusd".to_string(),
        spectrum_gov: HumanAddr("specgov0000".to_string()),
        mirror_gov: HumanAddr("mirrorgob0000".to_string()),
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(&mut deps, QueryMsg::config {}).unwrap();
    let value: ConfigInfo = from_binary(&res).unwrap();
    assert_eq!("owner0000", value.owner.as_str());
    assert_eq!("reward0000", value.spectrum_token.as_str());
}

#[test]
fn test_bond_tokens() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        owner: HumanAddr("owner0000".to_string()),
        terraswap_factory: HumanAddr("terraswap0000".to_string()),
        spectrum_token: HumanAddr("reward0000".to_string()),
        mirror_token: HumanAddr("mirror0001".to_string()),
        mirror_staking: HumanAddr("mirror0000".to_string()),
        base_denom: "uusd".to_string(),
        spectrum_gov: HumanAddr("specgov0000".to_string()),
        mirror_gov: HumanAddr("mirrorgob0000".to_string()),
    };

    let env = mock_env("addr0000", &[]);
    let _res: InitResponse = init(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::register_asset {
        asset_token: HumanAddr::from("asset0000"),
        staking_token: HumanAddr::from("staking0000"),
    };

    let env = mock_env("owner0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg.clone()).unwrap();

    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from("addr0000"),
        amount: Uint128(100u128),
        msg: Some(
            to_binary(&Cw20HookMsg::bond {
                asset_token: HumanAddr::from("asset0000"),
                compound_rate: Decimal::percent(100),
            })
            .unwrap(),
        ),
    });
    let env = mock_env("staking0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg).unwrap();
    let res = query(
        &mut deps,
        QueryMsg::reward_info {
            staker_addr: HumanAddr::from("addr0000"),
            asset_token: Some(HumanAddr::from("asset0000")),
        },
    )
    .unwrap();
    let reward_info: RewardInfoResponse = from_binary(&res).unwrap();
    assert_eq!(
        reward_info,
        RewardInfoResponse {
            staker_addr: HumanAddr::from("addr0000"),
            reward_infos: vec![RewardInfoResponseItem {
                asset_token: HumanAddr::from("asset0000"),
                index: Decimal::zero(),
                mir_withdrawed: Uint128::zero(),
                pending_spec_reward: Uint128::zero(),
                auto_bond_share: Uint128(100u128),
                stake_bond_amount: Uint128::zero()
            }],
        }
    );

    let res = query(
        &mut deps,
        QueryMsg::pool_info {
            asset_token: HumanAddr::from("asset0000"),
        },
    )
    .unwrap();
    let pool_info: PoolInfoResponse = from_binary(&res).unwrap();
    assert_eq!(
        pool_info,
        PoolInfoResponse {
            asset_token: HumanAddr::from("asset0000"),
            staking_token: HumanAddr::from("staking0000"),
            total_bond_amount: Uint128(100u128),
            total_auto_bond_share: Uint128(100u128),
            total_stake_bond_share: Uint128::zero(),
            reward_index: Decimal::zero(),
            pending_reward: Uint128::zero(),
        }
    );

    // bond 100 more tokens from other account
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from("addr0001"),
        amount: Uint128(100u128),
        msg: Some(
            to_binary(&Cw20HookMsg::bond {
                asset_token: HumanAddr::from("asset0000"),
                compound_rate: Decimal::percent(90),
            })
            .unwrap(),
        ),
    });
    let env = mock_env("staking0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg).unwrap();

    let data = query(
        &mut deps,
        QueryMsg::pool_info {
            asset_token: HumanAddr::from("asset0000"),
        },
    )
    .unwrap();
    let pool_info: PoolInfoResponse = from_binary(&data).unwrap();
    assert_eq!(
        pool_info,
        PoolInfoResponse {
            asset_token: HumanAddr::from("asset0000"),
            staking_token: HumanAddr::from("staking0000"),
            total_bond_amount: Uint128(200u128),
            total_auto_bond_share: Uint128(190u128),
            total_stake_bond_share: Uint128(10u128),
            reward_index: Decimal::zero(),
            pending_reward: Uint128::zero(),
        }
    );

    // failed with unautorized
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from("addr0000"),
        amount: Uint128(100u128),
        msg: Some(
            to_binary(&Cw20HookMsg::bond {
                asset_token: HumanAddr::from("asset0000"),
                compound_rate: Decimal::percent(90),
            })
            .unwrap(),
        ),
    });

    let env = mock_env("staking0001", &[]);
    let res: HandleResult = handle(&mut deps, env, msg);
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("Must return unauthorized error"),
    }
}

#[test]
fn test_deposit_reward() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        owner: HumanAddr("owner0000".to_string()),
        terraswap_factory: HumanAddr("terraswap0000".to_string()),
        spectrum_token: HumanAddr("reward0000".to_string()),
        mirror_token: HumanAddr("mirror0001".to_string()),
        mirror_staking: HumanAddr("mirror0000".to_string()),
        base_denom: "uusd".to_string(),
        spectrum_gov: HumanAddr("specgov0000".to_string()),
        mirror_gov: HumanAddr("mirrorgob0000".to_string()),
    };

    let env = mock_env("addr0000", &[]);
    let _res: InitResponse = init(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::register_asset {
        asset_token: HumanAddr::from("asset0000"),
        staking_token: HumanAddr::from("staking0000"),
    };

    let env = mock_env("owner0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg.clone()).unwrap();

    // bond 100 tokens
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from("addr0000"),
        amount: Uint128(100u128),
        msg: Some(
            to_binary(&Cw20HookMsg::bond {
                asset_token: HumanAddr::from("asset0000"),
                compound_rate: Decimal::one(),
            })
            .unwrap(),
        ),
    });
    let env = mock_env("staking0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg).unwrap();

    // unauthoirzed
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from("owner0000"),
        amount: Uint128(1000u128),
        msg: Some(
            to_binary(&Cw20HookMsg::deposit_reward {
                asset_token: HumanAddr::from("asset0000"),
            })
            .unwrap(),
        ),
    });

    let env = mock_env("wrongtoken", &[]);
    let res: HandleResult = handle(&mut deps, env, msg.clone());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("Must return unauthorized error"),
    }

    // factory deposit 1000 reward tokens
    let env = mock_env("reward0000", &[]);
    let _res: HandleResponse = handle(&mut deps, env, msg).unwrap();

    let data = query(
        &mut deps,
        QueryMsg::pool_info {
            asset_token: HumanAddr::from("asset0000"),
        },
    )
    .unwrap();
    let pool_info: PoolInfoResponse = from_binary(&data).unwrap();
    assert_eq!(
        pool_info,
        PoolInfoResponse {
            asset_token: HumanAddr::from("asset0000"),
            staking_token: HumanAddr::from("staking0000"),
            total_bond_amount: Uint128(100u128),
            total_auto_bond_share: Uint128(100u128),
            total_stake_bond_share: Uint128::zero(),
            reward_index: Decimal::from_ratio(10u128, 1u128),
            pending_reward: Uint128::zero(),
        }
    );
}
