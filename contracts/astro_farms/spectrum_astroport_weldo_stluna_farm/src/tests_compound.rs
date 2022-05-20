use crate::bond::{deposit_farm2_share, deposit_farm_share};
use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::model::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use crate::state::{pool_info_read, pool_info_store, read_config, read_state, state_store};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::generator::ExecuteMsg as AstroportExecuteMsg;
use astroport::pair::{
    Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg,
};
use astroport::router::{ExecuteMsg as AstroportRouterExecuteMsg, SwapOperation};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, Api, Coin, CosmosMsg, Decimal, OwnedDeps, Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum_protocol::gov::ExecuteMsg as GovExecuteMsg;
use spectrum_protocol::gov_proxy::{
    Cw20HookMsg as GovProxyCw20HookMsg, ExecuteMsg as GovProxyExecuteMsg,
};
use std::fmt::Debug;
use std::str;

const SPEC_GOV: &str = "SPEC_GOV";
const SPEC_PLATFORM: &str = "spec_platform";
const SPEC_TOKEN: &str = "spec_token";
const WELDO_TOKEN: &str = "weldo_token";
const STLUNA_TOKEN: &str = "stluna_token";
const ASTROPORT_ROUTER: &str = "astroport_router";
const GOV_PROXY: &str = "gov_proxy";
const ASTROPORT_GENERATOR: &str = "astroport_generator";
const STLUNA_WELDO_LP: &str = "stluna_weldo_lp";
const TEST_CREATOR: &str = "creator";
const TEST_CONTROLLER: &str = "controller";
const FAIL_TOKEN: &str = "fail_token";
const FAIL_LP: &str = "fail_lp";
const USER1: &str = "user1";
const USER2: &str = "user2";
const ANC_MARKET: &str = "anc_market";
const AUST_TOKEN: &str = "aust_token";
const PAIR_CONTRACT: &str = "pair_contract";
const XASTRO_PROXY: &str = "xastro_proxy";
const ASTRO_TOKEN: &str = "astro_token";
const ASTRO_UST_PAIR_CONTRACT: &str = "astro_ust_pair_contract";

const uusd: &str = "uusd";
const uluna: &str = "uluna";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_infos: Vec<RewardInfoResponseItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: String,
    pub farm_share_index: Decimal,
    pub farm2_share_index: Decimal,
    pub auto_spec_share_index: Decimal,
    pub stake_spec_share_index: Decimal,
    pub bond_amount: Uint128,
    pub auto_bond_amount: Uint128,
    pub stake_bond_amount: Uint128,
    pub farm_share: Uint128,
    pub farm2_share: Uint128,
    pub spec_share: Uint128,
    pub auto_bond_share: Uint128,
    pub stake_bond_share: Uint128,
    pub pending_farm_reward: Uint128,
    pub pending_farm2_reward: Uint128,
    pub pending_spec_reward: Uint128,
    pub deposit_amount: Option<Uint128>,
    pub deposit_time: Option<u64>,
}

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance_percent(100);
    deps.querier.with_astroport_pairs(&[(
        &"stluna_tokenweldo_token".to_string(),
        &PairInfo {
            asset_infos: [
                AssetInfo::Token {
                    contract_addr: deps.api.addr_validate(STLUNA_TOKEN).unwrap(),
                },
                AssetInfo::Token {
                    contract_addr: deps.api.addr_validate(WELDO_TOKEN).unwrap(),
                },
            ],
            contract_addr: deps.api.addr_validate(PAIR_CONTRACT).unwrap(),
            liquidity_token: deps.api.addr_validate(STLUNA_WELDO_LP).unwrap(),
            pair_type: PairType::Xyk {},
        },
    )]);
    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1500000u128))],
    );

    let _ = test_config(&mut deps);
    test_register_asset(&mut deps);
    test_compound_unauthorized(&mut deps);
    test_compound_zero(&mut deps);
    test_bond(&mut deps);
    test_compound_farm_token_and_astro_with_fees(&mut deps);
    // compound ASTRO only because gov_proxy is not set,
    // compound ASTRO only because gov_proxy is set but no STLUNA_TOKEN in contract,
    // compound STLUNA_TOKEN and ASTRO,
    // compound STLUNA_TOKEN only
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: TEST_CREATOR.to_string(),
        spectrum_gov: SPEC_GOV.to_string(),
        spectrum_token: SPEC_TOKEN.to_string(),
        gov_proxy: None,
        weldo_token: WELDO_TOKEN.to_string(),
        astroport_generator: ASTROPORT_GENERATOR.to_string(),
        platform: SPEC_PLATFORM.to_string(),
        controller: TEST_CONTROLLER.to_string(),
        community_fee: Decimal::zero(),
        platform_fee: Decimal::zero(),
        controller_fee: Decimal::zero(),
        deposit_fee: Decimal::zero(),
        anchor_market: ANC_MARKET.to_string(),
        aust_token: AUST_TOKEN.to_string(),
        pair_contract: PAIR_CONTRACT.to_string(),
        xastro_proxy: XASTRO_PROXY.to_string(),
        astro_token: ASTRO_TOKEN.to_string(),
        astro_ust_pair_contract: ASTRO_UST_PAIR_CONTRACT.to_string(),
        astroport_router: ASTROPORT_ROUTER.to_string(),
        stluna_token: STLUNA_TOKEN.to_string(),
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
            total_farm2_share: Uint128::zero(),
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
        asset_token: STLUNA_TOKEN.to_string(),
        staking_token: STLUNA_WELDO_LP.to_string(),
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
                asset_token: STLUNA_TOKEN.to_string(),
                staking_token: STLUNA_WELDO_LP.to_string(),
                weight: 1u32,
                farm_share: Uint128::zero(),
                farm2_share: Uint128::zero(),
                state_spec_share_index: Decimal::zero(),
                stake_spec_share_index: Decimal::zero(),
                auto_spec_share_index: Decimal::zero(),
                farm_share_index: Decimal::zero(),
                total_stake_bond_amount: Uint128::zero(),
                total_stake_bond_share: Uint128::zero(),
                total_auto_bond_share: Uint128::zero(),
                farm2_share_index: Decimal::zero(),
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
    let msg = ExecuteMsg::compound {
        threshold_compound_astro: Some(Uint128::from(10000u128)),
    };
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_err());
}

fn test_compound_zero(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    // reinvest zero
    let env = mock_env();
    let info = mock_info(TEST_CONTROLLER, &[]);
    let msg = ExecuteMsg::compound {
        threshold_compound_astro: Some(Uint128::from(10000u128)),
    };
    let res = execute(deps.as_mut(), env, info, msg).unwrap();

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: ASTROPORT_GENERATOR.to_string(),
            funds: vec![],
            msg: to_binary(&AstroportExecuteMsg::Withdraw {
                lp_token: deps.api.addr_validate(STLUNA_WELDO_LP).unwrap(),
                amount: Uint128::zero()
            })
            .unwrap(),
        }),]
    );
}

fn test_bond(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) {
    deps.querier.with_token_balances(&[]);

    // bond err
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER1.to_string(),
        amount: Uint128::from(10000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: STLUNA_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(100)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // bond ok user1 1000 FARM-LP
    let info = mock_info(STLUNA_WELDO_LP, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let config = read_config(deps_ref.storage).unwrap();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.stluna_token.as_slice())
        .unwrap();
    deposit_farm_share(
        deps_ref,
        &env,
        &mut state,
        &mut pool_info,
        &config,
        Uint128::zero(),
    )
    .unwrap();
    deposit_farm2_share(
        deps_ref,
        &env,
        &mut state,
        &mut pool_info,
        &config,
        Uint128::zero(),
    )
    .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.weldo_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(10000u128))],
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
            asset_token: STLUNA_TOKEN.to_string(),
            pending_farm_reward: Uint128::zero(),
            pending_farm2_reward: Uint128::zero(),
            pending_spec_reward: Uint128::from(2700u128),
            deposit_amount: Option::from(Uint128::from(10000u128)),
            bond_amount: Uint128::from(10000u128),
            auto_bond_amount: Uint128::from(10000u128),
            stake_bond_amount: Uint128::zero(),
            farm_share_index: Decimal::zero(),
            farm2_share_index: Default::default(),
            auto_spec_share_index: Decimal::zero(),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::zero(),
            farm2_share: Uint128::zero(),
            spec_share: Uint128::from(2700u128),
            auto_bond_share: Uint128::from(10000u128),
            stake_bond_share: Uint128::zero(),
            deposit_time: Some(1571797419)
        }]
    );

    // unbond 3000 FARM-LP
    let info = mock_info(USER1, &[]);
    let msg = ExecuteMsg::unbond {
        asset_token: STLUNA_TOKEN.to_string(),
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
                    lp_token: deps.api.addr_validate(STLUNA_WELDO_LP).unwrap(),
                    amount: Uint128::from(3000u128),
                })
                .unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: STLUNA_WELDO_LP.to_string(),
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
    let msg = ExecuteMsg::withdraw {
        asset_token: None,
        spec_amount: None,
        farm_amount: None,
        farm2_amount: None,
    };
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
            })
        ]
    );

    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(7000u128))],
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
            asset_token: STLUNA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_farm2_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(0u128),
            deposit_amount: Some(Uint128::from(7000u128)),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(7000u128),
            stake_bond_amount: Uint128::zero(),
            farm_share_index: Decimal::zero(),
            farm2_share_index: Default::default(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::from(0u128),
            farm2_share: Uint128::from(0u128),
            spec_share: Uint128::from(0u128),
            auto_bond_share: Uint128::from(7000u128),
            stake_bond_share: Uint128::zero(),
            deposit_time: Some(1571797419)
        },]
    );

    // bond user2 5000 STLUNA_TOKEN -LP auto-stake
    let info = mock_info(STLUNA_WELDO_LP, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: USER2.to_string(),
        amount: Uint128::from(5000u128),
        msg: to_binary(&Cw20HookMsg::bond {
            staker_addr: None,
            asset_token: STLUNA_TOKEN.to_string(),
            compound_rate: Some(Decimal::percent(100)),
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let deps_ref = deps.as_ref();
    let mut state = read_state(deps_ref.storage).unwrap();
    let mut pool_info = pool_info_read(deps_ref.storage)
        .load(config.weldo_token.as_slice())
        .unwrap();
    deposit_farm_share(
        deps_ref,
        &env,
        &mut state,
        &mut pool_info,
        &config,
        Uint128::zero(),
    )
    .unwrap();
    deposit_farm2_share(
        deps_ref,
        &env,
        &mut state,
        &mut pool_info,
        &config,
        Uint128::zero(),
    )
    .unwrap();
    state_store(deps.as_mut().storage).save(&state).unwrap();
    pool_info_store(deps.as_mut().storage)
        .save(config.weldo_token.as_slice(), &pool_info)
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12000u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    /*
        USER1 7000 (auto 7000, stake 0)
        USER2 5000 (auto 5000, stake 0)
        Total lp 12000
        Total farm share 0
        SPEC reward +1000
        USER1 SPEC reward ~ 583
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
            asset_token: STLUNA_TOKEN.to_string(),
            pending_farm_reward: Uint128::from(0u128),
            pending_farm2_reward: Uint128::from(0u128),
            pending_spec_reward: Uint128::from(583u128),
            deposit_amount: Some(Uint128::from(7000u128)),
            bond_amount: Uint128::from(7000u128),
            auto_bond_amount: Uint128::from(7000u128),
            stake_bond_amount: Uint128::zero(),
            farm_share_index: Decimal::zero(),
            farm2_share_index: Default::default(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::zero(),
            farm2_share: Uint128::zero(),
            spec_share: Uint128::from(583u128),
            auto_bond_share: Uint128::from(7000u128),
            stake_bond_share: Uint128::zero(),
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
            asset_token: STLUNA_TOKEN.to_string(),
            pending_farm_reward: Uint128::zero(),
            pending_farm2_reward: Uint128::zero(),
            pending_spec_reward: Uint128::from(416u128),
            deposit_amount: Some(Uint128::from(5000u128)),
            bond_amount: Uint128::from(5000u128),
            auto_bond_amount: Uint128::from(5000u128),
            stake_bond_amount: Uint128::zero(),
            farm_share_index: Decimal::zero(),
            farm2_share_index: Default::default(),
            auto_spec_share_index: Decimal::from_ratio(270u128, 1000u128),
            stake_spec_share_index: Decimal::zero(),
            farm_share: Uint128::zero(),
            farm2_share: Uint128::zero(),
            spec_share: Uint128::from(416u128),
            auto_bond_share: Uint128::from(5000u128),
            stake_bond_share: Uint128::zero(),
            deposit_time: Some(1571797419)
        }]
    );
}

fn test_compound_farm_token_and_astro_with_fees(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
) {
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

    // compound first time, has no UST in contract yet
    deps.querier.with_token_balances(&[
        (
            &WELDO_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(6050u128))],
        ),
        (
            &ASTRO_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(6050u128))],
        ),
        (
            &ASTROPORT_GENERATOR.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(12100u128))],
        ),
        (
            &SPEC_GOV.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1000u128))],
        ),
    ]);

    let msg = ExecuteMsg::compound {
        threshold_compound_astro: Some(Uint128::from(1u128)),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    let WELDO_TOKEN_ADDR = deps.api.addr_validate(&WELDO_TOKEN.to_string()).unwrap();
    let STLUNA_TOKEN_ADDR = deps.api.addr_validate(&STLUNA_TOKEN.to_string()).unwrap();

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
                    lp_token: deps.api.addr_validate(STLUNA_WELDO_LP).unwrap(),
                    amount: Uint128::zero()
                }).unwrap(),
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: WELDO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: ASTROPORT_ROUTER.to_string(),
                    amount: Uint128::from(302u128),
                    msg: to_binary(&AstroportRouterExecuteMsg::ExecuteSwapOperations {
                        operations: vec![
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::Token {
                                    contract_addr: WELDO_TOKEN_ADDR,
                                },
                                ask_asset_info: AssetInfo::Token {
                                    contract_addr: STLUNA_TOKEN_ADDR.clone(),
                                },
                            },
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::Token {
                                    contract_addr: STLUNA_TOKEN_ADDR.clone(),
                                },
                                ask_asset_info: AssetInfo::NativeToken {
                                    denom: uluna.to_string()
                                },
                            },
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::NativeToken {
                                    denom: uluna.to_string()
                                },
                                ask_asset_info: AssetInfo::NativeToken {
                                    denom: uusd.to_string()
                                },
                            },
                        ],
                        minimum_receive: None,
                        to: None,
                        max_spread: Some(Decimal::percent(50))
                    })
                    .unwrap(),
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTRO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: ASTRO_UST_PAIR_CONTRACT.to_string(),
                    amount: Uint128::from(6050u128),
                    msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                        max_spread: Some(Decimal::percent(100)),
                        belief_price: None,
                        to: None,
                    }).unwrap() 
                })
                .unwrap(),
                funds: vec![],
            }),
            // CosmosMsg::Wasm(WasmMsg::Execute {
            //     contract_addr: PAIR_CONTRACT.to_string(),
            //     msg: to_binary(&AstroportPairExecuteMsg::Swap {
            //         to: None,
            //         max_spread: Some(Decimal::percent(100)),
            //         belief_price: None,
            //         offer_asset: Asset {
            //             info: AssetInfo::NativeToken {
            //                 denom: uusd.to_string(),
            //             },
            //             amount: Uint128::from(6050u128),
            //         }
            //     })
            //     .unwrap(),
            //     funds: vec![Coin {
            //         denom: uusd.to_string(),
            //         amount: Uint128::from(6050u128)
            //     }]
            // }),
            // CosmosMsg::Wasm(WasmMsg::Execute {
            //     contract_addr: STLUNA_TOKEN.to_string(),
            //     msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            //         spender: PAIR_CONTRACT.to_string(),
            //         amount: Uint128::from(5618u128),
            //         expires: None
            //     })
            //     .unwrap(),
            //     funds: vec![],
            // }),
            // CosmosMsg::Wasm(WasmMsg::Execute {
            //     contract_addr: PAIR_CONTRACT.to_string(),
            //     msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
            //         assets: [
            //             Asset {
            //                 info: AssetInfo::Token {
            //                     contract_addr: deps.api.addr_validate(STLUNA_TOKEN).unwrap(),
            //                 },
            //                 amount: Uint128::from(5618u128),
            //             },
            //             Asset {
            //                 info: AssetInfo::NativeToken {
            //                     denom: "uusd".to_string(),
            //                 },
            //                 amount: Uint128::from(5617u128),
            //             },
            //         ],
            //         slippage_tolerance: None,
            //         auto_stake: Some(true),
            //         receiver: None
            //     })
            //     .unwrap(),
            //     funds: vec![Coin {
            //         denom: "uusd".to_string(),
            //         amount: Uint128::from(5617u128),
            //     }],
            // }),
            // CosmosMsg::Wasm(WasmMsg::Execute {
            //     contract_addr: env.contract.address.to_string(),
            //     msg: to_binary(&ExecuteMsg::stake {
            //         asset_token: STLUNA_TOKEN.to_string(),
            //     })
            //         .unwrap(),
            //     funds: vec![],
            // }),
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
        ]
    );


    deps.querier.with_token_balances(&[
        (
            &AUST_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(590u128))],
        ),
        (
            &ASTRO_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100_000_000u128))],
        ),
        (
            &WELDO_TOKEN.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100_000_000u128))],
        ),
        (
            &uusd.to_string(),
            &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(100_000_000u128))],
        ),
    ]);
    // cannot call send fee from others
    let info = mock_info(SPEC_GOV, &[]);
    let msg = ExecuteMsg::send_fee {};
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    for message in res.messages.clone() {
        if let CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) = message.msg {
            println!("{}", String::from_utf8(msg.0).unwrap());
        }
    }

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
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: WELDO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                     contract: PAIR_CONTRACT.to_string(), 
                     amount: Uint128::from(50073864u128), 
                     msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                        to: None,
                        max_spread: Some(Decimal::percent(100)),
                        belief_price: None,
                     }).unwrap()                     
                    }).unwrap(),
                funds: vec![]
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: WELDO_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: PAIR_CONTRACT.to_string(),
                    amount: Uint128::from(49926136u128),
                    expires: None
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: STLUNA_TOKEN.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: PAIR_CONTRACT.to_string(),
                    amount: Uint128::from(49923643u128),
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
                                contract_addr: deps.api.addr_validate(WELDO_TOKEN).unwrap(),
                            },
                            amount: Uint128::from(49926136u128),
                        },
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: deps.api.addr_validate(STLUNA_TOKEN).unwrap(),
                            },
                            amount: Uint128::from(49923643u128),
                        },
                    ],
                    slippage_tolerance: None,
                    auto_stake: Some(true),
                    receiver: None
                })
                .unwrap(),
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ASTROPORT_ROUTER.to_string(),
                msg: to_binary(&AstroportRouterExecuteMsg::ExecuteSwapOperations {
                    operations: vec![
                        SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::NativeToken { denom: "uusd".to_string() },
                            ask_asset_info: AssetInfo::NativeToken { denom: "uluna".to_string() },
                        },
                        SwapOperation::AstroSwap {
                            offer_asset_info: AssetInfo::NativeToken { denom: "uluna".to_string() },
                            ask_asset_info: AssetInfo::Token { contract_addr: STLUNA_TOKEN_ADDR.clone() },
                        },
                    ],
                    minimum_receive: None,
                    to: None,
                    max_spread: Some(Decimal::percent(50))
                }).unwrap(),
                funds: vec![
                    Coin { denom: "uusd".to_string(), amount: Uint128::from(99009900u128) }
                ],
            })
        ]
    );
}
