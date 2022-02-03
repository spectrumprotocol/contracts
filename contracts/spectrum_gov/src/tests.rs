use std::str::FromStr;
use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::stake::{calc_mintable, reconcile_balance};
use crate::state::{Config, State, StatePool,};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{Binary, CanonicalAddr, CosmosMsg, Decimal, OwnedDeps, StdError, SubMsg, Uint128, WasmMsg, from_binary, to_binary, to_vec, Api};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::common::OrderBy;
use spectrum_protocol::gov::{BalanceResponse, ConfigInfo, Cw20HookMsg, ExecuteMsg, PollExecuteMsg, PollInfo, PollStatus, PollsResponse, QueryMsg, StateInfo, VaultInfo, VaultsResponse, VoteOption, VoterInfo, VotersResponse, StatePoolInfo};

const VOTING_TOKEN: &str = "voting_token";
const TEST_CREATOR: &str = "creator";
const TEST_VOTER: &str = "voter1";
const TEST_VOTER_2: &str = "voter2";
const TEST_VAULT: &str = "vault1";
const TEST_VAULT_2: &str = "vault2";
const WARCHEST: &str = "warchest";
const BURNVAULT: &str = "burnvault";
const AUST_TOKEN: &str = "aust_token";
const DEFAULT_QUORUM: u64 = 30u64;
const DEFAULT_THRESHOLD: u64 = 50u64;
const DEFAULT_VOTING_PERIOD: u64 = 10000u64;
const DEFAULT_EFFECTIVE_DELAY: u64 = 12342u64;
const DEFAULT_EXPIRATION_PERIOD: u64 = 20000u64;
const DEFAULT_PROPOSAL_DEPOSIT: u128 = 100u128;
const DEFAULT_MINT_PER_BLOCK: u128 = 100u128;
const DEFAULT_WARCHEST_RATIO: u64 = 10u64;
const DEFAULT_MINT_START: u64 = 1_000_000u64;

#[test]
fn test() {
    let mut deps = mock_dependencies(&[]);
    let config = test_config(&mut deps);
    let stake = test_stake(&mut deps);

    let stake = test_poll_executed(&mut deps, &config, stake);
    let stake = test_poll_low_quorum(&mut deps, stake);
    let stake = test_poll_low_threshold(&mut deps, stake);
    let stake = test_poll_expired(&mut deps, stake);

    let stake = test_reward(&mut deps, stake);
    let stake = test_pools(&mut deps, stake);
    test_aust(&mut deps, stake);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test instantiate & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: MOCK_CONTRACT_ADDR.to_string(),
        spec_token: Some(VOTING_TOKEN.to_string()),
        quorum: Decimal::percent(120u64),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: 0,
        effective_delay: DEFAULT_EFFECTIVE_DELAY,
        expiration_period: 0,
        proposal_deposit: Uint128::zero(),
        mint_per_block: Uint128::from(DEFAULT_MINT_PER_BLOCK),
        mint_start: DEFAULT_MINT_START,
        mint_end: DEFAULT_MINT_START + 5,
        warchest_address: None,
        warchest_ratio: Decimal::percent(DEFAULT_WARCHEST_RATIO),
        aust_token: AUST_TOKEN.to_string(),
        burnvault_address: Some(BURNVAULT.to_string()),
        burnvault_ratio: Decimal::percent(50),
    };

    // validate quorum
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), config.clone());
    assert_eq!(res, Err(StdError::generic_err("quorum must be 0 to 1")));

    // validate threshold
    config.quorum = Decimal::percent(DEFAULT_QUORUM);
    config.threshold = Decimal::percent(120u64);
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), config.clone());
    assert_eq!(res, Err(StdError::generic_err("threshold must be 0 to 1")));

    // validate threshold
    config.threshold = Decimal::percent(DEFAULT_THRESHOLD);
    config.effective_delay = 12341u64;
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), config.clone());
    assert_eq!(res, Err(StdError::generic_err("minimum effective_delay is 12342")));

    // success instantiate
    config.effective_delay = DEFAULT_EFFECTIVE_DELAY;
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, config);

    // read state
    let msg = QueryMsg::state { };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            poll_count: 0,
            poll_deposit: Uint128::zero(),
            last_mint: env.block.height,
            total_weight: 0,
            total_staked: Uint128::zero(),
            prev_balance: Uint128::zero(),
            pools: vec![StatePoolInfo {
                days: 0u64,
                total_balance: Uint128::zero(),
                total_share: Uint128::zero(),
                weight: 1u32,
                aust_index: Decimal::zero(),
            }],
            prev_aust_balance: Uint128::zero(),
            vault_balances: Uint128::zero(),
            vault_share_multiplier: Decimal::one(),
            pool_weight: 1,
        }
    );

    // alter config, validate owner
    let msg = ExecuteMsg::update_config {
        owner: None,
        spec_token: None,
        quorum: None,
        threshold: None,
        voting_period: Some(DEFAULT_VOTING_PERIOD),
        effective_delay: Some(DEFAULT_EFFECTIVE_DELAY),
        expiration_period: Some(DEFAULT_EXPIRATION_PERIOD),
        proposal_deposit: Some(Uint128::from(DEFAULT_PROPOSAL_DEPOSIT)),
        warchest_address: None,
        burnvault_address: None,
        burnvault_ratio: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    config.voting_period = DEFAULT_VOTING_PERIOD;
    config.effective_delay = DEFAULT_EFFECTIVE_DELAY;
    config.expiration_period = DEFAULT_EXPIRATION_PERIOD;
    config.proposal_deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    assert_eq!(res, config);

    // alter config, validate value
    let msg = ExecuteMsg::update_config {
        owner: None,
        spec_token: None,
        quorum: None,
        threshold: Some(Decimal::percent(120u64)),
        voting_period: None,
        effective_delay: None,
        expiration_period: None,
        proposal_deposit: None,
        warchest_address: None,
        burnvault_address: None,
        burnvault_ratio: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(res, Err(StdError::generic_err("threshold must be 0 to 1")));

    // alter config, validate value
    let msg = ExecuteMsg::update_config {
        owner: None,
        spec_token: None,
        quorum: None,
        threshold: None,
        voting_period: None,
        effective_delay: Some(0u64),
        expiration_period: None,
        proposal_deposit: None,
        warchest_address: None,
        burnvault_address: None,
        burnvault_ratio: None,
    };
    let res = execute(deps.as_mut(), env, info, msg);
    assert_eq!(res, Err(StdError::generic_err("minimum effective_delay is 12342")));

    config
}

fn test_stake(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
) -> (Uint128, Uint128, Uint128) {
    // stake, error
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let stake_amount = Uint128::from(25u128);
    let total_amount = stake_amount;

    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: stake_amount,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: None }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    let info = mock_info(VOTING_TOKEN, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].total_share, total_amount);

    // query account
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    // query account not found
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, Uint128::zero());

    // stake voter2, error (0)
    let stake_amount_2 = Uint128::from(75u128);
    let total_amount = total_amount + stake_amount_2;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: Uint128::zero(),
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: None }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // withdraw failed (0)
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::withdraw {
        amount: Some(stake_amount_2),
        days: None
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // stake voter2
    let info = mock_info(VOTING_TOKEN, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: stake_amount_2,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: None }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].total_share, total_amount);

    // query account
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    // query account 2
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_executed(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    config: &ConfigInfo,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {
    // start poll
    let mut env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let execute_msg = PollExecuteMsg::execute {
        contract: VOTING_TOKEN.to_string(),
        msg: String::from_utf8(
            to_vec(&Cw20ExecuteMsg::Burn {
                amount: Uint128::from(123u128),
            })
            .unwrap(),
        )
        .unwrap(),
    };
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: deposit,
        msg: to_binary(&Cw20HookMsg::poll_start {
            title: "title".to_string(),
            description: "description".to_string(),
            link: None,
            execute_msgs: vec![execute_msg.clone()],
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    let total_amount = total_amount + deposit;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // read state
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.poll_count, 1u64);
    assert_eq!(res.poll_deposit, deposit);

    let poll = PollInfo {
        id: 1u64,
        creator: TEST_VOTER.to_string(),
        status: PollStatus::in_progress,
        end_height: env.block.height + config.voting_period,
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        deposit_amount: deposit,
        execute_msgs: vec![execute_msg.clone()],
        yes_votes: Uint128::zero(),
        no_votes: Uint128::zero(),
        total_balance_at_end_poll: None,
    };

    // query polls
    let msg = QueryMsg::polls {
        filter: None,
        start_after: None,
        limit: None,
        order_by: None,
    };
    let res: PollsResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PollsResponse {
            polls: vec![poll.clone()]
        }
    );

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, poll);

    // vote failed (id not found)
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // vote failed (not enough amount)
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount + Uint128::from(1u128),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // vote success
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.yes_votes, stake_amount);
    assert_eq!(res.no_votes, Uint128::zero());

    // query account
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res.locked_balance,
        vec![(
            1,
            VoterInfo {
                vote: VoteOption::yes,
                balance: stake_amount
            }
        )]
    );

    // vote failed (duplicate)
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());
    // end poll failed (voting period not end)
    let msg = ExecuteMsg::poll_end { poll_id: 1 };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // vote 2
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // withdraw all failed
    let msg = ExecuteMsg::withdraw {
        amount: Some(stake_amount_2),
        days: None
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // withdraw non-vote is ok
    let withdraw_amount = stake_amount_2.checked_sub(stake_amount).unwrap();
    let msg = ExecuteMsg::withdraw {
        amount: Some(withdraw_amount),
        days: None
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    let total_amount = total_amount.checked_sub(withdraw_amount).unwrap();
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // query account 2
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    // query voters
    let msg = QueryMsg::voters {
        poll_id: 1,
        start_after: None,
        limit: None,
        order_by: None,
    };
    let res: VotersResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        VotersResponse {
            voters: vec![
                (
                    TEST_VOTER.to_string(),
                    VoterInfo {
                        vote: VoteOption::yes,
                        balance: stake_amount,
                    }
                ),
                (
                    TEST_VOTER_2.to_string(),
                    VoterInfo {
                        vote: VoteOption::yes,
                        balance: stake_amount,
                    }
                ),
            ]
        }
    );

    // end poll success
    let info = mock_info(TEST_VOTER_2, &[]);
    env.block.height += DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 1 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: poll.creator.to_string(),
                amount: poll.deposit_amount,
            })
            .unwrap(),
        })
    );

    // read state
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.poll_deposit, Uint128::zero());

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    let total_amount = total_amount.checked_sub(deposit).unwrap();
    assert_eq!(res.status, PollStatus::passed);
    assert_eq!(res.total_balance_at_end_poll, Some(total_amount));
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // withdraw all success after end poll
    let msg = ExecuteMsg::withdraw { amount: None, days: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    let total_amount = total_amount.checked_sub(stake_amount).unwrap();
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // query account 2
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, Uint128::zero());

    // stake voter2
    let total_amount = total_amount + stake_amount_2;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let info = mock_info(VOTING_TOKEN, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: stake_amount_2,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: None }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query account 2
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    // end poll failed (not in progress)
    let msg = ExecuteMsg::poll_end { poll_id: 1 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // execute failed (wait period)
    let msg = ExecuteMsg::poll_execute { poll_id: 1 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    // execute success
    env.block.height += DEFAULT_EFFECTIVE_DELAY;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    let (contract_addr, msg) = match execute_msg {
        PollExecuteMsg::execute { contract, msg } => (contract, msg),
    };
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg: Binary(msg.into_bytes()),
            funds: vec![],
        }))
    );

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::executed);

    // execute failed (status)
    let msg = ExecuteMsg::poll_execute { poll_id: 1 };
    let res = execute(deps.as_mut(), env, info, msg);
    assert!(res.is_err());

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_low_quorum(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {
    // start poll2
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let mut env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: deposit,
        msg: to_binary(&Cw20HookMsg::poll_start {
            title: "title".to_string(),
            description: "description".to_string(),
            link: None,
            execute_msgs: vec![
                PollExecuteMsg::execute {
                    contract: VOTING_TOKEN.to_string(),
                    msg: String::from_utf8(
                        to_vec(&Cw20ExecuteMsg::Burn {
                            amount: Uint128::from(123u128),
                        }).unwrap()).unwrap(),
                }
            ],
        }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // start poll 3
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: deposit,
        msg: to_binary(&Cw20HookMsg::poll_start {
            title: "title".to_string(),
            description: "description".to_string(),
            link: None,
            execute_msgs: vec![],
        }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let total_amount = total_amount + deposit + deposit;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // vote poll2
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // vote poll failed (expired)
    let info = mock_info(TEST_CREATOR, &[]);
    env.block.height += DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::no,
        amount: stake_amount_2,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // mint before end poll
    let msg = ExecuteMsg::mint {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // end poll success
    let msg = ExecuteMsg::poll_end { poll_id: 2 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_VOTER.to_string(),
                amount: Uint128::from(83u64),  // 100 (deposit) * 25 (yes) / [.3 (quorum) * 100 (staked)]
            }).unwrap(),
        })
    );

    // end poll success, always return full for no execution message
    let msg = ExecuteMsg::poll_end { poll_id: 3 };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_VOTER.to_string(),
                amount: Uint128::from(DEFAULT_PROPOSAL_DEPOSIT),
            }).unwrap(),
        })
    );

    // read state
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.poll_deposit, Uint128::zero());

    // get poll
    let msg = QueryMsg::poll { poll_id: 2 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some(Uint128::from(100u64))
    );

    // query account
    let total_shares = stake_amount + stake_amount_2;
    let stake_amount = stake_amount.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    // query account 2
    let stake_amount_2 = stake_amount_2.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_low_threshold(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {

    // start poll4
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let mut env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: deposit,
        msg: to_binary(&Cw20HookMsg::poll_start {
            title: "title".to_string(),
            description: "description".to_string(),
            link: None,
            execute_msgs: vec![
                PollExecuteMsg::execute {
                    contract: VOTING_TOKEN.to_string(),
                    msg: String::from_utf8(
                        to_vec(&Cw20ExecuteMsg::Burn {
                            amount: Uint128::from(123u128),
                        }).unwrap()).unwrap(),
                }
            ],
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // start poll 5
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: deposit,
        msg: to_binary(&Cw20HookMsg::poll_start {
            title: "title".to_string(),
            description: "description".to_string(),
            link: None,
            execute_msgs: vec![],
        }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let total_amount = total_amount + deposit + deposit;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // vote poll4
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // vote poll4 as no
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::no,
        amount: stake_amount_2,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // vote poll5 as no
    let msg = ExecuteMsg::poll_vote {
        poll_id: 5,
        vote: VoteOption::no,
        amount: stake_amount_2,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // end poll success
    let info = mock_info(TEST_CREATOR, &[]);
    env.block.height += DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 4 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_VOTER.to_string(),
                amount: Uint128::from(25u64),  // 100 (deposit) * 75 (yes) / [75 (yes) * 225 (no)]
            }).unwrap(),
        })
    );

    // end poll success, always return full for no execution message
    let msg = ExecuteMsg::poll_end { poll_id: 5 };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_VOTER.to_string(),
                amount: Uint128::from(DEFAULT_PROPOSAL_DEPOSIT),
            }).unwrap(),
        })
    );

    // read state
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.poll_deposit, Uint128::zero());

    // get poll
    let msg = QueryMsg::poll { poll_id: 4 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some(Uint128::from(300u64))
    );

    // query account
    let total_shares = stake_amount + stake_amount_2;
    let stake_amount = stake_amount.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    // query account 2
    let stake_amount_2 = stake_amount_2.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_expired(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {
    // start poll
    let mut env = mock_env();
    let info = mock_info(VOTING_TOKEN, &[]);
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let execute_msg = PollExecuteMsg::execute {
        contract: VOTING_TOKEN.to_string(),
        msg: String::from_utf8(
            to_vec(&Cw20ExecuteMsg::Burn {
                amount: Uint128::from(123u128),
            })
            .unwrap(),
        )
        .unwrap(),
    };
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: deposit,
        msg: to_binary(&Cw20HookMsg::poll_start {
            title: "title".to_string(),
            description: "description".to_string(),
            link: None,
            execute_msgs: vec![execute_msg],
        })
        .unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    let total_amount = total_amount + deposit;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // vote success
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 6,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // vote 2
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 6,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // end poll success
    let info = mock_info(TEST_CREATOR, &[]);
    env.block.height += DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 6 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let total_amount = total_amount.checked_sub(deposit).unwrap();
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // expired failed (wait period)
    env.block.height += DEFAULT_EFFECTIVE_DELAY;
    let msg = ExecuteMsg::poll_expire { poll_id: 6 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    // execute success
    env.block.height += DEFAULT_EFFECTIVE_DELAY;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 6 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::expired);

    // expired failed (status)
    let msg = ExecuteMsg::poll_expire { poll_id: 6 };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // query polls
    let msg = QueryMsg::polls {
        filter: Some(PollStatus::rejected),
        start_after: None,
        limit: None,
        order_by: Some(OrderBy::Asc),
    };
    let res: PollsResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.polls[0].id, 2);
    assert_eq!(res.polls[1].id, 3);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_reward(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> Uint128 {
    let mut env = mock_env();
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);

    // mint before add vault
    let msg = ExecuteMsg::mint {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // add vault 1
    let msg = ExecuteMsg::upsert_vault {
        vault_address: TEST_VAULT.to_string(),
        weight: 1,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // add vault 2
    let msg = ExecuteMsg::upsert_vault {
        vault_address: TEST_VAULT_2.to_string(),
        weight: 4,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // modify vault 1
    let msg = ExecuteMsg::upsert_vault {
        vault_address: TEST_VAULT.to_string(),
        weight: 5,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // validate weight
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(9, res.total_weight);

    // validate vaults
    let msg = QueryMsg::vaults {};
    let res: VaultsResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        VaultInfo {
            address: TEST_VAULT_2.to_string(),
            weight: 4,
            balance: Uint128::zero(),
        },
        res.vaults[1]
    );
    assert_eq!(
        VaultInfo {
            address: TEST_VAULT.to_string(),
            weight: 5,
            balance: Uint128::zero(),
        },
        res.vaults[0]
    );

    let msg = ExecuteMsg::update_config {
        owner: None,
        spec_token: None,
        quorum: None,
        threshold: None,
        voting_period: None,
        effective_delay: None,
        expiration_period: None,
        proposal_deposit: None,
        warchest_address: Some(WARCHEST.to_string()),
        burnvault_address: None,
        burnvault_ratio: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let info = mock_info(VOTING_TOKEN, &[]);
    let height = 3u64;
    env.block.height = DEFAULT_MINT_START + height;

    // mint first
    let mint = Uint128::from(300u64);
    let msg = ExecuteMsg::mint {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_eq!(
        res.unwrap().messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                amount: mint,
                recipient: MOCK_CONTRACT_ADDR.to_string(),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    let total_amount = total_amount + mint;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    let burnvault_amount = Uint128::from(150u128);
    let warchest_amount = Uint128::from(15u128);
    let vault_amount = Uint128::from(75u128);
    let vault_amount_2 = Uint128::from(60u128);

    // check balance all users
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    let msg = QueryMsg::balance {
        address: TEST_VAULT.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

    let msg = QueryMsg::balance {
        address: TEST_VAULT_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount_2);

    let msg = QueryMsg::balance {
        address: WARCHEST.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, warchest_amount);

    let msg = QueryMsg::balance {
        address: BURNVAULT.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, burnvault_amount);

    let reward = Uint128::from(665u128);
    let total_amount = total_amount + reward;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let non_vault_amount = stake_amount + stake_amount_2 + warchest_amount + burnvault_amount;
    let warchest_amount = warchest_amount + reward.multiply_ratio(warchest_amount, non_vault_amount);
    let burnvault_amount = burnvault_amount + reward.multiply_ratio(burnvault_amount, non_vault_amount);

    let msg = QueryMsg::balance {
        address: TEST_VAULT.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

    let msg = QueryMsg::balance {
        address: WARCHEST.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, warchest_amount);

    // get balance with height (with exceed mint_end)
    env.block.height += 3u64;
    let mint = Uint128::from(DEFAULT_MINT_PER_BLOCK * 2u128); // mint only 2 blocks because of mint_end
    let to_burnvault = mint * Decimal::percent(50);
    let to_warchest = mint.checked_sub(to_burnvault).unwrap() *
        Decimal::percent(DEFAULT_WARCHEST_RATIO);
    let add_vault_amount = mint.checked_sub(to_burnvault + to_warchest).unwrap();
    let vault_amount = vault_amount + add_vault_amount.multiply_ratio(5u32, 9u32);
    let vault_amount_2 = vault_amount_2 + add_vault_amount.multiply_ratio(4u32, 9u32);
    let warchest_amount = warchest_amount + to_warchest;
    let burnvault_amount = burnvault_amount + to_burnvault;

    let msg = QueryMsg::balance {
        address: TEST_VAULT.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

    let msg = QueryMsg::balance {
        address: TEST_VAULT_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount_2);

    let msg = QueryMsg::balance {
        address: WARCHEST.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, warchest_amount);

    // mint again
    let msg = ExecuteMsg::mint {};
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let total_amount = total_amount + mint;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // withdraw all
    let info = mock_info(TEST_VAULT, &[]);
    let msg = ExecuteMsg::withdraw { amount: None, days: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: VOTING_TOKEN.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: TEST_VAULT.to_string(),
                amount: vault_amount,
            })
            .unwrap(),
            funds: vec![],
        }))
    );
    let total_amount = total_amount.checked_sub(vault_amount).unwrap();
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    let msg = QueryMsg::balance {
        address: BURNVAULT.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, burnvault_amount);

    total_amount
}

fn test_pools(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    total_amount: Uint128,
) -> Uint128 {
    // invalid owner cannot add pool
    let mut env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let msg = ExecuteMsg::upsert_pool { days: 45u64, weight: 1 };
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    // owner can add pool
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query, must show 2 pools
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools.len(), 2);
    assert_eq!(res.pools[1].days, 45u64);

    // pool will always sorted
    let msg = ExecuteMsg::upsert_pool { days: 30u64, weight: 1 };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // query, must show 3 pools
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools.len(), 3);
    assert_eq!(res.pools[1].days, 30u64);
    assert_eq!(res.pools[2].days, 45u64);

    // user 1 stake for 30 days
    let new_amount = Uint128::from(120u128);
    let total_amount = total_amount + new_amount;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let info = mock_info(VOTING_TOKEN, &[]);

    // cannot stake if pool not available
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: new_amount,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: Some(1u64) }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // stake correct pool
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: new_amount,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: Some(30u64) }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query, staker 1
    let seconds_per_day = 24u64 * 60u64 * 60u64;
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools.len(), 2);
    assert_eq!(res.pools[1].days, 30u64);
    assert_eq!(res.pools[1].balance, new_amount);
    assert_eq!(res.pools[1].unlock, env.block.time.seconds() + 30u64 * seconds_per_day);

    // reward
    let total_amount = total_amount + Uint128::from(468u128);
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // query, staker 1
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();

    // pool 0 => 0: 156 (reward) * 1440 (amount) / 1560 (total amount) = +144
    // pool 0 => 1: 156 (reward) * 120 (amount) / 1560 (total amount) = +12
    // pool 1 => 1: 156 (reward) * 120 (amount) / 156 (total amount) = +156
    // pool 2 => 2: 156 (reward)

    // total pool 0: +144
    // total pool 1: +168
    // total pool 2: +156

    assert_eq!(res.pools[0].balance, Uint128::from(275u128));   // 250 (existing) + 144 (pool 0) * 25 (share) / 144 (total share)
    assert_eq!(res.pools[1].balance, Uint128::from(288u128));   // 120 (existing) + 168 (pool 1)

    // deposit pool 45-day
    let new_amount = Uint128::from(100u128);
    let total_amount = total_amount + new_amount;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: new_amount,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: Some(45u64) }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query, staker 1
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].balance, Uint128::from(275u128));
    assert_eq!(res.pools[1].balance, Uint128::from(288u128));
    assert_eq!(res.pools[2].balance, Uint128::from(256u128));   // 100 + 156 (pool 3)

    // loss
    let total_amount = total_amount.checked_sub(Uint128::from(400u128)).unwrap();
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // pool 0 = 1584 (current) - 400 (deduct) * 1584 (current) / 2128 (total) - 1 (remain) = 1286
    // pool 1 = 288 (current) - 400 (deduct) * 288 (current) / 2128 (total) = 234
    // pool 2 = 256(current) - 400 (deduct) * 256 (current) / 2128 (total) = 208

    // query, staker 1
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].balance, Uint128::from(223u128));   // 1286 (pool 0) * 25 (share) / 144 (total share)
    assert_eq!(res.pools[1].balance, Uint128::from(234u128));
    assert_eq!(res.pools[2].balance, Uint128::from(208u128));

    // time +15
    env.block.time = env.block.time.plus_seconds(15 * seconds_per_day);
    let new_amount = Uint128::from(117u128);
    let total_amount = total_amount + new_amount;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: new_amount,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: Some(30u64) }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query, staker 1
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[1].balance, Uint128::from(351u128));
    assert_eq!(res.pools[1].unlock - env.block.time.seconds(), 1728000); // 20 days

    // time +30
    env.block.time = env.block.time.plus_seconds(30 * seconds_per_day);
    let new_amount = Uint128::from(117u128);
    let total_amount = total_amount + new_amount;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER.to_string(),
        amount: new_amount,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None, days: Some(30u64) }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // query, staker 1
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[1].balance, Uint128::from(468u128));
    assert_eq!(res.pools[1].unlock - env.block.time.seconds(), 648000); // 7.5 days

    // cannot withdraw locked
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::withdraw { amount: None, days: Some(30u64) };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // can withdraw unlocked
    let msg = ExecuteMsg::withdraw { amount: None, days: Some(45u64) };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query, staker 1
    let total_amount = total_amount.checked_sub(Uint128::from(208u128)).unwrap();
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].balance, Uint128::from(223u128));
    assert_eq!(res.pools[1].balance, Uint128::from(468u128));
    assert_eq!(res.pools[2].balance, Uint128::from(0u128));

    // cannot move down
    let msg = ExecuteMsg::update_stake { amount: Uint128::from(468u128), from_days: 30u64, to_days: 0u64 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // move up
    let msg = ExecuteMsg::update_stake { amount: Uint128::from(468u128), from_days: 30u64, to_days: 45u64 };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // query staker 1
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].balance, Uint128::from(223u128));
    assert_eq!(res.pools[1].balance, Uint128::from(0u128));
    assert_eq!(res.pools[2].balance, Uint128::from(468u128));
    assert_eq!(res.balance, Uint128::from(691u128));
    assert_eq!(res.pools[2].unlock - env.block.time.seconds(), 648000 + 15 * seconds_per_day);

    total_amount
}

fn test_aust(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    total_amount: Uint128,
) {
    // add aust
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    deps.querier.with_token_balances(&[
        (&AUST_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(600u128))]),
        (&VOTING_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)]),
    ]);

    // pool 0 -> 0: 200 * 1286 (pool 0) / 1754 (total) = 146
    // pool 0 -> 2: 200 * 468 (pool 2) / 1754 (total) = 53 + 1 (remain) = 54
    // pool 1 -> 2: 200
    // pool 2 -> 2: 200

    // pool 0: 146
    // pool 2: 454

    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].pending_aust, Uint128::from(25u128));  // 146 (pool 0) * 25 (share) / 144 (total share)
    assert_eq!(res.pools[1].pending_aust, Uint128::from(0u128));
    assert_eq!(res.pools[2].pending_aust, Uint128::from(453u128));

    // cannot withdraw more than available
    let msg = ExecuteMsg::harvest { aust_amount: Some(Uint128::from(26u128)), days: Some(0u64)};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // withdraw all
    let msg = ExecuteMsg::harvest { aust_amount: None, days: Some(0u64)};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    deps.querier.with_token_balances(&[
        (&AUST_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(575u128))]),
        (&VOTING_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)]),
    ]);

    // withdraw some
    let msg = ExecuteMsg::harvest { aust_amount: Some(Uint128::from(200u128)), days: Some(45u64)};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
    deps.querier.with_token_balances(&[
        (&AUST_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(375u128))]),
        (&VOTING_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)]),
    ]);

    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].pending_aust, Uint128::from(0u128));
    assert_eq!(res.pools[1].pending_aust, Uint128::from(0u128));
    assert_eq!(res.pools[2].pending_aust, Uint128::from(253u128));

    // add more
    deps.querier.with_token_balances(&[
        (&AUST_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(975u128))]),
        (&VOTING_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)]),
    ]);
    let msg = QueryMsg::balance { address: TEST_VOTER.to_string() };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.pools[0].pending_aust, Uint128::from(25u128));  // 146 (pool 0) * 25 (share) / 144 (total share)
    assert_eq!(res.pools[1].pending_aust, Uint128::from(0u128));
    assert_eq!(res.pools[2].pending_aust, Uint128::from(706u128));
}

#[test]
fn test_mintable() {
    // First mint
    let mut state = State {
        contract_addr: CanonicalAddr::from(vec![]),
        poll_count: 0,
        total_share: Uint128::zero(),
        prev_balance: Default::default(),
        prev_aust_balance: Uint128::zero(),
        total_balance: Default::default(),
        poll_deposit: Uint128::zero(),
        last_mint: 0,
        total_weight: 0,
        pools: vec![],
        vault_balances: Uint128::zero(),
        aust_index: Decimal::zero(),
        vault_share_multiplier: Decimal::one(),
        pool_weight: 1u32,
    };
    let config = Config {
        owner: CanonicalAddr::from(vec![]),
        spec_token: CanonicalAddr::from(vec![]),
        quorum: Decimal::percent(120u64),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: 0,
        effective_delay: 0,
        expiration_period: 0,
        proposal_deposit: Uint128::zero(),
        mint_per_block: Uint128::from(1u64),
        mint_start: 10,
        mint_end: 110,
        warchest_address: CanonicalAddr::from(vec![]),
        warchest_ratio: Decimal::zero(),
        aust_token: CanonicalAddr::from(vec![]),
        burnvault_address: CanonicalAddr::from(vec![]),
        burnvault_ratio: Decimal::zero()
    };
    assert_eq!(calc_mintable(&state, &config, 0), Uint128::zero());
    assert_eq!(calc_mintable(&state, &config, 10), Uint128::zero());
    assert_eq!(calc_mintable(&state, &config, 12), Uint128::from(2u64));
    assert_eq!(calc_mintable(&state, &config, 110), Uint128::from(100u64));
    assert_eq!(calc_mintable(&state, &config, 111), Uint128::from(100u64));

    // Next mint
    state.last_mint = 12;
    assert_eq!(calc_mintable(&state, &config, 20), Uint128::from(8u64));
    assert_eq!(calc_mintable(&state, &config, 110), Uint128::from(98u64));
    assert_eq!(calc_mintable(&state, &config, 111), Uint128::from(98u64));

    // // All minted
    state.last_mint = 110;
    assert_eq!(calc_mintable(&state, &config, 0), Uint128::zero());
    assert_eq!(calc_mintable(&state, &config, 10), Uint128::zero());
    assert_eq!(calc_mintable(&state, &config, 110), Uint128::zero());
    assert_eq!(calc_mintable(&state, &config, 111), Uint128::zero());
}

#[test]
fn test_reconcile_balance() {
    let pool = StatePool {
        days: 30u64,
        total_share: Uint128::from(100u128),
        total_balance: Uint128::from(100u128),
        weight: 1,
        aust_index: Decimal::zero(),
    };
    let mut deps = mock_dependencies(&[]);
    let mut state = State {
        contract_addr: deps.api.addr_canonicalize(MOCK_CONTRACT_ADDR).unwrap(),
        poll_count: 0,
        total_share: Uint128::from(100u128),
        prev_balance: Uint128::from(200u128),
        prev_aust_balance: Uint128::zero(),
        total_balance: Uint128::from(100u128),
        poll_deposit: Uint128::zero(),
        last_mint: 0,
        total_weight: 0,
        pools: vec![pool],
        vault_balances: Uint128::zero(),
        aust_index: Decimal::zero(),
        vault_share_multiplier: Decimal::one(),
        pool_weight: 2u32,
    };
    let config = Config {
        owner: deps.api.addr_canonicalize(MOCK_CONTRACT_ADDR).unwrap(),
        spec_token: deps.api.addr_canonicalize(VOTING_TOKEN).unwrap(),
        quorum: Decimal::percent(120u64),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: 0,
        effective_delay: DEFAULT_EFFECTIVE_DELAY,
        expiration_period: 0,
        proposal_deposit: Uint128::zero(),
        mint_per_block: Uint128::from(DEFAULT_MINT_PER_BLOCK),
        mint_start: DEFAULT_MINT_START,
        mint_end: DEFAULT_MINT_START + 5,
        warchest_address: deps.api.addr_canonicalize(WARCHEST).unwrap(),
        warchest_ratio: Decimal::percent(DEFAULT_WARCHEST_RATIO),
        aust_token: deps.api.addr_canonicalize(AUST_TOKEN).unwrap(),
        burnvault_address: deps.api.addr_canonicalize(BURNVAULT).unwrap(),
        burnvault_ratio: Decimal::percent(50),
    };

    deps.querier.with_token_balances(&[
        (&VOTING_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1200u128))]),
        (&AUST_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(400u128))]),
    ]);

    reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero()).unwrap();
    assert_eq!(state.aust_index, Decimal::from_str("1").unwrap());
    assert_eq!(state.pools[0].aust_index, Decimal::from_str("3").unwrap());
    assert_eq!(state.total_balance, Uint128::from(350u128));
    assert_eq!(state.pools[0].total_balance, Uint128::from(850u128));

    deps.querier.with_token_balances(&[
        (&VOTING_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1080u128))]),
        (&AUST_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(1600u128))]),
    ]);

    reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero()).unwrap();
    assert_eq!(state.aust_index, Decimal::from_str("2.75").unwrap());
    assert_eq!(state.pools[0].aust_index, Decimal::from_str("13.25").unwrap());
    assert_eq!(state.total_balance, Uint128::from(315u128));
    assert_eq!(state.pools[0].total_balance, Uint128::from(765u128));
}

#[test]
fn test_reconcile_balance_2() {
    let pool30 = StatePool {
        days: 30u64,
        total_share: Uint128::from(2000u128),
        total_balance: Uint128::from(2000u128),
        weight: 1,
        aust_index: Decimal::zero(),
    };
    let pool180 = StatePool {
        days: 180u64,
        total_share: Uint128::from(1000u128),
        total_balance: Uint128::from(1000u128),
        weight: 2,
        aust_index: Decimal::zero(),
    };
    let mut deps = mock_dependencies(&[]);
    let mut state = State {
        contract_addr: deps.api.addr_canonicalize(MOCK_CONTRACT_ADDR).unwrap(),
        poll_count: 0,
        total_share: Uint128::from(500u128),
        prev_balance: Uint128::from(3500u128),
        prev_aust_balance: Uint128::zero(),
        total_balance: Uint128::from(500u128),
        poll_deposit: Uint128::zero(),
        last_mint: 0,
        total_weight: 0,
        pools: vec![pool30, pool180],
        vault_balances: Uint128::zero(),
        aust_index: Decimal::zero(),
        vault_share_multiplier: Decimal::one(),
        pool_weight: 4u32,
    };
    let config = Config {
        owner: deps.api.addr_canonicalize(MOCK_CONTRACT_ADDR).unwrap(),
        spec_token: deps.api.addr_canonicalize(VOTING_TOKEN).unwrap(),
        quorum: Decimal::percent(120u64),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: 0,
        effective_delay: DEFAULT_EFFECTIVE_DELAY,
        expiration_period: 0,
        proposal_deposit: Uint128::zero(),
        mint_per_block: Uint128::from(DEFAULT_MINT_PER_BLOCK),
        mint_start: DEFAULT_MINT_START,
        mint_end: DEFAULT_MINT_START + 5,
        warchest_address: deps.api.addr_canonicalize(WARCHEST).unwrap(),
        warchest_ratio: Decimal::percent(DEFAULT_WARCHEST_RATIO),
        aust_token: deps.api.addr_canonicalize(AUST_TOKEN).unwrap(),
        burnvault_address: deps.api.addr_canonicalize(BURNVAULT).unwrap(),
        burnvault_ratio: Decimal::percent(50),
    };

    deps.querier.with_token_balances(&[
        (&VOTING_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(3500u128))]),
        (&AUST_TOKEN.to_string(), &[(&MOCK_CONTRACT_ADDR.to_string(), &Uint128::from(400u128))]),
    ]);

    reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero()).unwrap();
    assert_eq!(state.aust_index, Decimal::from_str("0.028").unwrap());
    assert_eq!(state.pools[0].aust_index, Decimal::from_str("0.0615").unwrap());
    assert_eq!(state.pools[1].aust_index, Decimal::from_str("0.263").unwrap());
}
