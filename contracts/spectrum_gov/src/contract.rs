#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Binary, CanonicalAddr, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128,
};
use spectrum_protocol::gov::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg, StateInfo,
};

use crate::poll::{
    poll_end, poll_execute, poll_expire, poll_start, poll_vote, query_poll, query_polls,
    query_voters,
};
use crate::stake::{calc_mintable, mint, query_balances, query_vaults, stake_tokens, upsert_vault, withdraw, validate_minted};
use crate::state::{config_store, read_config, read_state, state_store, Config, State};
use cw20::Cw20ReceiveMsg;
use terraswap::querier::query_token_balance;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    validate_percentage(msg.quorum, "quorum")?;
    validate_percentage(msg.threshold, "threshold")?;
    validate_percentage(msg.warchest_ratio, "warchest_ratio")?;

    if msg.mint_end < msg.mint_start {
        return Err(StdError::generic_err("invalid mint parameters"));
    }

    let config = Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        spec_token: if let Some(spec_token) = msg.spec_token {
            deps.api.addr_canonicalize(&spec_token)?
        } else {
            CanonicalAddr::from(vec![])
        },
        quorum: msg.quorum,
        threshold: msg.threshold,
        voting_period: msg.voting_period,
        effective_delay: msg.effective_delay,
        expiration_period: msg.expiration_period,
        proposal_deposit: msg.proposal_deposit,
        mint_per_block: msg.mint_per_block,
        mint_start: msg.mint_start,
        mint_end: msg.mint_end,
        warchest_address: if let Some(warchest_address) = msg.warchest_address {
            deps.api.addr_canonicalize(&warchest_address)?
        } else {
            CanonicalAddr::from(vec![])
        },
        warchest_ratio: msg.warchest_ratio,
    };

    let state = State {
        contract_addr: deps.api.addr_canonicalize(env.contract.address.as_str())?,
        poll_count: 0,
        total_share: Uint128::zero(),
        poll_deposit: Uint128::zero(),
        last_mint: if msg.mint_end == 0 {
            0
        } else {
            env.block.height
        },
        total_weight: 0,
    };

    config_store(deps.storage).save(&config)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::default())
}

/// validate_quorum returns an error if the quorum is invalid
/// (we require 0-1)
fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " must be 0 to 1"))
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::mint {} => mint(deps, env),
        ExecuteMsg::poll_end { poll_id } => poll_end(deps, env, poll_id),
        ExecuteMsg::poll_execute { poll_id } => poll_execute(deps, env, poll_id),
        ExecuteMsg::poll_expire { poll_id } => poll_expire(deps, env, poll_id),
        ExecuteMsg::poll_vote {
            poll_id,
            vote,
            amount,
        } => poll_vote(deps, env, info, poll_id, vote, amount),
        ExecuteMsg::receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::update_config {
            owner,
            spec_token,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
            proposal_deposit,
            warchest_address,
        } => update_config(
            deps,
            env,
            info,
            owner,
            spec_token,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
            proposal_deposit,
            warchest_address,
        ),
        ExecuteMsg::upsert_vault {
            vault_address,
            weight,
        } => upsert_vault(deps, env, info, vault_address, weight),
        ExecuteMsg::withdraw { amount } => withdraw(deps, info, amount),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    // only asset contract can execute this message
    let config = read_config(deps.storage)?;
    if config.spec_token != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::poll_start {
            title,
            description,
            link,
            execute_msgs,
        }) => poll_start(
            deps,
            env,
            cw20_msg.sender,
            cw20_msg.amount,
            title,
            description,
            link,
            execute_msgs,
        ),
        Ok(Cw20HookMsg::stake_tokens { staker_addr }) => stake_tokens(
            deps,
            env,
            staker_addr.unwrap_or(cw20_msg.sender),
            cw20_msg.amount,
        ),
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

#[allow(clippy::too_many_arguments)]
fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: Option<String>,
    spec_token: Option<String>,
    quorum: Option<Decimal>,
    threshold: Option<Decimal>,
    voting_period: Option<u64>,
    effective_delay: Option<u64>,
    expiration_period: Option<u64>,
    proposal_deposit: Option<Uint128>,
    warchest_address: Option<String>,
) -> StdResult<Response> {
    let mut config = config_store(deps.storage).load()?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        let state = read_state(deps.storage)?;
        if config.owner == state.contract_addr {
            return Err(StdError::generic_err("cannot update owner"));
        }
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    if let Some(spec_token) = spec_token {
        if config.spec_token != CanonicalAddr::from(vec![]) {
            return Err(StdError::generic_err("SPEC token is already assigned"));
        }
        config.spec_token = deps.api.addr_canonicalize(&spec_token)?;
    }

    if let Some(quorum) = quorum {
        validate_percentage(quorum, "quorum")?;
        config.quorum = quorum;
    }

    if let Some(threshold) = threshold {
        validate_percentage(threshold, "threshold")?;
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

    if let Some(proposal_deposit) = proposal_deposit {
        config.proposal_deposit = proposal_deposit;
    }

    if let Some(warchest_address) = warchest_address {
        if config.warchest_address != CanonicalAddr::from(vec![]) {
            return Err(StdError::generic_err("Warchest address is already assigned"));
        }
        let state = read_state(deps.storage)?;
        validate_minted(&state, &config, env.block.height)?;
        config.warchest_address = deps.api.addr_canonicalize(&warchest_address)?;
    }

    config_store(deps.storage).save(&config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::balance { address, height } => to_binary(&query_balances(deps, address, height)?),
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::poll { poll_id } => to_binary(&query_poll(deps, poll_id)?),
        QueryMsg::polls {
            filter,
            start_after,
            limit,
            order_by,
        } => to_binary(&query_polls(deps, filter, start_after, limit, order_by)?),
        QueryMsg::state { height } => to_binary(&query_state(deps, height)?),
        QueryMsg::vaults {} => to_binary(&query_vaults(deps)?),
        QueryMsg::voters {
            poll_id,
            start_after,
            limit,
            order_by,
        } => to_binary(&query_voters(deps, poll_id, start_after, limit, order_by)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    Ok(ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        spec_token: if config.spec_token == CanonicalAddr::from(vec![]) {
            None
        } else {
            Some(deps.api.addr_humanize(&config.spec_token)?.to_string())
        },
        quorum: config.quorum,
        threshold: config.threshold,
        voting_period: config.voting_period,
        effective_delay: config.effective_delay,
        expiration_period: config.expiration_period,
        proposal_deposit: config.proposal_deposit,
        mint_per_block: config.mint_per_block,
        mint_start: config.mint_start,
        mint_end: config.mint_end,
        warchest_address: if config.warchest_address == CanonicalAddr::from(vec![]) {
            None
        } else {
            Some(
                deps.api
                    .addr_humanize(&config.warchest_address)?
                    .to_string(),
            )
        },
        warchest_ratio: config.warchest_ratio,
    })
}

fn query_state(deps: Deps, height: u64) -> StdResult<StateInfo> {
    let state = read_state(deps.storage)?;
    let config = read_config(deps.storage)?;
    let balance = query_token_balance(&deps.querier,
                                      deps.api.addr_humanize(&config.spec_token)?,
                                      deps.api.addr_humanize(&state.contract_addr)?)?;
    let mintable = calc_mintable(&state, &config, height);
    Ok(StateInfo {
        poll_count: state.poll_count,
        total_share: state.total_share,
        poll_deposit: state.poll_deposit,
        last_mint: state.last_mint,
        total_weight: state.total_weight,
        total_staked: (balance + mintable).checked_sub(state.poll_deposit)?,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
