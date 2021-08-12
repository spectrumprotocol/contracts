use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CanonicalAddr, Decimal, Env, Extern, HandleResponse,
    HandleResult, String, InitResponse, MigrateResponse, MigrateResult, Order, Querier,
    StdError, StdResult, Storage, Uint128,
};

use crate::state::{
    config_store, pool_info_read, pool_info_store, read_config, read_state, state_store, Config,
    PoolInfo, State,
};

use crate::bond::{bond, deposit_reward, query_reward_info, unbond, withdraw};
use cw20::Cw20ReceiveMsg;
use spectrum_protocol::spec_farm::{
    ConfigInfo, Cw20HookMsg, HandleMsg, MigrateMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: ConfigInfo,
) -> StdResult<InitResponse> {
    config_store(&mut deps.storage).save(&Config {
        owner: deps.api.canonical_address(&msg.owner)?,
        spectrum_gov: deps.api.canonical_address(&msg.spectrum_gov)?,
        spectrum_token: deps.api.canonical_address(&msg.spectrum_token)?,
        lock_start: msg.lock_start,
        lock_end: msg.lock_end,
    })?;

    state_store(&mut deps.storage).save(&State {
        contract_addr: deps.api.canonical_address(&env.contract.address)?,
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_weight: 0u32,
    })?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::receive(msg) => receive_cw20(deps, env, msg),
        HandleMsg::register_asset {
            asset_token,
            staking_token,
            weight,
        } => register_asset(deps, env, asset_token, staking_token, weight),
        HandleMsg::withdraw { asset_token } => withdraw(deps, env, asset_token),
        HandleMsg::unbond {
            asset_token,
            amount,
        } => unbond(deps, env, asset_token, amount),
        HandleMsg::update_config {
            owner,
            lock_start,
            lock_end,
        } => update_config(deps, env, owner, lock_start, lock_end),
    }
}

fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> HandleResult {
    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::bond {
                staker_addr,
                asset_token,
            } => bond(
                deps,
                env,
                staker_addr.unwrap_or(cw20_msg.sender),
                asset_token,
                cw20_msg.amount,
            ),
        }
    } else {
        Err(StdError::generic_err("data should be given"))
    }
}

fn register_asset<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_token: String,
    staking_token: String,
    weight: u32,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    let asset_token_raw = deps.api.canonical_address(&asset_token)?;

    if config.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    let mut state = read_state(&deps.storage)?;
    deposit_reward(deps, &mut state, &config, env.block.height, false)?;

    let mut pool_info = pool_info_read(&deps.storage)
        .may_load(asset_token_raw.as_slice())?
        .unwrap_or_else(|| PoolInfo {
            staking_token: deps.api.canonical_address(&staking_token).unwrap(),
            total_bond_amount: Uint128::zero(),
            weight: 0u32,
            state_spec_share_index: state.spec_share_index,
            spec_share_index: Decimal::zero(),
        });
    state.total_weight = state.total_weight + weight - pool_info.weight;
    pool_info.weight = weight;

    pool_info_store(&mut deps.storage).save(&asset_token_raw.as_slice(), &pool_info)?;
    state_store(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "register_asset"),
            log("asset_token", asset_token.as_str()),
        ],
        data: None,
    })
}

fn update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<String>,
    lock_start: Option<u64>,
    lock_end: Option<u64>,
) -> StdResult<HandleResponse> {
    let mut config = read_config(&deps.storage)?;

    if deps.api.canonical_address(&env.message.sender)? != config.owner {
        return Err(StdError::unauthorized());
    }

    if let Some(owner) = owner {
        config.owner = deps.api.canonical_address(&owner)?;
    }

    if let Some(lock_start) = lock_start {
        config.lock_start = lock_start;
    }

    if let Some(lock_end) = lock_end {
        config.lock_end = lock_end;
    }

    config_store(&mut deps.storage).save(&config)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_config")],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::pools {} => to_binary(&query_pools(deps)?),
        QueryMsg::reward_info {
            staker_addr,
            asset_token,
            height,
        } => to_binary(&query_reward_info(deps, staker_addr, asset_token, height)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<ConfigInfo> {
    let config = read_config(&deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.human_address(&config.owner)?,
        spectrum_token: deps.api.human_address(&config.spectrum_token)?,
        spectrum_gov: deps.api.human_address(&config.spectrum_gov)?,
        lock_start: config.lock_start,
        lock_end: config.lock_end,
    };

    Ok(resp)
}

fn query_pools<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<PoolsResponse> {
    let pools = pool_info_read(&deps.storage)
        .range(None, None, Order::Descending)
        .map(|item| {
            let (asset_token, pool_info) = item?;
            Ok(PoolItem {
                asset_token: deps.api.human_address(&CanonicalAddr::from(asset_token))?,
                staking_token: deps.api.human_address(&pool_info.staking_token)?,
                weight: pool_info.weight,
                total_bond_amount: pool_info.total_bond_amount,
                state_spec_share_index: pool_info.state_spec_share_index,
                spec_share_index: pool_info.spec_share_index,
            })
        })
        .collect::<StdResult<Vec<PoolItem>>>()?;
    Ok(PoolsResponse { pools })
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateInfo> {
    let state = read_state(&deps.storage)?;
    Ok(StateInfo {
        previous_spec_share: state.previous_spec_share,
        spec_share_index: state.spec_share_index,
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
