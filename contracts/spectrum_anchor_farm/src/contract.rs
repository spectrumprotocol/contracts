use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CanonicalAddr, Decimal, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, InitResponse, MigrateResponse, MigrateResult, Order, Querier,
    StdError, StdResult, Storage, Uint128,
};

use crate::{
    bond::bond,
    compound::{compound, stake},
    state::{read_config, state_store, store_config, Config, PoolInfo, State},
};

use cw20::Cw20ReceiveMsg;

use crate::bond::{deposit_spec_reward, query_reward_info, unbond, withdraw};
use crate::state::{pool_info_read, pool_info_store, read_state};
use spectrum_protocol::anchor_farm::{QueryMsg, PoolsResponse, HandleMsg, ConfigInfo, Cw20HookMsg, PoolItem, StateInfo, MigrateMsg};

/// (we require 0-1)
fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " must be 0 to 1"))
    } else {
        Ok(())
    }
}

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: ConfigInfo,
) -> StdResult<InitResponse> {
    validate_percentage(msg.community_fee, "community_fee")?;
    validate_percentage(msg.platform_fee, "platform_fee")?;
    validate_percentage(msg.controller_fee, "controller_fee")?;

    let api = deps.api;
    store_config(
        &mut deps.storage,
        &Config {
            owner: deps.api.canonical_address(&msg.owner)?,
            terraswap_factory: deps.api.canonical_address(&msg.terraswap_factory)?,
            spectrum_token: deps.api.canonical_address(&msg.spectrum_token)?,
            spectrum_gov: deps.api.canonical_address(&msg.spectrum_gov)?,
            anchor_token: deps.api.canonical_address(&msg.anchor_token)?,
            anchor_staking: deps.api.canonical_address(&msg.anchor_staking)?,
            anchor_gov: deps.api.canonical_address(&msg.anchor_gov)?,
            platform: if let Some(platform) = msg.platform {
                api.canonical_address(&platform)?
            } else {
                CanonicalAddr::default()
            },
            controller: if let Some(controller) = msg.controller {
                api.canonical_address(&controller)?
            } else {
                CanonicalAddr::default()
            },
            base_denom: msg.base_denom,
            community_fee: msg.community_fee,
            platform_fee: msg.platform_fee,
            controller_fee: msg.controller_fee,
            deposit_fee: msg.deposit_fee,
            lock_start: msg.lock_start,
            lock_end: msg.lock_end,
        },
    )?;

    state_store(&mut deps.storage).save(&State {
        contract_addr: deps.api.canonical_address(&env.contract.address)?,
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_farm_share: Uint128::zero(),
        total_weight: 0u32,
        earning: Uint128::zero(),
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
        HandleMsg::update_config {
            owner,
            platform,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
            deposit_fee,
            lock_start,
            lock_end,
        } => update_config(
            deps,
            env,
            owner,
            platform,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
            deposit_fee,
            lock_start,
            lock_end,
        ),
        HandleMsg::register_asset {
            asset_token,
            staking_token,
            weight,
            auto_compound,
        } => register_asset(deps, env, asset_token, staking_token, weight, auto_compound),
        HandleMsg::unbond {
            asset_token,
            amount,
        } => unbond(deps, env, asset_token, amount),
        HandleMsg::withdraw { asset_token } => withdraw(deps, env, asset_token),
        HandleMsg::stake { asset_token } => stake(deps, env, asset_token),
        HandleMsg::compound {} => compound(deps, env)
    }
}

pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> HandleResult {
    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::bond {
                staker_addr,
                asset_token,
                compound_rate,
            } => bond(
                deps,
                env,
                staker_addr.unwrap_or(cw20_msg.sender),
                asset_token,
                cw20_msg.amount,
                compound_rate,
            ),
        }
    } else {
        Err(StdError::generic_err("data should be given"))
    }
}

pub fn update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    platform: Option<HumanAddr>,
    controller: Option<HumanAddr>,
    community_fee: Option<Decimal>,
    platform_fee: Option<Decimal>,
    controller_fee: Option<Decimal>,
    deposit_fee: Option<Decimal>,
    lock_start: Option<u64>,
    lock_end: Option<u64>,
) -> StdResult<HandleResponse> {
    let mut config: Config = read_config(&deps.storage)?;

    if deps.api.canonical_address(&env.message.sender)? != config.owner {
        return Err(StdError::unauthorized());
    }

    if let Some(owner) = owner {
        config.owner = deps.api.canonical_address(&owner)?;
    }

    if let Some(platform) = platform {
        config.platform = deps.api.canonical_address(&platform)?;
    }

    if let Some(controller) = controller {
        config.controller = deps.api.canonical_address(&controller)?;
    }

    if let Some(community_fee) = community_fee {
        validate_percentage(community_fee, "community_fee")?;
        config.community_fee = community_fee;
    }

    if let Some(platform_fee) = platform_fee {
        validate_percentage(platform_fee, "platform_fee")?;
        config.platform_fee = platform_fee;
    }

    if let Some(controller_fee) = controller_fee {
        validate_percentage(controller_fee, "controller_fee")?;
        config.controller_fee = controller_fee;
    }

    if let Some(deposit_fee) = deposit_fee {
        validate_percentage(deposit_fee, "deposit_fee")?;
        config.deposit_fee = deposit_fee;
    }

    if let Some(lock_start) = lock_start {
        config.lock_start = lock_start;
    }

    if let Some(lock_end) = lock_end {
        config.lock_end = lock_end;
    }

    store_config(&mut deps.storage, &config)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_config")],
        data: None,
    })
}

pub fn register_asset<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_token: HumanAddr,
    staking_token: HumanAddr,
    weight: u32,
    auto_compound: bool,
) -> HandleResult {
    let config: Config = read_config(&deps.storage)?;
    let asset_token_raw = deps.api.canonical_address(&asset_token)?;

    if config.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    let pool_count = pool_info_read(&deps.storage)
        .range(None, None, Order::Descending).count();

    if pool_count >= 1 {
        return Err(StdError::generic_err("Already registered one asset"))
    }

    let mut state = read_state(&deps.storage)?;
    deposit_spec_reward(deps, &mut state, &config, env.block.height, false)?;

    let mut pool_info = pool_info_read(&deps.storage)
        .may_load(asset_token_raw.as_slice())?
        .unwrap_or_else(|| PoolInfo {
            staking_token: deps.api.canonical_address(&staking_token).unwrap(),
            total_auto_bond_share: Uint128::zero(),
            total_stake_bond_share: Uint128::zero(),
            total_stake_bond_amount: Uint128::zero(),
            weight: 0u32,
            auto_compound: false,
            farm_share: Uint128::zero(),
            farm_share_index: Decimal::zero(),
            state_spec_share_index: state.spec_share_index,
            auto_spec_share_index: Decimal::zero(),
            stake_spec_share_index: Decimal::zero(),
            reinvest_allowance: Uint128::zero(),
        });
    state.total_weight = state.total_weight + weight - pool_info.weight;
    pool_info.weight = weight;
    pool_info.auto_compound = auto_compound;

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

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::pools {} => to_binary(&query_pools(deps)?),
        QueryMsg::reward_info {
            staker_addr,
            height,
        } => to_binary(&query_reward_info(deps, staker_addr, height)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
    }
}

pub fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigInfo> {
    let config = read_config(&deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.human_address(&config.owner)?,
        terraswap_factory: deps.api.human_address(&config.terraswap_factory)?,
        spectrum_token: deps.api.human_address(&config.spectrum_token)?,
        anchor_token: deps.api.human_address(&config.anchor_token)?,
        anchor_staking: deps.api.human_address(&config.anchor_staking)?,
        spectrum_gov: deps.api.human_address(&config.spectrum_gov)?,
        anchor_gov: deps.api.human_address(&config.anchor_gov)?,
        platform: if config.platform == CanonicalAddr::default() {
            None
        } else {
            Some(deps.api.human_address(&config.platform)?)
        },
        controller: if config.controller == CanonicalAddr::default() {
            None
        } else {
            Some(deps.api.human_address(&config.controller)?)
        },
        base_denom: config.base_denom,
        community_fee: config.community_fee,
        platform_fee: config.platform_fee,
        controller_fee: config.controller_fee,
        deposit_fee: config.deposit_fee,
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
                auto_compound: pool_info.auto_compound,
                total_auto_bond_share: pool_info.total_auto_bond_share,
                total_stake_bond_share: pool_info.total_stake_bond_share,
                total_stake_bond_amount: pool_info.total_stake_bond_amount,
                farm_share: pool_info.farm_share,
                state_spec_share_index: pool_info.state_spec_share_index,
                farm_share_index: pool_info.farm_share_index,
                stake_spec_share_index: pool_info.stake_spec_share_index,
                auto_spec_share_index: pool_info.auto_spec_share_index,
                reinvest_allowance: pool_info.reinvest_allowance,
            })
        })
        .collect::<StdResult<Vec<PoolItem>>>()?;
    Ok(PoolsResponse { pools })
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateInfo> {
    let state = read_state(&deps.storage)?;
    Ok(StateInfo {
        spec_share_index: state.spec_share_index,
        previous_spec_share: state.previous_spec_share,
        total_farm_share: state.total_farm_share,
        total_weight: state.total_weight,
    })
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    let config = read_config(&deps.storage)?;
    let state = read_state(&deps.storage)?;
    let mut pool_info = pool_info_read(&deps.storage).load(config.anchor_token.as_slice())?;
    let new_share = state.total_farm_share;
    let share_per_bond = Decimal::from_ratio(new_share, pool_info.total_stake_bond_share);
    pool_info.farm_share_index = pool_info.farm_share_index + share_per_bond;
    pool_info.farm_share += new_share;
    pool_info_store(&mut deps.storage).save(config.anchor_token.as_slice(), &pool_info)?;
    Ok(MigrateResponse::default())
}
