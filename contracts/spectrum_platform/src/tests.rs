use crate::contract::{execute, instantiate, query};
use cosmwasm_std::testing::{
    mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR,
};
use cosmwasm_std::{
    from_binary, to_vec, Binary, CosmosMsg, Decimal, DepsMut, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::platform::{
    BoardsResponse, ConfigInfo, ExecuteMsg, PollExecuteMsg, PollInfo, PollStatus, PollsResponse,
    QueryMsg, StateInfo, VoteOption, VoterInfo, VotersResponse,
};
use spectrum_protocol::common::OrderBy;

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
    let mut deps = mock_dependencies(&[]);

    let config = test_config(deps.as_mut());
    let (weight, weight_2, total_weight) = test_board(deps.as_mut());

    test_poll_executed(deps.as_mut(), &config, weight, weight_2, total_weight);
    test_poll_low_quorum(deps.as_mut(), total_weight);
    test_poll_low_threshold(deps.as_mut(), total_weight);
    test_poll_expired(deps.as_mut());
}

fn test_config(mut deps: DepsMut) -> ConfigInfo {
    // test instantiate & read config & read state
    let env = mock_env();
    let info = mock_info(TEST_CREATOR, &[]);
    let mut config = ConfigInfo {
        owner: MOCK_CONTRACT_ADDR.to_string(),
        quorum: Decimal::percent(120u64),
        threshold: Decimal::percent(DEFAULT_THRESHOLD),
        voting_period: 0,
        effective_delay: 0,
        expiration_period: 0,
    };

    // validate quorum
    let res = instantiate(deps.branch(), env.clone(), info.clone(), config.clone());
    assert!(res.is_err());

    // validate threshold
    config.quorum = Decimal::percent(DEFAULT_QUORUM);
    config.threshold = Decimal::percent(120u64);
    let res = instantiate(deps.branch(), env.clone(), info.clone(), config.clone());
    assert!(res.is_err());

    // success instantiate
    config.threshold = Decimal::percent(DEFAULT_THRESHOLD);
    let res = instantiate(deps.branch(), env.clone(), info.clone(), config.clone());
    assert!(res.is_ok());

    // read config
    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res, config.clone());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(
        res,
        StateInfo {
            poll_count: 0,
            total_weight: 0,
        }
    );

    // alter config, validate owner
    let msg = ExecuteMsg::update_config {
        owner: None,
        quorum: None,
        threshold: None,
        voting_period: Some(DEFAULT_VOTING_PERIOD),
        effective_delay: Some(DEFAULT_EFFECTIVE_DELAY),
        expiration_period: Some(DEFAULT_EXPIRATION_PERIOD),
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    // success
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = QueryMsg::config {};
    let res: ConfigInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    config.voting_period = DEFAULT_VOTING_PERIOD;
    config.effective_delay = DEFAULT_EFFECTIVE_DELAY;
    config.expiration_period = DEFAULT_EXPIRATION_PERIOD;
    assert_eq!(res, config.clone());

    // alter config, validate value
    let msg = ExecuteMsg::update_config {
        owner: None,
        quorum: None,
        threshold: Some(Decimal::percent(120u64)),
        voting_period: None,
        effective_delay: None,
        expiration_period: None,
    };
    let res = execute(deps.branch(), env.clone(), info, msg);
    assert!(res.is_err());

    config
}

fn test_board(mut deps: DepsMut) -> (u32, u32, u32) {
    // stake, error
    let env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let weight = 25u32;
    let total_weight = weight;
    let msg = ExecuteMsg::upsert_board {
        address: TEST_VOTER.to_string(),
        weight,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, total_weight);

    // query boards
    let msg = QueryMsg::boards {};
    let res: BoardsResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.boards[0].weight, weight);

    // stake voter2, error (0)
    let weight_2 = 75u32;
    let total_weight = total_weight + weight_2;
    let msg = ExecuteMsg::upsert_board {
        address: TEST_VOTER_2.to_string(),
        weight: weight_2,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.total_weight, total_weight);

    // query boards
    let msg = QueryMsg::boards {};
    let res: BoardsResponse =
        from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.boards[0].weight, weight);
    assert_eq!(res.boards[1].weight, weight_2);

    (weight, weight_2, total_weight)
}

fn test_poll_executed(
    mut deps: DepsMut,
    config: &ConfigInfo,
    weight: u32,
    weight_2: u32,
    total_weight: u32,
) {
    // start poll
    let mut env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
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
    let msg = ExecuteMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![execute_msg.clone()],
    };
    let res = execute(deps.branch(), env.clone(), info, msg);
    assert!(res.is_ok());

    // read state
    let msg = QueryMsg::state {};
    let res: StateInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
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
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // vote success
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.yes_votes, weight);
    assert_eq!(res.no_votes, 0u32);

    // vote failed (duplicate)
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // end poll failed (voting period not end)
    let msg = ExecuteMsg::poll_end { poll_id: 1 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // vote 2
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 1,
        vote: VoteOption::yes,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

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
                        balance: weight,
                    }
                ),
                (
                    TEST_VOTER_2.to_string(),
                    VoterInfo {
                        vote: VoteOption::yes,
                        balance: weight_2,
                    }
                ),
            ]
        }
    );

    // end poll success
    let info = mock_info(TEST_VOTER_2, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 1 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 1 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::passed);
    assert_eq!(res.total_balance_at_end_poll, Some(total_weight));

    // end poll failed (not in progress)
    let msg = ExecuteMsg::poll_end { poll_id: 1 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // execute failed (wait period)
    let msg = ExecuteMsg::poll_execute { poll_id: 1 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    // execute success
    env.block.height = env.block.height + DEFAULT_EFFECTIVE_DELAY;
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
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
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_err());
}

fn test_poll_low_quorum(
    mut deps: DepsMut,
    total_weight: u32,
) {
    // start poll2
    let mut env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![],
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // vote poll2
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::yes,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // vote poll failed (expired)
    let info = mock_info(TEST_VOTER, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_vote {
        poll_id: 2,
        vote: VoteOption::no,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // end poll success
    let msg = ExecuteMsg::poll_end { poll_id: 2 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 2 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some(total_weight)
    );
}

fn test_poll_low_threshold(
    mut deps: DepsMut,
    total_weight: u32,
) {
    // start poll3
    let mut env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![],
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // vote poll3
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::yes,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // vote poll3 as no
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 3,
        vote: VoteOption::no,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // end poll success
    let info = mock_info(TEST_CREATOR, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 3 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 3 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::rejected);
    assert_eq!(
        res.total_balance_at_end_poll,
        Some(total_weight)
    );
}

fn test_poll_expired(
    mut deps: DepsMut,
) {
    // start poll
    let mut env = mock_env();
    let info = mock_info(TEST_VOTER, &[]);
    let execute_msg = PollExecuteMsg::execute {
        contract: VOTING_TOKEN.to_string(),
        msg: String::from_utf8(
            to_vec(&Cw20ExecuteMsg::Burn {
                amount: Uint128::new(123),
            })
                .unwrap(),
        )
            .unwrap(),
    };
    let msg = ExecuteMsg::poll_start {
        title: "title".to_string(),
        description: "description".to_string(),
        link: None,
        execute_msgs: vec![execute_msg.clone()],
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // vote success
    let info = mock_info(TEST_VOTER, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // vote 2
    let info = mock_info(TEST_VOTER_2, &[]);
    let msg = ExecuteMsg::poll_vote {
        poll_id: 4,
        vote: VoteOption::yes,
    };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // end poll success
    let info = mock_info(TEST_CREATOR, &[]);
    env.block.height = env.block.height + DEFAULT_VOTING_PERIOD;
    let msg = ExecuteMsg::poll_end { poll_id: 4 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // expired failed (wait period)
    env.block.height = env.block.height + DEFAULT_EFFECTIVE_DELAY;
    let msg = ExecuteMsg::poll_expire { poll_id: 4 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_err());

    // execute success
    env.block.height = env.block.height + DEFAULT_EFFECTIVE_DELAY;
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // get poll
    let msg = QueryMsg::poll { poll_id: 4 };
    let res: PollInfo = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.status, PollStatus::expired);

    // expired failed (status)
    let msg = ExecuteMsg::poll_expire { poll_id: 4 };
    let res = execute(deps.branch(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    // query polls
    let msg = QueryMsg::polls {
        filter: Some(PollStatus::rejected),
        start_after: None,
        limit: None,
        order_by: Some(OrderBy::Asc),
    };
    let res: PollsResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
    assert_eq!(res.polls[0].id, 2);
    assert_eq!(res.polls[1].id, 3);
}
