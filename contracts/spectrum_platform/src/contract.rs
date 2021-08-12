#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Storage,
};
use spectrum_protocol::platform::{
    BoardInfo, BoardsResponse, ConfigInfo, ExecuteMsg, MigrateMsg, QueryMsg, StateInfo,
};

use crate::poll::{
    poll_end, poll_execute, poll_expire, poll_start, poll_vote, query_poll, query_polls,
    query_voters,
};
use crate::state::{
    board_store, config_store, read_board, read_boards, read_config, read_state, state_store,
    Config, State,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    validate_quorum(msg.quorum)?;
    validate_threshold(msg.threshold)?;

    let config = Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        quorum: msg.quorum,
        threshold: msg.threshold,
        voting_period: msg.voting_period,
        effective_delay: msg.effective_delay,
        expiration_period: msg.expiration_period,
    };

    let state = State {
        contract_addr: deps.api.addr_canonicalize(&env.contract.address.as_str())?,
        poll_count: 0u64,
        total_weight: 0u32,
    };

    config_store(deps.storage).save(&config)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::default())
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::poll_end { poll_id } => poll_end(deps, env, poll_id),
        ExecuteMsg::poll_execute { poll_id } => poll_execute(deps, env, poll_id),
        ExecuteMsg::poll_expire { poll_id } => poll_expire(deps, env, poll_id),
        ExecuteMsg::poll_start {
            title,
            description,
            link,
            execute_msgs,
        } => poll_start(deps, env, info, title, description, link, execute_msgs),
        ExecuteMsg::poll_vote { poll_id, vote } => poll_vote(deps, env, info, poll_id, vote),
        ExecuteMsg::update_config {
            owner,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
        } => update_config(
            deps,
            env,
            info,
            owner,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
        ),
        ExecuteMsg::upsert_board { address, weight } => {
            upsert_board(deps, env, info, address, weight)
        }
    }
}

fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: Option<String>,
    quorum: Option<Decimal>,
    threshold: Option<Decimal>,
    voting_period: Option<u64>,
    effective_delay: Option<u64>,
    expiration_period: Option<u64>,
) -> StdResult<Response> {
    let mut config = config_store(deps.storage).load()?;
    if config.owner != deps.api.addr_canonicalize(&info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(&owner)?;
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
    config_store(deps.storage).save(&config)?;

    Ok(Response::default())
}

fn upsert_board(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    address: String,
    weight: u32,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(&info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let address_raw = deps.api.addr_canonicalize(&address)?;
    let key = address_raw.as_slice();
    let old_weight = read_board(deps.storage, key);

    let mut state = state_store(deps.storage).load()?;
    state.total_weight = state.total_weight + weight - old_weight;
    state_store(deps.storage).save(&state)?;

    if weight == 0 {
        board_store(deps.storage).remove(key);
    } else {
        board_store(deps.storage).save(key, &weight)?;
    }

    Ok(Response::new().add_attributes(vec![attr(
        "new_total_weight",
        state.total_weight.to_string(),
    )]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps,  _env: Env, msg: QueryMsg) -> StdResult<Binary> {
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

fn query_boards(deps: Deps) -> StdResult<BoardsResponse> {
    let boards = read_boards(deps.storage)?;
    Ok(BoardsResponse {
        boards: boards
            .into_iter()
            .map(|(addr, weight)| BoardInfo {
                address: deps.api.addr_humanize(&addr).unwrap().to_string(),
                weight,
            })
            .collect(),
    })
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    Ok(ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        quorum: config.quorum,
        threshold: config.threshold,
        voting_period: config.voting_period,
        effective_delay: config.effective_delay,
        expiration_period: config.expiration_period,
    })
}

fn query_state(deps: Deps) -> StdResult<StateInfo> {
    let state = read_state(deps.storage)?;
    Ok(StateInfo {
        poll_count: state.poll_count,
        total_weight: state.total_weight,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
