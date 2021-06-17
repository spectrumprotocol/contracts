use cosmwasm_std::{
    from_binary, to_binary, Api, Binary, CanonicalAddr, Decimal, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, InitResponse, MigrateResponse, MigrateResult, Querier, StdError,
    StdResult, Storage, Uint128,
};
use spectrum_protocol::gov::{ConfigInfo, Cw20HookMsg, HandleMsg, MigrateMsg, QueryMsg, StateInfo};

use crate::poll::{
    poll_end, poll_execute, poll_expire, poll_start, poll_vote, query_poll, query_polls,
    query_voters,
};
use crate::stake::{
    calc_mintable, mint, query_balances, query_vaults, stake_tokens, upsert_vault, validate_minted,
    withdraw,
};
use crate::state::{config_store, read_config, read_state, state_store, Config, State};
use cw20::Cw20ReceiveMsg;
use spectrum_protocol::querier::load_token_balance;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: ConfigInfo,
) -> StdResult<InitResponse> {
    validate_percentage(msg.quorum, "quorum")?;
    validate_percentage(msg.threshold, "threshold")?;
    validate_percentage(msg.warchest_ratio, "warchest_ratio")?;

    let config = Config {
        owner: deps.api.canonical_address(&msg.owner)?,
        spec_token: if let Some(spec_token) = msg.spec_token {
            deps.api.canonical_address(&spec_token)?
        } else {
            CanonicalAddr::default()
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
            deps.api.canonical_address(&warchest_address)?
        } else {
            CanonicalAddr::default()
        },
        warchest_ratio: msg.warchest_ratio,
    };

    let state = State {
        contract_addr: deps.api.canonical_address(&env.contract.address)?,
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

    config_store(&mut deps.storage).save(&config)?;
    state_store(&mut deps.storage).save(&state)?;

    Ok(InitResponse::default())
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

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::mint {} => mint(deps, env),
        HandleMsg::poll_end { poll_id } => poll_end(deps, env, poll_id),
        HandleMsg::poll_execute { poll_id } => poll_execute(deps, env, poll_id),
        HandleMsg::poll_expire { poll_id } => poll_expire(deps, env, poll_id),
        HandleMsg::poll_vote {
            poll_id,
            vote,
            amount,
        } => poll_vote(deps, env, poll_id, vote, amount),
        HandleMsg::receive(msg) => receive_cw20(deps, env, msg),
        HandleMsg::update_config {
            owner,
            spec_token,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
            proposal_deposit,
            mint_per_block,
            mint_start,
            mint_end,
            warchest_address,
            warchest_ratio,
        } => update_config(
            deps,
            env,
            owner,
            spec_token,
            quorum,
            threshold,
            voting_period,
            effective_delay,
            expiration_period,
            proposal_deposit,
            mint_per_block,
            mint_start,
            mint_end,
            warchest_address,
            warchest_ratio,
        ),
        HandleMsg::upsert_vault {
            vault_address,
            weight,
        } => upsert_vault(deps, env, vault_address, weight),
        HandleMsg::withdraw { amount } => withdraw(deps, env, amount),
    }
}

pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> HandleResult {
    // only asset contract can execute this message
    let config = read_config(&deps.storage)?;
    if config.spec_token != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::poll_start {
                title,
                description,
                link,
                execute_msgs,
            } => poll_start(
                deps,
                env,
                cw20_msg.sender,
                cw20_msg.amount,
                title,
                description,
                link,
                execute_msgs,
            ),
            Cw20HookMsg::stake_tokens { staker_addr } => stake_tokens(
                deps,
                env,
                staker_addr.unwrap_or(cw20_msg.sender),
                cw20_msg.amount,
            ),
        }
    } else {
        Err(StdError::generic_err("data should be given"))
    }
}

fn update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    spec_token: Option<HumanAddr>,
    quorum: Option<Decimal>,
    threshold: Option<Decimal>,
    voting_period: Option<u64>,
    effective_delay: Option<u64>,
    expiration_period: Option<u64>,
    proposal_deposit: Option<Uint128>,
    mint_per_block: Option<Uint128>,
    mint_start: Option<u64>,
    mint_end: Option<u64>,
    warchest_address: Option<HumanAddr>,
    warchest_ratio: Option<Decimal>,
) -> HandleResult {
    let mut config = config_store(&mut deps.storage).load()?;
    if config.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    if let Some(owner) = owner {
        config.owner = deps.api.canonical_address(&owner)?;
    }

    if let Some(spec_token) = spec_token {
        if config.spec_token != CanonicalAddr::default() {
            return Err(StdError::generic_err("SPEC token is already assigned"));
        }
        config.spec_token = deps.api.canonical_address(&spec_token)?;
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

    if mint_per_block.is_some()
        || mint_start.is_some()
        || mint_end.is_some()
        || warchest_address.is_some()
        || warchest_ratio.is_some()
    {
        let state = read_state(&deps.storage)?;
        validate_minted(&state, &config, env.block.height)?;
    }

    if let Some(mint_per_block) = mint_per_block {
        config.mint_per_block = mint_per_block;
    }

    if let Some(mint_start) = mint_start {
        config.mint_start = mint_start;
    }

    if let Some(mint_end) = mint_end {
        config.mint_end = mint_end;

        let mut state = state_store(&mut deps.storage).load()?;
        if validate_minted(&state, &config, env.block.height).is_err() {
            state.last_mint = env.block.height;
            state_store(&mut deps.storage).save(&state)?;
        }
    }

    if let Some(warchest_address) = warchest_address {
        config.warchest_address = deps.api.canonical_address(&warchest_address)?;
    }

    if let Some(warchest_ratio) = warchest_ratio {
        validate_percentage(warchest_ratio, "warchest_ratio")?;
        config.warchest_ratio = warchest_ratio;
    }

    config_store(&mut deps.storage).save(&config)?;

    Ok(HandleResponse::default())
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
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

fn query_config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<ConfigInfo> {
    let config = read_config(&deps.storage)?;
    Ok(ConfigInfo {
        owner: deps.api.human_address(&config.owner)?,
        spec_token: if config.spec_token == CanonicalAddr::default() {
            None
        } else {
            Some(deps.api.human_address(&config.spec_token)?)
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
        warchest_address: if config.warchest_address == CanonicalAddr::default() {
            None
        } else {
            Some(deps.api.human_address(&config.warchest_address)?)
        },
        warchest_ratio: config.warchest_ratio,
    })
}

fn query_state<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    height: u64,
) -> StdResult<StateInfo> {
    let state = read_state(&deps.storage)?;
    let config = read_config(&deps.storage)?;
    let balance = load_token_balance(deps, &config.spec_token, &state.contract_addr)?;
    let mintable = calc_mintable(&state, &config, height);
    Ok(StateInfo {
        poll_count: state.poll_count,
        total_share: state.total_share,
        poll_deposit: state.poll_deposit,
        last_mint: state.last_mint,
        total_weight: state.total_weight,
        total_staked: (balance + mintable - state.poll_deposit)?,
    })
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    Ok(MigrateResponse::default())
}
