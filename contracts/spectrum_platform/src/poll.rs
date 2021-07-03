use cosmwasm_std::{
    log, Api, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse, HandleResult, HumanAddr,
    Querier, StdError, StdResult, Storage, WasmMsg,
};
use spectrum_protocol::common::OrderBy;
use spectrum_protocol::platform::{
    ExecuteMsg, PollInfo, PollStatus, PollsResponse, VoteOption, VoterInfo, VotersResponse,
};

use crate::state::{
    poll_indexer_store, poll_store, poll_voter_store, read_board, read_config, read_poll,
    read_poll_voter, read_poll_voters, read_polls, read_state, state_store, Poll,
};

/// create a new poll
pub fn poll_start<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    title: String,
    description: String,
    link: Option<String>,
    execute_msgs: Vec<ExecuteMsg>,
) -> StdResult<HandleResponse> {
    validate_title(&title)?;
    validate_description(&description)?;
    validate_link(&link)?;

    let sender_address_raw = deps.api.canonical_address(&env.message.sender)?;
    let key = sender_address_raw.as_slice();
    let weight = read_board(&deps.storage, key);
    if weight == 0 {
        return Err(StdError::unauthorized());
    }

    let config = read_config(&deps.storage)?;

    let mut state = state_store(&mut deps.storage).load()?;
    let poll_id = state.poll_count + 1;

    // Increase poll count & total deposit amount
    state.poll_count += 1;

    let new_poll = Poll {
        id: poll_id,
        creator: deps.api.canonical_address(&env.message.sender)?,
        status: PollStatus::in_progress,
        yes_votes: 0u32,
        no_votes: 0u32,
        end_height: env.block.height + config.voting_period,
        title,
        description,
        link,
        execute_msgs: execute_msgs,
        total_balance_at_end_poll: None,
    };

    poll_store(&mut deps.storage).save(&poll_id.to_be_bytes(), &new_poll)?;
    poll_indexer_store(&mut deps.storage, &PollStatus::in_progress)
        .save(&poll_id.to_be_bytes(), &true)?;

    state_store(&mut deps.storage).save(&state)?;

    let r = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "create_poll"),
            log(
                "creator",
                deps.api.human_address(&new_poll.creator)?.as_str(),
            ),
            log("poll_id", &poll_id.to_string()),
            log("end_height", new_poll.end_height),
        ],
        data: None,
    };
    Ok(r)
}

const MIN_TITLE_LENGTH: usize = 4;
const MAX_TITLE_LENGTH: usize = 64;
const MIN_DESC_LENGTH: usize = 4;
const MAX_DESC_LENGTH: usize = 256;
const MIN_LINK_LENGTH: usize = 12;
const MAX_LINK_LENGTH: usize = 128;

/// validate_title returns an error if the title is invalid
fn validate_title(title: &str) -> StdResult<()> {
    if title.len() < MIN_TITLE_LENGTH {
        Err(StdError::generic_err("Title too short"))
    } else if title.len() > MAX_TITLE_LENGTH {
        Err(StdError::generic_err("Title too long"))
    } else {
        Ok(())
    }
}

/// validate_description returns an error if the description is invalid
fn validate_description(description: &str) -> StdResult<()> {
    if description.len() < MIN_DESC_LENGTH {
        Err(StdError::generic_err("Description too short"))
    } else if description.len() > MAX_DESC_LENGTH {
        Err(StdError::generic_err("Description too long"))
    } else {
        Ok(())
    }
}

/// validate_link returns an error if the link is invalid
fn validate_link(link: &Option<String>) -> StdResult<()> {
    if let Some(link) = link {
        if link.len() < MIN_LINK_LENGTH {
            Err(StdError::generic_err("Link too short"))
        } else if link.len() > MAX_LINK_LENGTH {
            Err(StdError::generic_err("Link too long"))
        } else {
            Ok(())
        }
    } else {
        Ok(())
    }
}

pub fn poll_vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    poll_id: u64,
    vote: VoteOption,
) -> HandleResult {
    let sender_address_raw = deps.api.canonical_address(&env.message.sender)?;
    let state = read_state(&deps.storage)?;
    if poll_id == 0 || state.poll_count < poll_id {
        return Err(StdError::generic_err("Poll does not exist"));
    }

    let mut a_poll = poll_store(&mut deps.storage).load(&poll_id.to_be_bytes())?;
    if a_poll.status != PollStatus::in_progress || env.block.height > a_poll.end_height {
        return Err(StdError::generic_err("Poll is not in progress"));
    }

    // Check the voter already has a vote on the poll
    if read_poll_voter(&deps.storage, poll_id, &sender_address_raw).is_ok() {
        return Err(StdError::generic_err("User has already voted."));
    }

    let key = sender_address_raw.as_slice();
    let weight = read_board(&deps.storage, key);
    if weight == 0 {
        return Err(StdError::unauthorized());
    }

    // update tally info
    if VoteOption::yes == vote {
        a_poll.yes_votes += weight;
    } else {
        a_poll.no_votes += weight;
    }

    let vote_info = VoterInfo {
        vote,
        balance: weight,
    };

    // store poll voter && and update poll data
    poll_voter_store(&mut deps.storage, poll_id).save(sender_address_raw.as_slice(), &vote_info)?;
    poll_store(&mut deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;

    let log = vec![
        log("action", "cast_vote"),
        log("poll_id", &poll_id.to_string()),
        log("amount", &weight.to_string()),
        log("voter", &env.message.sender.as_str()),
        log("vote_option", vote_info.vote),
    ];

    let r = HandleResponse {
        messages: vec![],
        log,
        data: None,
    };
    Ok(r)
}

/*
 * Ends a poll.
 */
pub fn poll_end<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    poll_id: u64,
) -> HandleResult {
    let mut a_poll = poll_store(&mut deps.storage).load(&poll_id.to_be_bytes())?;

    if a_poll.status != PollStatus::in_progress {
        return Err(StdError::generic_err("Poll is not in progress"));
    }

    let no = a_poll.no_votes;
    let yes = a_poll.yes_votes;

    let all_votes = yes + no;

    let config = read_config(&deps.storage)?;
    let state = read_state(&deps.storage)?;

    if a_poll.end_height > env.block.height
        && Decimal::from_ratio(yes, state.total_weight) < config.threshold
        && Decimal::from_ratio(no, state.total_weight) < config.threshold
    {
        return Err(StdError::generic_err("Voting period has not expired"));
    }

    let quorum = Decimal::from_ratio(all_votes, state.total_weight);
    let (passed, rejected_reason) = if quorum.is_zero() || quorum < config.quorum {
        // Quorum: More than quorum of the total staked tokens at the end of the voting
        // period need to have participated in the vote.
        (false, "Quorum not reached")
    } else if Decimal::from_ratio(yes, all_votes) < config.threshold {
        (false, "Threshold not reached")
    } else {
        //Threshold: More than 50% of the tokens that participated in the vote
        // (after excluding “Abstain” votes) need to have voted in favor of the proposal (“Yes”).
        (true, "")
    };

    // Update poll status
    a_poll.status = if passed {
        PollStatus::passed
    } else {
        PollStatus::rejected
    };
    a_poll.total_balance_at_end_poll = Some(state.total_weight);
    if env.block.height < a_poll.end_height {
        a_poll.end_height = env.block.height;
    }
    poll_store(&mut deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;

    // Update poll indexer
    poll_indexer_store(&mut deps.storage, &PollStatus::in_progress)
        .remove(&a_poll.id.to_be_bytes());
    poll_indexer_store(&mut deps.storage, &a_poll.status).save(&a_poll.id.to_be_bytes(), &true)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "end_poll"),
            log("poll_id", &poll_id.to_string()),
            log("rejected_reason", rejected_reason),
            log("passed", &passed.to_string()),
        ],
        data: None,
    })
}

/*
 * Execute a msg of passed poll.
 */
pub fn poll_execute<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    poll_id: u64,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let mut a_poll = poll_store(&mut deps.storage).load(&poll_id.to_be_bytes())?;

    if a_poll.status != PollStatus::passed {
        return Err(StdError::generic_err("Poll is not in passed status"));
    }

    if a_poll.end_height + config.effective_delay > env.block.height {
        return Err(StdError::generic_err("Effective delay has not expired"));
    }

    if a_poll.execute_msgs.len() == 0 {
        return Err(StdError::generic_err("The poll does not have execute_data"));
    }

    poll_indexer_store(&mut deps.storage, &PollStatus::passed).remove(&poll_id.to_be_bytes());
    poll_indexer_store(&mut deps.storage, &PollStatus::executed)
        .save(&poll_id.to_be_bytes(), &true)?;

    a_poll.status = PollStatus::executed;
    poll_store(&mut deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;

    Ok(HandleResponse {
        messages: a_poll.execute_msgs.into_iter().map(match_msg).collect(),
        log: vec![
            log("action", "execute_poll"),
            log("poll_id", poll_id.to_string()),
        ],
        data: None,
    })
}

fn match_msg(msg: ExecuteMsg) -> CosmosMsg {
    match msg {
        ExecuteMsg::execute { contract, msg } => CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract,
            msg: Binary(msg.into_bytes()),
            send: vec![],
        }),
    }
}

/// ExpirePoll is used to make the poll as expired state for querying purpose
pub fn poll_expire<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    poll_id: u64,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let mut a_poll = poll_store(&mut deps.storage).load(&poll_id.to_be_bytes())?;

    if a_poll.status != PollStatus::passed {
        return Err(StdError::generic_err("Poll is not in passed status"));
    }

    if a_poll.execute_msgs.len() == 0 {
        return Err(StdError::generic_err(
            "Cannot make a text proposal to expired state",
        ));
    }

    if a_poll.end_height + config.expiration_period > env.block.height {
        return Err(StdError::generic_err("Expire height has not been reached"));
    }

    poll_indexer_store(&mut deps.storage, &PollStatus::passed).remove(&poll_id.to_be_bytes());
    poll_indexer_store(&mut deps.storage, &PollStatus::expired)
        .save(&poll_id.to_be_bytes(), &true)?;

    a_poll.status = PollStatus::expired;
    poll_store(&mut deps.storage).save(&poll_id.to_be_bytes(), &a_poll)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "expire_poll"),
            log("poll_id", poll_id.to_string()),
        ],
        data: None,
    })
}

fn map_poll<A: Api>(poll: Poll, api: &A) -> StdResult<PollInfo> {
    Ok(PollInfo {
        id: poll.id,
        creator: api.human_address(&poll.creator).unwrap(),
        status: poll.status.clone(),
        end_height: poll.end_height,
        title: poll.title,
        description: poll.description,
        link: poll.link,
        execute_msgs: poll.execute_msgs,
        yes_votes: poll.yes_votes,
        no_votes: poll.no_votes,
        total_balance_at_end_poll: poll.total_balance_at_end_poll,
    })
}

pub fn query_poll<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    poll_id: u64,
) -> StdResult<PollInfo> {
    let poll = read_poll(&deps.storage, &poll_id.to_be_bytes())?;
    if poll.is_none() {
        return Err(StdError::generic_err("Poll does not exist"));
    }
    map_poll(poll.unwrap(), &deps.api)
}

pub fn query_polls<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    filter: Option<PollStatus>,
    start_after: Option<u64>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<PollsResponse> {
    let polls = read_polls(&deps.storage, filter, start_after, limit, order_by)?;
    let poll_responses: StdResult<Vec<PollInfo>> = polls
        .into_iter()
        .map(|poll| map_poll(poll, &deps.api))
        .collect();

    Ok(PollsResponse {
        polls: poll_responses?,
    })
}

pub fn query_voters<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    poll_id: u64,
    start_after: Option<HumanAddr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VotersResponse> {
    let poll = match read_poll(&deps.storage, &poll_id.to_be_bytes())? {
        Some(poll) => Some(poll),
        None => return Err(StdError::generic_err("Poll does not exist")),
    }
    .unwrap();

    let voters = if poll.status != PollStatus::in_progress {
        vec![]
    } else {
        read_poll_voters(
            &deps.storage,
            poll_id,
            match start_after {
                Some(sa) => Some(deps.api.canonical_address(&sa)?),
                None => None,
            },
            limit,
            order_by,
        )?
    };

    let voters_response: StdResult<Vec<(HumanAddr, VoterInfo)>> = voters
        .into_iter()
        .map(|voter_info| {
            Ok((
                deps.api.human_address(&voter_info.0)?,
                VoterInfo {
                    vote: voter_info.1.vote,
                    balance: voter_info.1.balance,
                },
            ))
        })
        .collect();

    Ok(VotersResponse {
        voters: voters_response?,
    })
}
