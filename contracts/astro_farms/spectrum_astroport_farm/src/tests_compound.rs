use crate::bond::{deposit_farm_share};
use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{pool_info_read, pool_info_store, read_config, read_state, state_store};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, to_binary, Coin, CosmosMsg, Decimal, OwnedDeps, Uint128, WasmMsg, Api};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use std::fmt::Debug;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::pair::{Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg};
use spectrum_protocol::astroport_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use astroport::generator::{
    ExecuteMsg as AstroportExecuteMsg,
};
use spectrum_protocol::gov_proxy::{ExecuteMsg as GovProxyExecuteMsg, Cw20HookMsg as GovProxyCw20HookMsg};
use std::str;

const SPEC_GOV: &str = "SPEC_GOV";
const SPEC_PLATFORM: &str = "spec_platform";
const SPEC_TOKEN: &str = "spec_token";
const ASTRO_GOV_PROXY: &str = "astro_gov_proxy";
const ASTRO_TOKEN: &str = "astro_token";
const ASTROPORT_GENERATOR: &str = "astroport_generator";
const ASTRO_LP: &str = "astro_lp";
const TEST_CREATOR: &str = "creator";
const TEST_CONTROLLER: &str = "controller";
const FAIL_TOKEN: &str = "fail_token";
const FAIL_LP: &str = "fail_lp";
const USER1: &str = "user1";
const USER2: &str = "user2";
const ANC_MARKET: &str = "anc_market";
const AUST_TOKEN: &str = "aust_token";
const PAIR_CONTRACT: &str = "pair_contract";


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
    pub deposit_amount: Option<Uint128>,
    pub deposit_time: Option<u64>,
}

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);
    deps.querier.with_astroport_pairs(&[
        (
            &"uusdastro_token".to_string(),
            &PairInfo {
                asset_infos: [
                    AssetInfo::Token {
                        contract_addr: deps.api.addr_validate(ASTRO_TOKEN).unwrap(),
                    },
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                contract_addr: deps.api.addr_validate(PAIR_CONTRACT).unwrap(),
                liquidity_token: deps.api.addr_validate(ASTRO_LP).unwrap(),
                pair_type: PairType::Xyk {}
            },
        )
    ]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1500000u128))],
    );

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_compound_unauthorized(&mut deps);
    test_compound_zero(&mut deps);
    test_bond(&mut deps);
    test_compound_astro(&mut deps);
    test_compound_astro_with_fees(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        astroport_generator: ASTROPORT_GENERATOR.to_string(),
        astro_gov_proxy: ASTRO_GOV_PROXY.to_string(),
        astro_token: ASTRO_TOKEN.to_string(),
        platform: SPEC_PLATFORM.to_string(),
        controller: TEST_CONTROLLER.to_string(),
        base_denom: "uusd".to_string(),
        community_fee: Decimal::zero(),
        platform_fee: Decimal::zero(),
        controller_fee: Decimal::zero(),
        deposit_fee: Decimal::zero(),
        anchor_market: ANC_MARKET.to_string(),
        aust_token: AUST_TOKEN.to_string(),
        pair_contract: PAIR_CONTRACT.to_string(),
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
            total_farm_share: Uint128::zero(),
            total_weight: 0u32,
            spec_share_index: Decimal::zero(),
            earning: Uint128::zero(),
        }
    );

    // alter config, validate owner
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::update_config {
        owner: Some(SPEC_GOV.to_string()),
        controller: None,
        community_fee: None,
        platform_fee: None,
        controller_fee: None,
        deposit_fee: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(TEST_CREATOR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    config.owner = SPEC_GOV.to_string();
    assert_eq!(res, config);

    config
}

fn test_register_asset(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // no permission
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::register_asset {
        asset_token: ASTRO_TOKEN.to_string(),
        staking_token: ASTRO_LP.to_string(),
        weight: 1u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(SPEC_GOV, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query pool info
    let msg = QueryMsg::pools {};
    let res: PoolsResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PoolsResponse {
            pools: vec![PoolItem {
                asset_token: ASTRO_TOKEN.to_string(),
                staking_token: ASTRO_LP.to_string(),
                weight: 1u32,
                farm_share: Uint128::zero(),
                state_spec_share_index: Decimal::zero(),
                stake_spec_share_index: Decimal::zero(),
                auto_spec_share_index: Decimal::zero(),
                farm_share_index: Decimal::zero(),
                total_stake_bond_amount: Uint128::zero(),
                total_stake_bond_share: Uint128::zero(),
                total_auto_bond_share: Uint128::zero(),
            }]
        }
    );

    // test register fail
    let msg = ExecuteMsg::register_asset {
        asset_token: FAIL_TOKEN.to_string(),
        staking_token: FAIL_LP.to_string(),
        weight: 2u32,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, 1u32);
}

fn test_compound_unauthorized(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_err());
}

fn test_compound_zero(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest zero
    let env = mock_env();
    let info = mock_info(TEST_CONTROLLER, &[]);
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTROPORT_GENERATOR.to_string(),
                funds: vec![],
                msg: to_binary(&AstroportExecuteMsg::Withdraw {
                    lp_token: deps.api.addr_validate(ASTRO_LP).unwrap(),
                    amount: Uint128::zero()
                }
                ).unwrap(),
            }),
        ]
    );
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // bond err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(10000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: ASTRO_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(60)),
        }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // bond success user1 1000 ASTRO-LP
    let info = mock_info(ASTRO_LP, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
        )
    ]);

    //update_bond success
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::update_bond {
        asset_token: ASTRO_TOKEN.to_string(),
        amount_to_stake: Uint128::from(1u128),
        amount_to_auto: Uint128::from(9999u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    //update_bond fail due to exceed deposited amount
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::update_bond {
        asset_token: ASTRO_TOKEN.to_string(),
        amount_to_stake: Uint128::from(2u128),
        amount_to_auto: Uint128::from(9999u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    //update_bond again to original value
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::update_bond {
        asset_token: ASTRO_TOKEN.to_string(),
        amount_to_stake: Uint128::from(4000u128),
        amount_to_auto: Uint128::from(6000u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let config = read_config(deps_ref.storage).unwrap();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.astro_token.as_slice())
        .unwrap();
    deposit_farm_share(
        deps_ref,
        &env,
        &mut state,
        &mut pool_info,
        &config,
        Uint128::from(500u128),
    )
        .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.astro_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
        ),
        (
            &ASTRO_GOV_PROXY.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(2700u128))],
        ),
    ]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: ASTRO_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(1000u128),
            pending_spec_reward: Uint128::from(2700u128),
            bond_amount: Uint128::from(10000u128),
            auto_bond_amount: Uint128::from(6000u128),
            stake_bond_amount: Uint128::from(4000u128),
            farm_share_index: Decimal::zero(),
            auto_spec_share_index: Decimal::zero(),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(500u128),
            spec_share: Uint128::from(2700u128),
            auto_bond_share: Uint128::from(6000u128),
            stake_bond_share: Uint128::from(4000u128),
            deposit_amount: Option::from(Uint128::from(10000u128)),
            deposit_time: Some(1571797419)
        }]
    );

    // unbond 3000 ASTRO-LP
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unbond {
        asset_token: ASTRO_TOKEN.to_string(),
        amount: Uint128::from(3000u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap()
            .messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTROPORT_GENERATOR.to_string(),
                funds: vec![],
                msg: to_binary(&AstroportExecuteMsg::Withdraw {
                    amount:Uint128::from(3000u128),
                    lp_token: deps.api.addr_validate(ASTRO_LP).unwrap(),
                }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_LP.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(3000u128),
                })
                    .unwrap(),
            }),
        ]
    );

    // withdraw rewards
    let msg = ExecuteMsg::withdraw { asset_token: None, spec_amount: None, farm_amount: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap()
            .messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                funds: vec![],
                msg: to_binary(&GovExecuteMsg::withdraw {
                    amount: Some(Uint128::from(2700u128)),
                    days: None,
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(2700u128),
                })
                    .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_GOV_PROXY.to_string(),
                funds: vec![],
                msg: to_binary(&GovProxyExecuteMsg::Unstake {
                    amount: Some(Uint128::from(1000u128)),
                }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: USER1.to_string(),
                    amount: Uint128::from(1000u128),
                })
                    .unwrap(),
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(7000u128))],
        ),
        (
            &ASTRO_GOV_PROXY.to_string(),
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
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.reward_infos, vec![]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: ASTRO_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(0u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(4200u128),
            stake_bond_amount: Uint128::from(2800u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(0u128),
            spec_share: Uint128::from(0u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
            deposit_amount: Option::from(Uint128::from(7000u128)),
            deposit_time: Some(1571797419)
        }, ]
    );

    // bond user2 5000 ASTRO-LP auto-stake
    let info = mock_info(ASTRO_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(5000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: ASTRO_TOKEN.to_string(),
            compound_rate: None,
        })
            .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.astro_token.as_slice())
        .unwrap();
    deposit_farm_share(
        deps_ref,
        &env,
        &mut state,
        &mut pool_info,
        &config,
        Uint128::from(10000u128),
    )
        .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.astro_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12000u128))],
        ),
        (
            &ASTRO_GOV_PROXY.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(5000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
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
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: ASTRO_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(1794u128),
            pending_spec_reward: Uint128::from(582u128),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(4200u128),
            stake_bond_amount: Uint128::from(2800u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(3589u128),
            spec_share: Uint128::from(582u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
            deposit_amount: Option::from(Uint128::from(7000u128)),
            deposit_time: Some(1571797419)
        }]
    );

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: ASTRO_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(3205u128),
            pending_spec_reward: Uint128::from(416u128),
            bond_amount: Uint128::from(5000u128),
            auto_bond_amount: Uint128::from(0u128),
            stake_bond_amount: Uint128::from(5000u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(6410u128),
            spec_share: Uint128::from(416u128),
            auto_bond_share: Uint128::from(0u128),
            stake_bond_share: Uint128::from(5000u128),
            deposit_amount: Option::from(Uint128::from(5000u128)),
            deposit_time: Some(1571797419)
        }, ]
    );
}

fn test_compound_astro(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    let env = mock_env();
    let info = mock_info(TEST_CONTROLLER, &[]);

    deps.querier.with_token_balances(&[
        (
            &ASTRO_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12000u128))],
        ),
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12000u128))],
        ),
        (
            &ASTRO_GOV_PROXY.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(5000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    /*
    pending rewards 12000 ASTRO
    USER1 7000 (auto 4200, stake 2800)
    USER2 5000 (auto 0, stake 5000)
    total 12000
    auto 4200 / 12000 * 12000 = 4200
    stake 7800 / 12000 * 12000 = 7800
    swap amount 2100 ASTRO -> 2073 UST
    provide UST = 2052
    provide ASTRO = 2052
    remaining = 48
    */
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTROPORT_GENERATOR.to_string(),
                funds: vec![],
                msg: to_binary(&AstroportExecuteMsg::Withdraw {
                    lp_token: deps.api.addr_validate(ASTRO_LP).unwrap(),
                    amount: Uint128::zero()
                }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: PAIR_CONTRACT.to_string(),
                    amount: Uint128::from(2100u128),
                    msg: to_binary(&AstroportCw20HookMsg::Swap {
                        max_spread: None,
                        belief_price: None,
                        to: None,
                    })
                    .unwrap()
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: ASTRO_GOV_PROXY.to_string(),
                    amount: Uint128::from(7800u128),
                    msg: to_binary(&GovProxyCw20HookMsg::Stake {}).unwrap(),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: PAIR_CONTRACT.to_string(),
                    amount: Uint128::from(2052u128),
                    expires: None
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR_CONTRACT.to_string(),
                msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: deps.api.addr_validate(ASTRO_TOKEN).unwrap(),
                            },
                            amount: Uint128::from(2052u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(2052u128),
                        },
                    ],
                    slippage_tolerance: None,
                    auto_stake: Some(true),
                    receiver: None
                }).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(2052u128),
                }],
            })
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12100u128))],
        ),
        (
            &ASTRO_GOV_PROXY.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12800u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    // query balance for user1
    let msg = QueryMsg::reward_info {
        staker_addr: USER1.to_string(),
    };
    let res: RewardInfoResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: ASTRO_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(4594u128),
            pending_spec_reward: Uint128::from(586u128),
            bond_amount: Uint128::from(7100u128),
            auto_bond_amount: Uint128::from(4300u128),
            stake_bond_amount: Uint128::from(2800u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(9189u128),
            spec_share: Uint128::from(586u128),
            auto_bond_share: Uint128::from(4200u128),
            stake_bond_share: Uint128::from(2800u128),
            deposit_amount: Some(Uint128::from(7000u128)),
            deposit_time: Some(1571797419)
        },]
    );

    // query balance for user2
    let msg = QueryMsg::reward_info {
        staker_addr: USER2.to_string(),
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(
        res.reward_infos,
        vec![RewardInfoResponseItem {
            asset_token: ASTRO_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(8205u128),
            pending_spec_reward: Uint128::from(413u128),
            bond_amount: Uint128::from(5000u128),
            auto_bond_amount: Uint128::from(0u128),
            stake_bond_amount: Uint128::from(5000u128),
            farm_share_index: Decimal::from_ratio(125u128, 1000u128),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            farm_share: Uint128::from(16410u128),
            spec_share: Uint128::from(413u128),
            auto_bond_share: Uint128::from(0u128),
            stake_bond_share: Uint128::from(5000u128),
            deposit_amount: Some(Uint128::from(5000u128)),
            deposit_time: Some(1571797419)
        }]
    );
}

fn test_compound_astro_with_fees(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // update fees
    let env = mock_env();
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::update_config {
        owner: None,
        controller: None,
        community_fee: Some(Decimal::percent(3u64)),
        platform_fee: Some(Decimal::percent(1u64)),
        controller_fee: Some(Decimal::percent(1u64)),
        deposit_fee: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let info = mock_info(TEST_CONTROLLER, &[]);

    deps.querier.with_token_balances(&[
        (
            &ASTRO_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12100u128))],
        ),
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12100u128))],
        ),
        (
            &ASTRO_GOV_PROXY.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12800u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    /*
    pending rewards 12100 ASTRO
    USER1 7100 (auto 4300, stake 2800)
    USER2 5000 (auto 0, stake 5000)
    total 12100
    total fee = 605
    remaining = 11495
    auto 4300 / 12100 * 11495 = 4085
    stake 7800 / 12100 * 11495 = 7410
    swap amount 2042 ASTRO -> 2016 UST
    provide UST = 1996
    provide ASTRO = 1996
    remaining = 46
    fee swap amount 605 ASTRO -> 591 UST -> 590 SPEC
    community fee = 363 / 605 * 590 = 354
    platform fee = 121 / 605 * 590 = 118
    controller fee = 121 / 605 * 590 = 118
    total swap amount 2647 ASTRO
    */
    let msg = ExecuteMsg::compound {};
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTROPORT_GENERATOR.to_string(),
                funds: vec![],
                msg: to_binary(&AstroportExecuteMsg::Withdraw {
                    lp_token: deps.api.addr_validate(ASTRO_LP).unwrap(),
                    amount: Uint128::zero()
                }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: PAIR_CONTRACT.to_string(),
                    amount: Uint128::from(2647u128),
                    msg: to_binary(&AstroportCw20HookMsg::Swap {
                        max_spread: None,
                        belief_price: None,
                        to: None,
                    }).unwrap()
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ANC_MARKET.to_string(),
                msg: to_binary(&moneymarket::market::ExecuteMsg::DepositStable {}).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(591u128),
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: SPEC_GOV.to_string(),
                msg: to_binary(&spectrum_protocol::gov::ExecuteMsg::mint {}).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::send_fee {}).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: ASTRO_GOV_PROXY.to_string(),
                    amount: Uint128::from(7410u128),
                    msg: to_binary(&GovProxyCw20HookMsg::Stake {}).unwrap(),
                }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: PAIR_CONTRACT.to_string(),
                    amount: Uint128::from(1996u128),
                    expires: None
                })
                    .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: PAIR_CONTRACT.to_string(),
                msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                    assets: [
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: deps.api.addr_validate(ASTRO_TOKEN).unwrap(),
                            },
                            amount: Uint128::from(1996u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            amount: Uint128::from(1996u128),
                        },
                    ],
                    slippage_tolerance: None,
                    auto_stake: Some(true),
                    receiver: None
                }).unwrap(),
                funds: vec![Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128::from(1996u128),
                }],
            }),
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &AUST_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(590u128))]
        ),
    ]);

    // cannot call send fee from others
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::send_fee {};
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: AUST_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: SPEC_GOV.to_string(),
                    amount: Uint128::from(354u128),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: AUST_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: SPEC_PLATFORM.to_string(),
                    amount: Uint128::from(118u128),
                }).unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: AUST_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: TEST_CONTROLLER.to_string(),
                    amount: Uint128::from(118u128),
                }).unwrap(),
                funds: vec![],
            }),
        ]);
}
