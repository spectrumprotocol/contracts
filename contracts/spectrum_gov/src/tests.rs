use crate::contract::{handle, init, query};
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, to_vec, Binary, CosmosMsg, Decimal, Extern, HumanAddr, Uint128, WasmMsg,
};
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use spectrum_protocol::common::OrderBy;
use spectrum_protocol::gov::{
    BalanceResponse, ConfigInfo, Cw20HookMsg, ExecuteMsg, HandleMsg, PollInfo, PollStatus,
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
const DEFAULT_EFFECTIVE_DELAY: u64 = 10000u64;
const DEFAULT_EXPIRATION_PERIOD: u64 = 20000u64;
const DEFAULT_PROPOSAL_DEPOSIT: u128 = 100u128;
const DEFAULT_MINT_PER_BLOCK: u128 = 50u128;
const DEFAULT_WARCHEST_RATIO: u64 = 10u64;

#[test]
fn test() {
    let mut deps = mock_dependencies(20, &[]);

    let config = test_config(&mut deps);
    let stake = test_stake(&mut deps);

    let stake = test_poll_executed(&mut deps, &config, stake);
    let stake = test_poll_low_quorum(&mut deps, stake);
    let stake = test_poll_low_threshold(&mut deps, stake);
    let stake = test_poll_expired(&mut deps, stake);

    test_reward(&mut deps, stake);
}

fn test_config(deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: HumanAddr::from(MOCK_CONTRACT_ADDR),
        spec_token: Some(HumanAddr::from(VOTING_TOKEN)),
        quorum: Decimal::percent(120u64),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: 0,
        effective_delay: 0,
        expiration_period: 0,
        proposal_deposit: Uint128::zero(),
        mint_per_block: Uint128::zero(),
        mint_start: 0,
        mint_end: 0,
        warchest_address: None,
        warchest_ratio: Decimal::zero(),
    };

    // validate quorum
    let res = init(deps, env.clone(), config.clone());
    assert!(res.is_err());

    // validate threshold
    config.quorum = Decimal::percent(DEFAULT_QUORUM);
    config.threshold = Decimal::percent(120u64);
    let res = init(deps, env.clone(), config.clone());
    assert!(res.is_err());

    // success init
    config.threshold = Decimal::percent(DEFAULT_THRESHOLD);
    let res = init(deps, env.clone(), config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res, config.clone());

    // read state
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            poll_count: 0,
            poll_deposit: Uint128::zero(),
            total_share: Uint128::zero(),
            last_mint: 0,
            total_weight: 0,
            total_staked: Uint128::zero(),
        }
    );

    // alter config, validate owner
    let msg = HandleMsg::update_config {
        owner: None,
        spec_token: None,
        quorum: None,
        threshold: None,
        voting_period: Some(DEFAULT_VOTING_PERIOD),
        effective_delay: Some(DEFAULT_EFFECTIVE_DELAY),
        expiration_period: Some(DEFAULT_EXPIRATION_PERIOD),
        proposal_deposit: Some(Uint128::from(DEFAULT_PROPOSAL_DEPOSIT)),
        mint_per_block: None,
        mint_start: None,
        mint_end: None,
        warchest_address: None,
        warchest_ratio: None,
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // success
    let env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    config.voting_period = DEFAULT_VOTING_PERIOD;
    config.effective_delay = DEFAULT_EFFECTIVE_DELAY;
    config.expiration_period = DEFAULT_EXPIRATION_PERIOD;
    config.proposal_deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    assert_eq!(res, config.clone());

    // alter config, validate value
    let msg = HandleMsg::update_config {
        owner: None,
        spec_token: None,
        quorum: None,
        threshold: Some(Decimal::percent(120u64)),
        voting_period: None,
        effective_delay: None,
        expiration_period: None,
        proposal_deposit: None,
        mint_per_block: None,
        mint_start: None,
        mint_end: None,
        warchest_address: None,
        warchest_ratio: None,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    config
}

fn test_stake(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
) -> (Uint128, Uint128, Uint128) {
    // stake, error
    let env = mock_env(TEST_VOTER, &[]);
    let stake_amount = Uint128::from(25u128);
    let total_amount = stake_amount;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: stake_amount,
        msg: Some(to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap()),
    });
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    let env = mock_env(VOTING_TOKEN, &[]);
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.total_share, total_amount);

    // query account
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);
    assert_eq!(res.share, stake_amount);

    // query account not found
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, Uint128::zero());
    assert_eq!(res.share, Uint128::zero());

    // stake voter2, error (0)
    let stake_amount_2 = Uint128::from(75u128);
    let total_amount = total_amount + stake_amount_2;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_2),
        amount: Uint128::zero(),
        msg: Some(to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap()),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // withdraw failed (0)
    let env = mock_env(TEST_VOTER_2, &[]);
    let msg = HandleMsg::withdraw {
        amount: Some(stake_amount_2),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // stake voter2
    let env = mock_env(VOTING_TOKEN, &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_2),
        amount: stake_amount_2,
        msg: Some(to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap()),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.total_share, total_amount);

    // query account
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);
    assert_eq!(res.share, stake_amount);

    // query account 2
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);
    assert_eq!(res.share, stake_amount_2);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_executed(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    config: &ConfigInfo,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {
    // start poll
    let env = mock_env(VOTING_TOKEN, &[]);
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let execute_msg = ExecuteMsg::execute {
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: String::from_utf8(
            to_vec(&Cw20HandleMsg::Burn {
                amount: Uint128(123),
            })
            .unwrap(),
        )
        .unwrap(),
    };
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: deposit,
        msg: Some(
            to_binary(&Cw20HookMsg::poll_start {
                title: "title".to_string(),
                description: "description".to_string(),
                link: None,
                execute_msgs: vec![execute_msg.clone()],
            })
            .unwrap(),
        ),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    let total_amount = total_amount + deposit;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // read state
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.poll_count, 1u64);
    assert_eq!(res.poll_deposit, deposit);

    let poll = PollInfo {
        id: 1u64,
        creator: HumanAddr::from(TEST_VOTER),
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
    let res: PollsResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        PollsResponse {
            polls: vec![poll.clone()]
        }
    );

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res, poll);

    // vote failed (id not found)
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // vote failed (not enough amount)
    let msg = HandleMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount + Uint128::from(1u128),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // vote success
    let msg = HandleMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.yes_votes, stake_amount);
    assert_eq!(res.no_votes, Uint128::zero());

    // query account
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
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
    let msg = HandleMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());
    // end poll failed (voting period not end)
    let msg = HandleMsg::poll_end { poll_id: 1 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // vote 2
    let env = mock_env(TEST_VOTER_2, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // withdraw all failed
    let msg = HandleMsg::withdraw {
        amount: Some(stake_amount_2),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // withdraw non-vote is ok
    let withdraw_amount = (stake_amount_2 - stake_amount).unwrap();
    let msg = HandleMsg::withdraw {
        amount: Some(withdraw_amount),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    let total_amount = (total_amount - withdraw_amount).unwrap();
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // query account 2
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);
    assert_eq!(res.share, stake_amount);

    // query voters
    let msg = QueryMsg::voters {
        poll_id: 1,
        start_after: None,
        limit: None,
        order_by: None,
    };
    let res: VotersResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        VotersResponse {
            voters: vec![
                (
                    HumanAddr::from(TEST_VOTER_2),
                    VoterInfo {
                        vote: VoteOption::yes,
                        balance: stake_amount,
                    }
                ),
                (
                    HumanAddr::from(TEST_VOTER),
                    VoterInfo {
                        vote: VoteOption::yes,
                        balance: stake_amount,
                    }
                ),
            ]
        }
    );

    // end poll success
    let mut env = mock_env(TEST_VOTER_2, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = HandleMsg::poll_end { poll_id: 1 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(&poll.creator),
                amount: poll.deposit_amount,
            })
            .unwrap(),
        })
    );

    // read state
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.poll_deposit, Uint128::zero());

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    let total_amount = (total_amount - deposit).unwrap();
    assert_eq!(res.status, PollStatus::passed);
    assert_eq!(res.total_balance_at_end_poll, Some(total_amount));
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // withdraw all success after end poll
    let msg = HandleMsg::withdraw { amount: None };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    let total_amount = (total_amount - stake_amount).unwrap();
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // query account 2
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, Uint128::zero());
    assert_eq!(res.share, Uint128::zero());

    // stake voter2
    let total_amount = total_amount + stake_amount_2;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);
    env.message.sender = HumanAddr::from(VOTING_TOKEN);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER_2),
        amount: stake_amount_2,
        msg: Some(to_binary(&Cw20HookMsg::stake_tokens { staker_addr: None }).unwrap()),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // query account 2
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);
    assert_eq!(res.share, stake_amount_2);

    // end poll failed (not in progress)
    let msg = HandleMsg::poll_end { poll_id: 1 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // execute failed (wait period)
    let msg = HandleMsg::poll_execute { poll_id: 1 };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // execute success
    env.block.height = env.block.height + DEFAULT_EFFECTIVE_DELAY;
    let res = handle(deps, env.clone(), msg);
    let (contract_addr, msg) = match execute_msg {
        ExecuteMsg::execute { contract, msg } => (contract, msg),
    };
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg: Binary(msg.into_bytes()),
            send: vec![],
        })
    );

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::executed);

    // execute failed (status)
    let msg = HandleMsg::poll_execute { poll_id: 1 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_low_quorum(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {
    // start poll2
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let env = mock_env(VOTING_TOKEN, &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: deposit,
        msg: Some(
            to_binary(&Cw20HookMsg::poll_start {
                title: "title".to_string(),
                description: "description".to_string(),
                link: None,
                execute_msgs: vec![],
            })
            .unwrap(),
        ),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    let total_amount = total_amount + deposit;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // vote poll2
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote poll failed (expired)
    let mut env = mock_env(TEST_CREATOR, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = HandleMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::no,
        amount: stake_amount_2,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // end poll success
    let msg = HandleMsg::poll_end { poll_id: 2 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.poll_deposit, Uint128::zero());

    // get poll
    let msg = QueryMsg::poll { poll_id: 2 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some((total_amount - deposit).unwrap())
    );

    // query account
    let total_shares = stake_amount + stake_amount_2;
    let stake_amount = stake_amount.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    // query account 2
    let stake_amount_2 = stake_amount_2.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_low_threshold(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {
    // start poll3
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let env = mock_env(VOTING_TOKEN, &[]);
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: deposit,
        msg: Some(
            to_binary(&Cw20HookMsg::poll_start {
                title: "title".to_string(),
                description: "description".to_string(),
                link: None,
                execute_msgs: vec![],
            })
            .unwrap(),
        ),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    let total_amount = total_amount + deposit;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // vote poll3
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote poll3 as no
    let env = mock_env(TEST_VOTER_2, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::no,
        amount: stake_amount_2,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // end poll success
    let mut env = mock_env(TEST_CREATOR, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = HandleMsg::poll_end { poll_id: 3 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.poll_deposit, Uint128::zero());

    // get poll
    let msg = QueryMsg::poll { poll_id: 3 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some((total_amount - deposit).unwrap())
    );

    // query account
    let total_shares = stake_amount + stake_amount_2;
    let stake_amount = stake_amount.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    // query account 2
    let stake_amount_2 = stake_amount_2.multiply_ratio(total_amount, total_shares);
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_poll_expired(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> (Uint128, Uint128, Uint128) {
    // start poll
    let env = mock_env(VOTING_TOKEN, &[]);
    let deposit = Uint128::from(DEFAULT_PROPOSAL_DEPOSIT);
    let execute_msg = ExecuteMsg::execute {
        contract: HumanAddr::from(VOTING_TOKEN),
        msg: String::from_utf8(
            to_vec(&Cw20HandleMsg::Burn {
                amount: Uint128(123),
            })
            .unwrap(),
        )
        .unwrap(),
    };
    let msg = HandleMsg::receive(Cw20ReceiveMsg {
        sender: HumanAddr::from(TEST_VOTER),
        amount: deposit,
        msg: Some(
            to_binary(&Cw20HookMsg::poll_start {
                title: "title".to_string(),
                description: "description".to_string(),
                link: None,
                execute_msgs: vec![execute_msg.clone()],
            })
            .unwrap(),
        ),
    });
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    let total_amount = total_amount + deposit;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // vote success
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote 2
    let env = mock_env(TEST_VOTER_2, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
        amount: stake_amount,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // end poll success
    let mut env = mock_env(TEST_CREATOR, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = HandleMsg::poll_end { poll_id: 4 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let total_amount = (total_amount - deposit).unwrap();
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // expired failed (wait period)
    env.block.height = env.block.height + DEFAULT_EFFECTIVE_DELAY;
    let msg = HandleMsg::poll_expire { poll_id: 4 };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    // execute success
    env.block.height = env.block.height + DEFAULT_EFFECTIVE_DELAY;
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 4 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::expired);

    // expired failed (status)
    let msg = HandleMsg::poll_expire { poll_id: 4 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // query polls
    let msg = QueryMsg::polls {
        filter: Some(PollStatus::rejected),
        start_after: None,
        limit: None,
        order_by: Some(OrderBy::Asc),
    };
    let res: PollsResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.polls[0].id, 2);
    assert_eq!(res.polls[1].id, 3);

    (stake_amount, stake_amount_2, total_amount)
}

fn test_reward(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    (stake_amount, stake_amount_2, total_amount): (Uint128, Uint128, Uint128),
) -> () {
    let env = mock_env(MOCK_CONTRACT_ADDR, &[]);

    // add vault 1
    let msg = HandleMsg::upsert_vault {
        vault_address: HumanAddr::from(TEST_VAULT),
        weight: 1,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // add vault 2
    let msg = HandleMsg::upsert_vault {
        vault_address: HumanAddr::from(TEST_VAULT_2),
        weight: 4,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // modify vault 1
    let msg = HandleMsg::upsert_vault {
        vault_address: HumanAddr::from(TEST_VAULT),
        weight: 5,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // validate weight
    let msg = QueryMsg::state { height: 0u64 };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(9, res.total_weight);

    // validate vaults
    let msg = QueryMsg::vaults {};
    let res: VaultsResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        VaultInfo {
            address: HumanAddr::from(TEST_VAULT_2),
            weight: 4
        },
        res.vaults[0]
    );
    assert_eq!(
        VaultInfo {
            address: HumanAddr::from(TEST_VAULT),
            weight: 5
        },
        res.vaults[1]
    );

    let msg = HandleMsg::update_config {
        owner: None,
        spec_token: None,
        quorum: None,
        threshold: None,
        voting_period: None,
        effective_delay: None,
        expiration_period: None,
        proposal_deposit: None,
        mint_per_block: Some(Uint128::from(DEFAULT_MINT_PER_BLOCK)),
        mint_start: Some(env.block.height),
        mint_end: Some(env.block.height + 5),
        warchest_address: Some(HumanAddr::from(WARCHEST)),
        warchest_ratio: Some(Decimal::percent(DEFAULT_WARCHEST_RATIO)),
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let mut env = mock_env(VOTING_TOKEN, &[]);
    let height = 3u64;
    env.block.height = env.block.height + height;

    let reward = Uint128::from(300u128);

    // mint first
    let mint = Uint128::from(150u128);
    let msg = HandleMsg::mint {};
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Mint {
                amount: mint,
                recipient: HumanAddr::from(MOCK_CONTRACT_ADDR),
            })
            .unwrap(),
            send: vec![],
        })]
    );

    let total_amount = total_amount + mint;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    let warchest_amount = Uint128::from(15u128);
    let vault_amount = Uint128::from(75u128);
    let vault_amount_2 = Uint128::from(60u128);

    // check balance all users
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount);

    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VOTER_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, stake_amount_2);

    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VAULT),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VAULT_2),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount_2);

    let msg = QueryMsg::balance {
        address: HumanAddr::from(WARCHEST),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, warchest_amount);

    let new_amount = total_amount + reward;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &new_amount)],
    )]);
    let vault_amount = vault_amount + reward.multiply_ratio(vault_amount, total_amount);

    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VAULT),
        height: None,
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

    let total_amount = new_amount;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // get balance with height (with exceed mint_end)
    env.block.height = env.block.height + 3u64;
    let mint = Uint128::from(DEFAULT_MINT_PER_BLOCK * 2u128); // mint only 2 blocks because of mint_end
    let warchest_amount = mint * Decimal::percent(DEFAULT_WARCHEST_RATIO);
    let add_vault_amount = (mint - warchest_amount).unwrap();
    let vault_amount = vault_amount + add_vault_amount.multiply_ratio(5u32, 9u32);
    let msg = QueryMsg::balance {
        address: HumanAddr::from(TEST_VAULT),
        height: Some(env.block.height),
    };
    let res: BalanceResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.balance, vault_amount);

    // mint again
    let msg = HandleMsg::mint {};
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    let total_amount = total_amount + mint;
    deps.querier.with_token_balances(&[(
        &HumanAddr::from(VOTING_TOKEN),
        &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &total_amount)],
    )]);

    // withdraw all
    let env = mock_env(TEST_VAULT, &[]);
    let msg = HandleMsg::withdraw { amount: None };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());
    assert_eq!(
        res.unwrap().messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(VOTING_TOKEN),
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: HumanAddr::from(TEST_VAULT),
                amount: Uint128::from(vault_amount),
            })
            .unwrap(),
            send: vec![],
        })
    )
}
