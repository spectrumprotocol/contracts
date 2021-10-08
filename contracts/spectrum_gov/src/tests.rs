use crate::contract::{execute, instantiate, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::stake::calc_mintable;
use crate::state::{Config, State};
use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{Binary, CanonicalAddr, CosmosMsg, Decimal, OwnedDeps, StdError, SubMsg, Uint128, WasmMsg, from_binary, to_binary, to_vec};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::common::OrderBy;
use spectrum_protocol::gov::{
    BalanceResponse, ConfigInfo, Cw20HookMsg, ExecuteMsg, PollExecuteMsg, PollInfo, PollStatus,
    PollsResponse, QueryMsg, StateInfo, VaultInfo, VaultsResponse, VoteOption, VoterInfo,
    VotersResponse,
};

const VOTING_TOKEN: &str = "voting_token";
const TEST_CREATOR: &str = "creator";
const TEST_VOTER: &str = "voter1";
const TEST_VOTER_2: &str = "voter2";
const TEST_VAULT: &str = "vault1";
const TEST_VAULT_2: &str = "vault2";
const WARCHEST: &str = "warchest";
const DEFAULT_QUORUM: u64 = 30u64;
const DEFAULT_THRESHOLD: u64 = 50u64;
const DEFAULT_VOTING_PERIOD: u64 = 10000u64;
const DEFAULT_EFFECTIVE_DELAY: u64 = 12342u64;
const DEFAULT_EXPIRATION_PERIOD: u64 = 20000u64;
const DEFAULT_PROPOSAL_DEPOSIT: u128 = 100u128;
const DEFAULT_MINT_PER_BLOCK: u128 = 50u128;
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

    test_reward(&mut deps, stake);
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
            total_share: Uint128::zero(),
            last_mint: env.block.height,
            total_weight: 0,
            total_staked: Uint128::zero(),
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
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_err());

    let info = mock_info(VOTING_TOKEN, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.total_share, total_amount);

    // query account
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);
    assert_eq!(res.share, stake_amount);

    // query account not found
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, Uint128::zero());
    assert_eq!(res.share, Uint128::zero());

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
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // withdraw failed (0)
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::withdraw {
        amount: Some(stake_amount_2),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_err());

    // stake voter2
    let info = mock_info(VOTING_TOKEN, &[]);
    let msg = ExecuteMsg::receive(Cw20ReceiveMsg {
        sender: TEST_VOTER_2.to_string(),
        amount: stake_amount_2,
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.total_share, total_amount);

    // query account
    let msg = QueryMsg::balance {
        address: TEST_VOTER.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);
    assert_eq!(res.share, stake_amount);

    // query account 2
    let msg = QueryMsg::balance {
        address: TEST_VOTER_2.to_string(),
    };
    let res: BalanceResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);
    assert_eq!(res.share, stake_amount_2);

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
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // withdraw non-vote is ok
    let withdraw_amount = stake_amount_2.checked_sub(stake_amount).unwrap();
    let msg = ExecuteMsg::withdraw {
        amount: Some(withdraw_amount),
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
    assert_eq!(res.share, stake_amount);

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
    let msg = ExecuteMsg::withdraw { amount: None };
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
    assert_eq!(res.share, Uint128::zero());

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
        msg: to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap(),
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
    assert_eq!(res.share, stake_amount_2);

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
            execute_msgs: vec![],
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
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

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
        Some(total_amount.checked_sub(deposit).unwrap())
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
    // start poll3
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
            execute_msgs: vec![],
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

    // vote poll3
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // vote poll3 as no
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::no,
        amount: stake_amount_2,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // end poll success
    let info = mock_info(TEST_CREATOR, &[]);
    env.block.height += DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 3 };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {  };
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.poll_deposit, Uint128::zero());

    // get poll
    let msg = QueryMsg::poll { poll_id: 3 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some(total_amount.checked_sub(deposit).unwrap())
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
        poll_id: 4,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // vote 2
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // end poll success
    let info = mock_info(TEST_CREATOR, &[]);
    env.block.height += DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 4 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let total_amount = total_amount.checked_sub(deposit).unwrap();
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // expired failed (wait period)
    env.block.height += DEFAULT_EFFECTIVE_DELAY;
    let msg = ExecuteMsg::poll_expire { poll_id: 4 };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    // execute success
    env.block.height += DEFAULT_EFFECTIVE_DELAY;
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 4 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::expired);

    // expired failed (status)
    let msg = ExecuteMsg::poll_expire { poll_id: 4 };
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
) {
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
            weight: 4
        },
        res.vaults[1]
    );
    assert_eq!(
        VaultInfo {
            address: TEST_VAULT.to_string(),
            weight: 5
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
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let info = mock_info(VOTING_TOKEN, &[]);
    let height = 3u64;
    env.block.height = DEFAULT_MINT_START + height;

    let reward = Uint128::from(300u128);

    // mint first
    let mint = Uint128::from(150u128);
    let msg = ExecuteMsg::mint {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());
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

    let new_amount = total_amount + reward;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &new_amount)],
    )]);
    let vault_amount = vault_amount + reward.multiply_ratio(vault_amount, total_amount);

    let msg = QueryMsg::balance {
        address: TEST_VAULT.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

    let total_amount = new_amount;
    deps.querier.with_token_balances(&[(
        &VOTING_TOKEN.to_string(),
        &[(&MOCK_CONTRACT_ADDR.to_string(), &total_amount)],
    )]);

    // get balance with height (with exceed mint_end)
    env.block.height += 3u64;
    let mint = Uint128::from(DEFAULT_MINT_PER_BLOCK * 2u128); // mint only 2 blocks because of mint_end
    let warchest_amount = mint * Decimal::percent(DEFAULT_WARCHEST_RATIO);
    let add_vault_amount = mint.checked_sub(warchest_amount).unwrap();
    let vault_amount = vault_amount + add_vault_amount.multiply_ratio(5u32, 9u32);
    let msg = QueryMsg::balance {
        address: TEST_VAULT.to_string(),
    };
    let res: BalanceResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

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
    let msg = ExecuteMsg::withdraw { amount: None };
    let res = execute(deps.as_mut(), env, info, msg);
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
    )
}

#[test]
fn test_mintable() {
    // First mint
    let mut state = State {
        contract_addr: CanonicalAddr::from(vec![]),
        poll_count: 0,
        total_share: Uint128::zero(),
        poll_deposit: Uint128::zero(),
        last_mint: 0,
        total_weight: 0,
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
