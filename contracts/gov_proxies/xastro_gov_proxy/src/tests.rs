use crate::contract::{ConfigInfo, execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{State};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, OwnedDeps, Uint128, CosmosMsg, WasmMsg};
use cw20::{Cw20ReceiveMsg, Cw20ExecuteMsg};
use spectrum_protocol::gov_proxy::{Cw20HookMsg, ExecuteMsg, QueryMsg, StakerResponse};
use astroport::staking::Cw20HookMsg as AstroportCw20HookMsg;

const TEST_CREATOR: &str = "creator";
const FARM_CONTRACT: &str = "farm_contract";
const FARM_CONTRACT_2: &str = "farm_contract_2";
const FARM_TOKEN: &str = "farm_token";
const FARM_GOV: &str = "farm_gov";
const XASTRO_TOKEN: &str = "xastro_token";

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
        xastro_token: XASTRO_TOKEN.to_string(),
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
    deps.querier.with_token_balances(&[
        (
            &XASTRO_TOKEN.to_string(),
            &[(&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128))],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(20_000_000u128))],
        )
    ]);

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
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: FARM_GOV.to_string(),
                    amount: Uint128::from(10000u128),
                    msg: to_binary(&AstroportCw20HookMsg::Enter {
                    }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

    // verify state
    deps.querier.with_token_balances(&[
        (
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(5000u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(20_000_000u128))],
        )
    ]);
    let msg = QueryMsg::Staker { address: FARM_CONTRACT.to_string() };
    let res: StakerResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(10000u128),
        }
    );

    // stake more and gov stake grows by 1000
    deps.querier.with_token_balances(&[
        (
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(5000u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(22_000_000u128))],
        )
    ]);
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
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: FARM_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: FARM_GOV.to_string(),
                    amount: Uint128::from(5000u128),
                    msg: to_binary(&AstroportCw20HookMsg::Enter {
                    }).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(7272u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(22_000_000u128))],
        )
    ]);

    let info = mock_info(FARM_TOKEN, &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: FARM_CONTRACT_2.to_string(),
        amount: Uint128::from(4000u128),
        msg: to_binary(&Cw20HookMsg::Stake {}).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    deps.querier.with_token_balances(&[
        (
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(9090u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(22_000_000u128))],
        )
    ]);

    let msg = QueryMsg::Staker { address: FARM_CONTRACT.to_string() };
    let res: StakerResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(15998u128),
        }
    );


}

fn test_unstake(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();

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
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(8181u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(22_000_000u128))],
        )
    ]);
    let msg = QueryMsg::Staker { address: FARM_CONTRACT.to_string() };
    let res: StakerResponse = from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(13998u128),
        }
    );

    // destination gov stake grows 10%
    deps.querier.with_token_balances(&[
        (
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(8181u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(24_200_000u128))],
        )
    ]);

    let msg = QueryMsg::Staker { address: FARM_CONTRACT.to_string() };
    let res: StakerResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(15398u128),
        }
    );

    // unstake more than deposited
    let info = mock_info(FARM_CONTRACT, &[]);
    let msg = ExecuteMsg::Unstake { amount: Some(Uint128::from(13400u128)) };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    // destination gov will have 15400 - 13400 + 5% (gain)
    deps.querier.with_token_balances(&[
        (
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(2644u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(25_410_000u128))],
        )
    ]);
    let msg = QueryMsg::Staker { address: FARM_CONTRACT.to_string() };
    let res: StakerResponse = from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(2097u128),
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
            &XASTRO_TOKEN.to_string(),
            &[
                (&XASTRO_TOKEN.to_string(), &Uint128::from(10_000_000u128)),
                (&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1818u128)),
            ],
        ),
        (
            &FARM_TOKEN.to_string(),
            &[(&FARM_GOV.to_string(), &Uint128::from(25_410_000u128))],
        )
    ]);

    let msg = QueryMsg::Staker { address: FARM_CONTRACT.to_string() };
    let res: StakerResponse = from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(0u128),
        }
    );

    let msg = QueryMsg::Staker { address: FARM_CONTRACT_2.to_string() };
    let res: StakerResponse = from_binary(&query(deps.as_ref(), env.clone(), msg.clone()).unwrap()).unwrap();
    assert_eq!(
        res,
        StakerResponse {
            balance: Uint128::from(4619u128),
        }
    );

}
