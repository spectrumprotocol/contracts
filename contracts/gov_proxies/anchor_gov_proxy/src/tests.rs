use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::State;
use anchor_token::gov::{Cw20HookMsg as AnchorCw20HookMsg, ExecuteMsg as AnchorExecuteMsg};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, CosmosMsg, OwnedDeps, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::gov_proxy::{ConfigInfo, Cw20HookMsg, ExecuteMsg, QueryMsg, StakerResponse};

const TEST_CREATOR: &str = "creator";
const FARM_CONTRACT: &str = "farm_contract";
const FARM_CONTRACT_2: &str = "farm_contract_2";
const FARM_TOKEN: &str = "farm_token";
const FARM_GOV: &str = "farm_gov";

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);

    let _ = test_config(&mut deps);
    test_stake(&mut deps);
    test_unstake(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state

    // farm contract is not deployed yet, because farm contract require gov_proxy address in instantiation first.
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let config = ConfigInfo {
        farm_token: FARM_TOKEN.to_string(),
        farm_gov: FARM_GOV.to_string(),
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
    let res: State = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        State {
            total_share: Uint128::zero(),
        }
    );

    config
}

fn test_stake(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();

    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: FARM_CONTRACT.to_string(),
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
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: FARM_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: FARM_GOV.to_string(),
                msg: to_binary(&AnchorCw20HookMsg::StakeVotingTokens {}).unwrap(),
                amount: Uint128::from(10000u128),
            })
            .unwrap(),
        }),]
    );

    // verify state
    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
    )]);
    let msg = QueryMsg::Staker {
        address: FARM_CONTRACT.to_string(),
    };
    let res: StakerResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(10000u128),
        }
    );

    // stake more and gov stake grows by 1000
    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(11000u128))],
    )]);
    let info = mock_info(FARM_TOKEN, &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: FARM_CONTRACT.to_string(),
        amount: Uint128::from(5000u128),
        msg: to_binary(&Cw20HookMsg::Stake {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: FARM_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: FARM_GOV.to_string(),
                msg: to_binary(&AnchorCw20HookMsg::StakeVotingTokens {}).unwrap(),
                amount: Uint128::from(5000u128),
            })
            .unwrap(),
        }),]
    );

    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(16000u128))],
    )]);

    let info = mock_info(FARM_TOKEN, &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: FARM_CONTRACT_2.to_string(),
        amount: Uint128::from(4000u128),
        msg: to_binary(&Cw20HookMsg::Stake {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: FARM_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: FARM_GOV.to_string(),
                msg: to_binary(&AnchorCw20HookMsg::StakeVotingTokens {}).unwrap(),
                amount: Uint128::from(4000u128),
            })
            .unwrap(),
        }),]
    );

    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(20000u128))],
    )]);

    let msg = QueryMsg::Staker {
        address: FARM_CONTRACT.to_string(),
    };
    let res: StakerResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(16000u128),
        }
    );
}

fn test_unstake(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();

    // unstake more than available is not allowed
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake {
        amount: Some(Uint128::from(100000u128)),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // unstake 2000
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake {
        amount: Some(Uint128::from(2000u128)),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_GOV.to_string(),
                msg: to_binary(&AnchorExecuteMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(2000u128)),
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: FARM_CONTRACT.to_string(),
                    amount: Uint128::from(2000u128),
                })
                .unwrap(),
                funds: vec![],
            })
        ]
    );

    // destination gov will have 20000 - 2000 = 18000
    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(18000u128))],
    )]);
    let msg = QueryMsg::Staker {
        address: FARM_CONTRACT.to_string(),
    };
    let res: StakerResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(14000u128),
        }
    );

    // destination gov stake grows 10%, 18000 + 1800 = 19800
    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(19800u128))],
    )]);

    let msg = QueryMsg::Staker {
        address: FARM_CONTRACT.to_string(),
    };
    let res: StakerResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(15400u128),
        }
    );

    // unstake 13400
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake {
        amount: Some(Uint128::from(13400u128)),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_GOV.to_string(),
                msg: to_binary(&AnchorExecuteMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(13400u128)),
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: FARM_CONTRACT.to_string(),
                    amount: Uint128::from(13400u128),
                })
                .unwrap(),
                funds: vec![],
            })
        ]
    );

    // destination gov will have 19800 - 13400 + 320 (5% gain)
    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(6720u128))],
    )]);
    let msg = QueryMsg::Staker {
        address: FARM_CONTRACT.to_string(),
    };
    let res: StakerResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(2099u128),
        }
    );

    // unstake all
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake { amount: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_GOV.to_string(),
                msg: to_binary(&AnchorExecuteMsg::WithdrawVotingTokens {
                    amount: Some(Uint128::from(2099u128)),
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: FARM_CONTRACT.to_string(),
                    amount: Uint128::from(2099u128),
                })
                .unwrap(),
                funds: vec![],
            })
        ]
    );

    // destination gov will have 0
    deps.querier.with_token_balances(&[(
        &FARM_GOV.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(4620u128))],
    )]);

    let msg = QueryMsg::Staker {
        address: FARM_CONTRACT.to_string(),
    };
    let res: StakerResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(0u128),
        }
    );

    let msg = QueryMsg::Staker {
        address: FARM_CONTRACT_2.to_string(),
    };
    let res: StakerResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(4620u128),
        }
    );
}
