use crate::contract::{handle, init, query};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR, mock_dependencies, MockQuerier};
use cosmwasm_std::{Binary, CosmosMsg, Decimal, Extern, String, Uint128, WasmMsg, from_binary, to_vec};
use cw20::{Cw20HandleMsg};
use spectrum_protocol::common::OrderBy;
use spectrum_protocol::platform::{BoardsResponse, ConfigInfo, ExecuteMsg, HandleMsg, PollInfo, PollStatus, PollsResponse, QueryMsg, StateInfo, VoteOption, VoterInfo, VotersResponse};

const VOTING_TOKEN: &str = "voting_token";
const TEST_CREATOR: &str = "creator";
const TEST_VOTER: &str = "voter1";
const TEST_VOTER_2: &str = "voter2";
const DEFAULT_QUORUM: u64 = 30u64;
const DEFAULT_THRESHOLD: u64 = 50u64;
const DEFAULT_VOTING_PERIOD: u64 = 10000u64;
const DEFAULT_EFFECTIVE_DELAY: u64 = 10000u64;
const DEFAULT_EXPIRATION_PERIOD: u64 = 20000u64;

#[test]
fn test() {
    let mut deps = mock_dependencies(20, &[]);

    let config = test_config(&mut deps);
    let (weight, weight_2, total_weight) = test_board(&mut deps);

    test_poll_executed(&mut deps, &config, weight, weight_2, total_weight);
    test_poll_low_quorum(&mut deps, total_weight);
    test_poll_low_threshold(&mut deps, total_weight);
    test_poll_expired(&mut deps);
}

fn test_config(deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>) -> ConfigInfo {
    // test init & read config & read state
    let env = mock_env(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: MOCK_CONTRACT_ADDR.to_string(),
        quorum: Decimal::percent(120u64),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: 0,
        effective_delay: 0,
        expiration_period: 0,
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
    let msg = QueryMsg::state { };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            poll_count: 0,
            total_weight: 0,
        }
    );

    // alter config, validate owner
    let msg = HandleMsg::update_config {
        owner: None,
        quorum: None,
        threshold: None,
        voting_period: Some(DEFAULT_VOTING_PERIOD),
        effective_delay: Some(DEFAULT_EFFECTIVE_DELAY),
        expiration_period: Some(DEFAULT_EXPIRATION_PERIOD),
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
    assert_eq!(res, config.clone());

    // alter config, validate value
    let msg = HandleMsg::update_config {
        owner: None,
        quorum: None,
        threshold: Some(Decimal::percent(120u64)),
        voting_period: None,
        effective_delay: None,
        expiration_period: None,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    config
}

fn test_board(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
) -> (u32, u32, u32) {
    // stake, error
    let env = mock_env(TEST_VOTER, &[]);
    let weight = 25u32;
    let total_weight = weight;
    let msg = HandleMsg::upsert_board {
        address: TEST_VOTER.to_string(),
        weight,
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_err());

    let env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, total_weight);

    // query boards
    let msg = QueryMsg::boards {};
    let res: BoardsResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.boards[0].weight, weight);

    // stake voter2, error (0)
    let weight_2 = 75u32;
    let total_weight = total_weight + weight_2;
    let msg = HandleMsg::upsert_board {
        address: TEST_VOTER_2.to_string(),
        weight: weight_2,
    };
    let res = handle(deps, env.clone(), msg.clone());
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, total_weight);

    // query boards
    let msg = QueryMsg::boards {};
    let res: BoardsResponse = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.boards[1].weight, weight);
    assert_eq!(res.boards[0].weight, weight_2);

    (weight, weight_2, total_weight)
}

fn test_poll_executed(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
    config: &ConfigInfo,
    weight: u32,
    weight_2: u32,
    total_weight: u32,
) {
    // start poll
    let env = mock_env(TEST_VOTER, &[]);
    let execute_msg = ExecuteMsg::execute {
        contract: VOTING_TOKEN.to_string(),
        msg: String::from_utf8(
            to_vec(&Cw20HandleMsg::Burn {
                amount: Uint128(123),
            })
                .unwrap(),
        )
            .unwrap(),
    };
    let msg = HandleMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![execute_msg.clone()],
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state { };
    let res: StateInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.poll_count, 1u64);

    let poll = PollInfo {
        id: 1u64,
        creator: TEST_VOTER.to_string(),
        status: PollStatus::in_progress,
        end_height: env.block.height + config.voting_period,
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![execute_msg.clone()],
        yes_votes: 0u32,
        no_votes: 0u32,
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
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // vote success
    let msg = HandleMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.yes_votes, weight);
    assert_eq!(res.no_votes, 0u32);

    // vote failed (duplicate)
    let msg = HandleMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
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
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

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
                    TEST_VOTER_2.to_string(),
                    VoterInfo {
                        vote: VoteOption::yes,
                        balance: weight_2,
                    }
                ),
                (
                    TEST_VOTER.to_string(),
                    VoterInfo {
                        vote: VoteOption::yes,
                        balance: weight,
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

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::passed);
    assert_eq!(res.total_balance_at_end_poll, Some(total_weight));

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
}

fn test_poll_low_quorum(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
    total_weight: u32,
) {
    // start poll2
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![],
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote poll2
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::yes,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote poll failed (expired)
    let mut env = mock_env(TEST_VOTER, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = HandleMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::no,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_err());

    // end poll success
    let msg = HandleMsg::poll_end { poll_id: 2 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 2 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some(total_weight)
    );
}

fn test_poll_low_threshold(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
    total_weight: u32,
) {
    // start poll3
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![],
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote poll3
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::yes,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote poll3 as no
    let env = mock_env(TEST_VOTER_2, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::no,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // end poll success
    let mut env = mock_env(TEST_CREATOR, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = HandleMsg::poll_end { poll_id: 3 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 3 };
    let res: PollInfo = from_binary(&query(deps, msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some(total_weight)
    );
}

fn test_poll_expired(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier>,
) {
    // start poll
    let env = mock_env(TEST_VOTER, &[]);
    let execute_msg = ExecuteMsg::execute {
        contract: VOTING_TOKEN.to_string(),
        msg: String::from_utf8(
            to_vec(&Cw20HandleMsg::Burn {
                amount: Uint128(123),
            })
                .unwrap(),
        )
            .unwrap(),
    };
    let msg = HandleMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![execute_msg.clone()],
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote success
    let env = mock_env(TEST_VOTER, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // vote 2
    let env = mock_env(TEST_VOTER_2, &[]);
    let msg = HandleMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
    };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

    // end poll success
    let mut env = mock_env(TEST_CREATOR, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = HandleMsg::poll_end { poll_id: 4 };
    let res = handle(deps, env.clone(), msg);
    assert!(res.is_ok());

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
}
