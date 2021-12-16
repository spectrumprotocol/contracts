
use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{read_config, read_state, state_store};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, CosmosMsg, Decimal, OwnedDeps, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use nexus_token::governance::{AnyoneMsg, ExecuteMsg as NexusGovExecuteMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::ExecuteMsg as GovExecuteMsg;
use std::fmt::Debug;
use spectrum_protocol::gov_proxy::{ConfigInfo, Cw20HookMsg, ExecuteMsg, QueryMsg, StakerInfoGovResponse, StateInfo};

const SPEC_GOV: &str = "SPEC_GOV";
const TEST_CREATOR: &str = "creator";
const PSI_POOL: &str = "psi_pool";
const FARM_CONTRACT: &str = "farm_contract";
const FARM_CONTRACT_NON_WHITELISTED: &str = "farm_contract_2";
const FARM_TOKEN: &str = "farm_token";
const FARM_GOV: &str = "farm_gov";

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);

    let _ = test_config(&mut deps);
    test_stake(&mut deps);
    test_unstake(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state

    // farm contract is not deployed yet, because farm contract require gov_proxy address in instantiation first.
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        farm_contract: None,
        farm_token: FARM_TOKEN.to_string(),
        farm_gov: FARM_GOV.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
    };

    // success init
    let res = instantiate(deps.as_mut(), env.clone(), info, config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::Config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, config);

    // read state
    let msg = QueryMsg::State {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            total_deposit: Uint128::zero(),
            total_withdraw: Uint128::zero(),
            token_gain: Uint128::zero(),
        }
    );

    // farm contract is deployed

    // validate owner, add farm contract, update owner
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        owner: Some(SPEC_GOV.to_string()),
        farm_contract: Some(FARM_CONTRACT.to_string())
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(TEST_CREATOR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::Config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    config.owner = SPEC_GOV.to_string();
    config.farm_contract = Some(FARM_CONTRACT.to_string());
    assert_eq!(res, config);

    // farm_contract cannot be set again once already set
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        owner: None,
        farm_contract: Some(FARM_CONTRACT.to_string())
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    config
}

fn test_stake(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();

    // only farm_contract and farm_token can stake
    let info = mock_info(FARM_TOKEN, &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: FARM_CONTRACT_NON_WHITELISTED.to_string(),
        amount: Uint128::from(10000u128),
        msg: to_binary(&Cw20HookMsg::Stake {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());


    let info = mock_info(FARM_TOKEN, &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: FARM_CONTRACT.to_string(),
        amount: Uint128::from(10000u128),
        msg: to_binary(&Cw20HookMsg::Stake {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    // verify state
    deps.querier.with_token_balances(&[
        (
            &FARM_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
        ),
    ]);
    let msg = QueryMsg::State {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            total_deposit: Uint128::from(10000u128),
            total_withdraw: Uint128::zero(),
            token_gain: Uint128::zero(),
        }
    );

    // stake more and gov stake grows by 1000
    deps.querier.with_token_balances(&[
        (
            &FARM_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(16000u128))],
        ),
    ]);
    let info = mock_info(FARM_TOKEN, &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: FARM_CONTRACT.to_string(),
        amount: Uint128::from(5000u128),
        msg: to_binary(&Cw20HookMsg::Stake {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    let msg = QueryMsg::State {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            total_deposit: Uint128::from(15000u128),
            total_withdraw: Uint128::zero(),
            token_gain: Uint128::from(1000u128),
        }
    );

    let msg = QueryMsg::StakerInfo {};
    let res: StakerInfoGovResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerInfoGovResponse {
            bond_amount: Uint128::from(16000u128),
        }
    );

}

fn test_unstake(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();

    // only farm_contract can unstake
    let info = mock_info(FARM_CONTRACT_NON_WHITELISTED, &[]);
    let msg = ExecuteMsg::Unstake { amount: Some(Uint128::from(1000u128)) };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // unstake more than available is not allowed
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake { amount: Some(Uint128::from(100000u128)) };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // unstake 2000
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake { amount: Some(Uint128::from(2000u128)) };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    // destination gov will have 16000 - 2000 = 14000
    deps.querier.with_token_balances(&[
        (
            &FARM_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(14000u128))],
        ),
    ]);
    let msg = QueryMsg::StakerInfo {};
    let res: StakerInfoGovResponse = from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerInfoGovResponse {
            bond_amount: Uint128::from(14000u128),
        }
    );

    let msg = QueryMsg::State {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            total_deposit: Uint128::from(15000u128),
            total_withdraw: Uint128::from(2000u128),
            token_gain: Uint128::from(1000u128),
        }
    );

    // destination gov stake grows 1500
    deps.querier.with_token_balances(&[
        (
            &FARM_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(15500u128))],
        ),
    ]);

    let msg = QueryMsg::State {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            total_deposit: Uint128::from(15000u128),
            total_withdraw: Uint128::from(2000u128),
            token_gain: Uint128::from(2500u128),
        }
    );

    // unstake more than deposited
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake { amount: Some(Uint128::from(13500u128)) };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    // destination gov will have 15500 - 13500 + 100 (gain)
    deps.querier.with_token_balances(&[
        (
            &FARM_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(2100u128))],
        ),
    ]);
    let msg = QueryMsg::StakerInfo {};
    let res: StakerInfoGovResponse = from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerInfoGovResponse {
            bond_amount: Uint128::from(2100u128),
        }
    );

    let msg = QueryMsg::State {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            total_deposit: Uint128::from(15000u128),
            total_withdraw: Uint128::from(15500u128),
            token_gain: Uint128::from(2600u128),
        }
    );

    // unstake all
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake { amount: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    // destination gov will have 0
    deps.querier.with_token_balances(&[
        (
            &FARM_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(0u128))],
        ),
    ]);
    let msg = QueryMsg::StakerInfo {};
    let res: StakerInfoGovResponse = from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerInfoGovResponse {
            bond_amount: Uint128::from(0u128),
        }
    );

    let msg = QueryMsg::State {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            total_deposit: Uint128::from(15000u128),
            total_withdraw: Uint128::from(17600u128),
            token_gain: Uint128::from(2600u128),
        }
    );
}
