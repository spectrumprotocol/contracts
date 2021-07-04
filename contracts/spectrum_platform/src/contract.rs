use cosmwasm_std::{
    log, to_binary, Api, Binary, Decimal, Env, Extern, HandleResponse, HandleResult, HumanAddr,
    InitResponse, MigrateResponse, MigrateResult, Querier, StdError, StdResult, Storage,
};
use spectrum_protocol::platform::{
    BoardInfo, BoardsResponse, ConfigInfo, HandleMsg, MigrateMsg, QueryMsg, StateInfo,
};

use crate::poll::{
    poll_end, poll_execute, poll_expire, poll_start, poll_vote, query_poll, query_polls,
    query_voters,
};
use crate::state::{
    board_store, config_store, read_board, read_boards, read_config, read_state, state_store,
    Config, State,
};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: ConfigInfo,
) -> StdResult<InitResponse> {
    validate_quorum(msg.quorum)?;
    validate_threshold(msg.threshold)?;

    let config = Config {
        owner: deps.api.canonical_address(&msg.owner)?,
        quorum: msg.quorum,
        threshold: msg.threshold,
        voting_period: msg.voting_period,
        effective_delay: msg.effective_delay,
        expiration_period: msg.expiration_period,
    };

    let state = State {
        contract_addr: deps.api.canonical_address(&env.contract.address)?,
        poll_count: 0u64,
        total_weight: 0u32,
    };

    config_store(&mut deps.storage).save(&config)?;
    state_store(&mut deps.storage).save(&state)?;

    Ok(InitResponse::default())
}

/// validate_quorum returns an error if the quorum is invalid
/// (we require 0-1)
fn validate_quorum(quorum: Decimal) -> StdResult<()> {
    if quorum > Decimal::one() {
        Err(StdError::generic_err("quorum must be 0 to 1"))
    } else {
        Ok(())
    }
}

/// validate_threshold returns an error if the threshold is invalid
/// (we require 0-1)
fn validate_threshold(threshold: Decimal) -> StdResult<()> {
    if threshold > Decimal::one() {
        Err(StdError::generic_err("threshold must be 0 to 1"))
    } else {
        Ok(())
    }
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::poll_end { poll_id } => poll_end(deps, env, poll_id),
        HandleMsg::poll_execute { poll_id } => poll_execute(deps, env, poll_id),
        HandleMsg::poll_expire { poll_id } => poll_expire(deps, env, poll_id),
        HandleMsg::poll_start {
            title,
            description,
            link,
            execute_msgs,
        } => poll_start(deps, env, title, description, link, execute_msgs),
        HandleMsg::poll_vote { poll_id, vote } => poll_vote(deps, env, poll_id, vote),
        HandleMsg::update_config {
            owner,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
        } => update_config(
            deps,
            env,
            owner,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
        ),
        HandleMsg::upsert_board { address, weight } => upsert_board(deps, env, address, weight),
    }
}

fn update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    quorum: Option<Decimal>,
    threshold: Option<Decimal>,
    voting_period: Option<u64>,
    effective_delay: Option<u64>,
    expiration_period: Option<u64>,
) -> HandleResult {
    let mut config = config_store(&mut deps.storage).load()?;
    if config.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    if let Some(owner) = owner {
        config.owner = deps.api.canonical_address(&owner)?;
    }

    if let Some(quorum) = quorum {
        validate_quorum(quorum)?;
        config.quorum = quorum;
    }

    if let Some(threshold) = threshold {
        validate_threshold(threshold)?;
        config.threshold = threshold;
    }

    if let Some(voting_period) = voting_period {
        config.voting_period = voting_period;
    }

    if let Some(effective_delay) = effective_delay {
        config.effective_delay = effective_delay;
    }

    if let Some(expiration_period) = expiration_period {
        config.expiration_period = expiration_period;
    }
    config_store(&mut deps.storage).save(&config)?;

    Ok(HandleResponse::default())
}

fn upsert_board<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    address: HumanAddr,
    weight: u32,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    if config.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    let address_raw = deps.api.canonical_address(&address)?;
    let key = address_raw.as_slice();
    let old_weight = read_board(&mut deps.storage, key);

    let mut state = state_store(&mut deps.storage).load()?;
    state.total_weight = state.total_weight + weight - old_weight;
    state_store(&mut deps.storage).save(&state)?;

    if weight == 0 {
        board_store(&mut deps.storage).remove(key);
    } else {
        board_store(&mut deps.storage).save(key, &weight)?;
    }

    Ok(HandleResponse {
        messages: vec![],
        data: None,
        log: vec![log("new_total_weight", state.total_weight.to_string())],
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::boards {} => to_binary(&query_boards(deps)?),
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::poll { poll_id } => to_binary(&query_poll(deps, poll_id)?),
        QueryMsg::polls {
            filter,
            start_after,
            limit,
            order_by,
        } => to_binary(&query_polls(deps, filter, start_after, limit, order_by)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
        QueryMsg::voters {
            poll_id,
            start_after,
            limit,
            order_by,
        } => to_binary(&query_voters(deps, poll_id, start_after, limit, order_by)?),
    }
}

fn query_boards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<BoardsResponse> {
    let boards = read_boards(&deps.storage)?;
    Ok(BoardsResponse {
        boards: boards
            .into_iter()
            .map(|(addr, weight)| BoardInfo {
                address: deps.api.human_address(&addr).unwrap(),
                weight,
            })
            .collect(),
    })
}

fn query_config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<ConfigInfo> {
    let config = read_config(&deps.storage)?;
    Ok(ConfigInfo {
        owner: deps.api.human_address(&config.owner)?,
        quorum: config.quorum,
        threshold: config.threshold,
        voting_period: config.voting_period,
        effective_delay: config.effective_delay,
        expiration_period: config.expiration_period,
    })
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateInfo> {
    let state = read_state(&deps.storage)?;
    Ok(StateInfo {
        poll_count: state.poll_count,
        total_weight: state.total_weight,
    })
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    Ok(MigrateResponse::default())
}
