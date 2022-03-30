use cosmwasm_std::{Addr, Coin, CosmosMsg, Decimal, from_binary, OwnedDeps, Response, StdResult, to_binary, Uint128, WasmMsg};
use cosmwasm_std::testing::{MOCK_CONTRACT_ADDR, mock_env, mock_info, MockApi, MockStorage};
use cw20::Cw20ExecuteMsg;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{ExecuteMsg as PairExecuteMsg};
use crate::contract::{execute, instantiate, query};
use crate::hub::{HubCw20HookMsg, HubState, Parameters, UnbondHistoryResponse};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::model::{ConfigInfo, ExecuteMsg, QueryMsg, RewardInfoResponse, RewardInfoResponseItem, SimulateCollectResponse, SwapOperation};
use crate::stader::{StaderConfig, StaderState};
use crate::state::{Burn, Hub, HubType, StakeCredit, State, Unbonding};

const CREATOR: &str = "creator";
const SPEC_TOKEN: &str = "spec_token";
const SPEC_GOV: &str = "spec_gov";
const PLATFORM: &str = "platform";
const CONTROLLER: &str = "controller";
const ANCHOR_MARKET: &str = "anchor_market";
const AUST_TOKEN: &str = "aust_token";
const UST_PAIR: &str = "ust_pair";
const ORACLE: &str ="oracle";
const BLUNA: &str = "bluna";
const STLUNA: &str = "stluna";
const CLUNA: &str = "cluna";
const LUNAX: &str = "lunax";
const ANCHOR_HUB: &str = "anchor_hub";
const PRISM_HUB: &str = "prism_hub";
const STADER: &str = "stader";
const BLUNA_PAIR: &str = "bluna_pair";
const STLUNA_PAIR: &str = "stluna_pair";
const LUNAX_PAIR: &str = "lunax_pair";
const PRISM_PAIR: &str = "prism_pair";
const CLUNA_PAIR: &str = "cluna_pair";
const SPEC_PAIR: &str = "spec_pair";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";

#[test]
fn test() {
    let mut deps = create_deps();
    test_config(&mut deps);
    test_bond(&mut deps);
    test_burn(&mut deps);
}

fn create_deps() -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let hub_state = HubState {
        exchange_rate: Decimal::percent(110),
        stluna_exchange_rate: Decimal::percent(120),
        bluna_exchange_rate: Decimal::percent(99),
    };
    let hub_parameters = Parameters {
        unbonding_period: 86400u64,
        er_threshold: Decimal::one(),
        peg_recovery_fee: Decimal::percent(5),
    };
    let stader_config = StaderConfig {
        protocol_withdraw_fee: Decimal::percent(5),
    };
    let stader_state = StaderState {
        exchange_rate: Decimal::percent(120),
        current_undelegation_batch_id: 1,
    };
    mock_dependencies(
        hub_state,
        hub_parameters,
        stader_config,
        stader_state,
    )
}

fn assert_error(res: StdResult<Response>, expected: &str) {
    match res {
        Err(err) => assert_eq!(expected, format!("{}", err)),
        _ => panic!("Expected exception"),
    }
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(CREATOR, &[]);

    let mut config = ConfigInfo {
        owner: CREATOR.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        platform: PLATFORM.to_string(),
        controller: CONTROLLER.to_string(),
        community_fee: Decimal::percent(120),
        platform_fee: Decimal::percent(1),
        controller_fee: Decimal::percent(1),
        deposit_fee: Decimal::permille(1),
        anchor_market: ANCHOR_MARKET.to_string(),
        aust_token: AUST_TOKEN.to_string(),
        max_unbond_count: 3u32,
        burn_period: 86400u64,
        ust_pair_contract: UST_PAIR.to_string(),
        oracle: ORACLE.to_string(),
        credits: vec![
            StakeCredit { days: 30u64, credit: Decimal::percent(1000) },
        ],
    };

    // validation
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), config.clone());
    assert_error(res, "Generic error: community_fee must be 0 to 1");

    // success
    config.community_fee = Decimal::percent(6);
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), config.clone());
    assert!(res.is_ok());

    // validate update
    let msg = ExecuteMsg::update_config {
        owner: None,
        controller: None,
        community_fee: None,
        platform_fee: Some(Decimal::percent(120)),
        controller_fee: None,
        deposit_fee: None,
        max_unbond_count: None,
        burn_period: None,
        credits: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_error(res, "Generic error: platform_fee must be 0 to 1");

    // update owner
    let msg = ExecuteMsg::update_config {
        owner: Some(SPEC_GOV.to_string()),
        controller: None,
        community_fee: None,
        platform_fee: None,
        controller_fee: None,
        deposit_fee: None,
        max_unbond_count: None,
        burn_period: None,
        credits: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());
    config.owner = SPEC_GOV.to_string();

    // allow only owner
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: unauthorized");

    // cannot update if gov set
    let info = mock_info(SPEC_GOV, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: cannot update owner");

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, config);

    // register hubs check owner
    let info = mock_info(CREATOR, &[]);
    let msg = ExecuteMsg::register_hub {
        token: BLUNA.to_string(),
        hub_type: HubType::bluna,
        hub_address: ANCHOR_HUB.to_string(),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: unauthorized");

    // register hubs, success
    let info = mock_info(SPEC_GOV, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    // stluna
    let msg = ExecuteMsg::register_hub {
        token: STLUNA.to_string(),
        hub_type: HubType::stluna,
        hub_address: ANCHOR_HUB.to_string(),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    // prism
    let msg = ExecuteMsg::register_hub {
        token: CLUNA.to_string(),
        hub_type: HubType::cluna,
        hub_address: PRISM_HUB.to_string(),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    // stader
    let msg = ExecuteMsg::register_hub {
        token: LUNAX.to_string(),
        hub_type: HubType::lunax,
        hub_address: STADER.to_string(),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    // query hubs
    let msg = QueryMsg::hubs {};
    let res: Vec<Hub> = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, vec![
        Hub {
            token: Addr::unchecked(LUNAX),
            hub_type: HubType::lunax,
            hub_address: Addr::unchecked(STADER),
        },
        Hub {
            token: Addr::unchecked(STLUNA),
            hub_type: HubType::stluna,
            hub_address: Addr::unchecked(ANCHOR_HUB),
        },
        Hub {
            token: Addr::unchecked(BLUNA),
            hub_type: HubType::bluna,
            hub_address: Addr::unchecked(ANCHOR_HUB),
        },
        Hub {
            token: Addr::unchecked(CLUNA),
            hub_type: HubType::cluna,
            hub_address: Addr::unchecked(PRISM_HUB),
        },
    ]);

}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();

    // send no coin
    let msg = ExecuteMsg::bond {
        staker_addr: None,
    };
    let info = mock_info(USER1, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: fund mismatch");

    // send uusd
    let info = mock_info(USER1, &[
        Coin { denom: "uusd".to_string(), amount: Uint128::from(1000000u128) },
    ]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: fund mismatch");

    // no credit
    let info = mock_info(USER1, &[
        Coin { denom: "uluna".to_string(), amount: Uint128::from(1000000u128) },
    ]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: Not enough credit, please stake at Spectrum gov to get credit");

    // deposit
    deps.querier.set_balance(SPEC_GOV.to_string(), USER1.to_string(), Uint128::from(100000u128));
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());
    deps.querier.set_balance("uluna".to_string(), MOCK_CONTRACT_ADDR.to_string(), Uint128::from(1000000u128));

    // check reward info
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, RewardInfoResponse {
        staker_addr: USER1.to_string(),
        reward_infos: vec![
            RewardInfoResponseItem {
                asset_token: "uluna".to_string(),
                bond_share: Uint128::from(999000u128),
                bond_amount: Uint128::from(999000u128),
                unbonding_amount: Uint128::zero(),
                spec_share_index: Decimal::zero(),
                spec_share: Uint128::zero(),
                pending_spec_reward: Uint128::zero(),
                deposit_amount: Uint128::from(999000u128),
                deposit_time: env.block.time.seconds()
            }
        ]
    });

    // check state
    let msg = QueryMsg::state {};
    let res: State = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, State {
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_bond_amount: Uint128::from(999000u128),
        total_bond_share: Uint128::from(999000u128),
        unbonding_amount: Uint128::zero(),
        claimable_amount: Uint128::zero(),
        unbond_counter: 0,
        unbonded_index: Uint128::zero(),
        unbonding_index: Uint128::zero(),
        deposit_fee: Uint128::from(1000u128),
        perf_fee: Uint128::zero(),
        deposit_earning: Uint128::zero(),
        perf_earning: Uint128::zero(),
        burn_counter: 0
    });

    // check credit again
    let msg = ExecuteMsg::bond {
        staker_addr: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: Not enough credit, please stake at Spectrum gov to get credit");

    // user2 unbond
    let info = mock_info(USER2, &[]);
    let msg = ExecuteMsg::unbond {
        amount: Uint128::from(33000u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: not found");

    // user1 unbond >3 times
    let info = mock_info(USER1, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: max unbond count reach");

    let msg = QueryMsg::unbond {
        staker_addr: USER1.to_string(),
    };
    let res: Vec<Unbonding> = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, vec![
        Unbonding {
            id: 1u64,
            time: env.block.time.seconds(),
            amount: Uint128::from(33000u128),
            unbonding_index: Uint128::from(33000u128),
        },
        Unbonding {
            id: 2u64,
            time: env.block.time.seconds(),
            amount: Uint128::from(33000u128),
            unbonding_index: Uint128::from(66000u128),
        },
        Unbonding {
            id: 3u64,
            time: env.block.time.seconds(),
            amount: Uint128::from(33000u128),
            unbonding_index: Uint128::from(99000u128),
        },
    ]);

    // check reward info again
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, RewardInfoResponse {
        staker_addr: USER1.to_string(),
        reward_infos: vec![
            RewardInfoResponseItem {
                asset_token: "uluna".to_string(),
                bond_share: Uint128::from(900000u128),
                bond_amount: Uint128::from(900000u128),
                unbonding_amount: Uint128::from(99000u128),
                spec_share_index: Decimal::zero(),
                spec_share: Uint128::zero(),
                pending_spec_reward: Uint128::zero(),
                deposit_amount: Uint128::from(900000u128),
                deposit_time: env.block.time.seconds()
            }
        ]
    });

    // check state again
    let msg = QueryMsg::state {};
    let res: State = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, State {
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_bond_amount: Uint128::from(900000u128),
        total_bond_share: Uint128::from(900000u128),
        unbonding_amount: Uint128::from(99000u128),
        claimable_amount: Uint128::zero(),
        unbond_counter: 3,
        unbonded_index: Uint128::zero(),
        unbonding_index: Uint128::from(99000u128),
        deposit_fee: Uint128::from(1000u128),
        perf_fee: Uint128::zero(),
        deposit_earning: Uint128::zero(),
        perf_earning: Uint128::zero(),
        burn_counter: 0,
    });
}

fn test_burn(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let mut env = mock_env();
    let info = mock_info(USER1, &[]);

    // no permission
    let msg = ExecuteMsg::burn {
        amount: Uint128::from(500000u128),
        min_profit: None,
        swap_operations: vec![]
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: unauthorized");

    // require swap
    let info = mock_info(CONTROLLER, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: require swap");

    // hub not found
    let msg = ExecuteMsg::burn {
        amount: Uint128::from(500000u128),
        min_profit: None,
        swap_operations: vec![
            SwapOperation {
                pair_address: SPEC_PAIR.to_string(),
                to_asset_info: AssetInfo::Token {
                    contract_addr: SPEC_TOKEN.to_string(),
                }
            }
        ]
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: hub not found");

    // loss
    let msg = ExecuteMsg::burn {
        amount: Uint128::from(500000u128),
        min_profit: None,
        swap_operations: vec![
            SwapOperation {
                pair_address: BLUNA_PAIR.to_string(),
                to_asset_info: AssetInfo::Token {
                    contract_addr: BLUNA.to_string(),
                }
            }
        ]
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: target luna is less than expected");

    // burn bluna
    deps.querier.set_price(BLUNA_PAIR.to_string(), Decimal::percent(90));
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BLUNA_PAIR.to_string(),
                msg: to_binary(&PairExecuteMsg::Swap {
                    offer_asset: Asset {
                        amount: Uint128::from(500000u128),
                        info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        }
                    },
                    belief_price: None,
                    max_spread: Some(Decimal::percent(50)),
                    to: None,
                }).unwrap(),
                funds: vec![
                    Coin { denom: "uluna".to_string(), amount: Uint128::from(500000u128) },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BLUNA.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: ANCHOR_HUB.to_string(),
                    amount: Uint128::from(553889u128),
                    msg: to_binary(&HubCw20HookMsg::Unbond {}).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );
    deps.querier.set_balance("uluna".to_string(), MOCK_CONTRACT_ADDR.to_string(), Uint128::from(500000u128));

    // burn stluna over
    let msg = ExecuteMsg::burn {
        amount: Uint128::from(450000u128),
        min_profit: None,
        swap_operations: vec![
            SwapOperation {
                pair_address: STLUNA_PAIR.to_string(),
                to_asset_info: AssetInfo::Token {
                    contract_addr: STLUNA.to_string(),
                }
            }
        ]
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Generic error: cannot burn more than available minus claimable amount");

    // user2 bond
    deps.querier.set_balance(SPEC_GOV.to_string(), USER2.to_string(), Uint128::from(20000u128));
    let info = mock_info(USER2, &[
        Coin { denom: "uluna".to_string(), amount: Uint128::from(200000u128) },
    ]);
    let msg = ExecuteMsg::bond {
        staker_addr: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());
    deps.querier.set_balance("uluna".to_string(), MOCK_CONTRACT_ADDR.to_string(), Uint128::from(700000u128));

    // burn stluna
    let info = mock_info(CONTROLLER, &[]);
    let msg = ExecuteMsg::burn {
        amount: Uint128::from(500000u128),
        min_profit: None,
        swap_operations: vec![
            SwapOperation {
                pair_address: STLUNA_PAIR.to_string(),
                to_asset_info: AssetInfo::Token {
                    contract_addr: STLUNA.to_string(),
                }
            }
        ]
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: STLUNA_PAIR.to_string(),
                msg: to_binary(&PairExecuteMsg::Swap {
                    offer_asset: Asset {
                        amount: Uint128::from(500000u128),
                        info: AssetInfo::NativeToken {
                            denom: "uluna".to_string(),
                        }
                    },
                    belief_price: None,
                    max_spread: Some(Decimal::percent(50)),
                    to: None,
                }).unwrap(),
                funds: vec![
                    Coin { denom: "uluna".to_string(), amount: Uint128::from(500000u128) },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: STLUNA.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: ANCHOR_HUB.to_string(),
                    amount: Uint128::from(498500u128),
                    msg: to_binary(&HubCw20HookMsg::Unbond {}).unwrap(),
                }).unwrap(),
                funds: vec![],
            }),
        ]
    );
    deps.querier.set_balance("uluna".to_string(), MOCK_CONTRACT_ADDR.to_string(), Uint128::from(200000u128));

    // query burn
    let msg = QueryMsg::burns {};
    let res: Vec<Burn> = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, vec![
        Burn {
            id: 1,
            batch_id: 1,
            input_amount: Uint128::from(500000u128),
            target_amount: Uint128::from(520932u128),
            start_burn: env.block.time.seconds(),
            end_burn: env.block.time.seconds() + 86400u64,
            hub_type: HubType::bluna,
            hub_address: Addr::unchecked(ANCHOR_HUB),
        },
        Burn {
            id: 2,
            batch_id: 1,
            input_amount: Uint128::from(500000u128),
            target_amount: Uint128::from(598200u128),
            start_burn: env.block.time.seconds(),
            end_burn: env.block.time.seconds() + 86400u64,
            hub_type: HubType::stluna,
            hub_address: Addr::unchecked(ANCHOR_HUB),
        },
    ]);

    // check state again
    let msg = QueryMsg::state {};
    let res: State = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, State {
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_bond_amount: Uint128::from(1099800u128),
        total_bond_share: Uint128::from(1099800u128),
        unbonding_amount: Uint128::zero(),
        claimable_amount: Uint128::from(99000u128),
        unbond_counter: 3,
        unbonded_index: Uint128::from(99000u128),
        unbonding_index: Uint128::from(99000u128),
        deposit_fee: Uint128::from(1200u128),
        perf_fee: Uint128::zero(),
        deposit_earning: Uint128::zero(),
        perf_earning: Uint128::zero(),
        burn_counter: 2,
    });

    // simulate collect
    let time = env.block.time.seconds();
    env.block.time = env.block.time.plus_seconds(86400u64);
    deps.querier.set_hub_history(ANCHOR_HUB.to_string(), UnbondHistoryResponse {
        batch_id: 1u64,
        time,
    });
    deps.querier.set_balance(ANCHOR_HUB.to_string(), MOCK_CONTRACT_ADDR.to_string(), Uint128::from(1120000u64));
    let msg = QueryMsg::simulate_collect {};
    let res: SimulateCollectResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    println!("{:?}", res);
    // assert_eq!(res, SimulateCollectResponse {
    //     burnable: Uint128::from(99800u128),
    //     total_bond_amount
    // })
}
