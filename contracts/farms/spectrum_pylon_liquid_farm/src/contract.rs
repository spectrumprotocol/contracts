use classic_bindings::TerraQuery;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_binary, to_binary, Binary, CanonicalAddr, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, Response, StdError, StdResult, Uint128,
};

use crate::{
    bond::bond,
    state::{read_config, state_store, store_config, Config, PoolInfo, State},
};

use cw20::Cw20ReceiveMsg;

use crate::bond::{deposit_spec_reward, query_reward_info, unbond, update_bond, withdraw};
use crate::state::{pool_info_read, pool_info_store, read_state};
use spectrum_protocol::pylon_liquid_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};
use crate::compound::{compound, send_fee};

/// (we require 0-1)
fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " must be 0 to 1"))
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<TerraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    validate_percentage(msg.community_fee, "community_fee")?;
    validate_percentage(msg.platform_fee, "platform_fee")?;
    validate_percentage(msg.controller_fee, "controller_fee")?;
    validate_percentage(msg.deposit_fee, "deposit_fee")?;

    store_config(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(&msg.owner)?,
            dp_token: deps.api.addr_canonicalize(&msg.dp_token)?,
            reward_token: deps.api.addr_canonicalize(&msg.reward_token)?,
            gov_proxy: if let Some(gov_proxy) = msg.gov_proxy {
                Some(deps.api.addr_canonicalize(&gov_proxy)?)
            } else {
                None
            },
            spectrum_token: deps.api.addr_canonicalize(&msg.spectrum_token)?,
            spectrum_gov: deps.api.addr_canonicalize(&msg.spectrum_gov)?,
            gateway_pool: deps.api.addr_canonicalize(&msg.gateway_pool)?,
            platform: deps.api.addr_canonicalize(&msg.platform)?,
            controller: deps.api.addr_canonicalize(&msg.controller)?,
            community_fee: msg.community_fee,
            platform_fee: msg.platform_fee,
            controller_fee: msg.controller_fee,
            deposit_fee: msg.deposit_fee,
            anchor_market: deps.api.addr_canonicalize(&msg.anchor_market)?,
            aust_token: deps.api.addr_canonicalize(&msg.aust_token)?,
            pair_contract: deps.api.addr_canonicalize(&msg.pair_contract)?,
            ust_pair_contract: deps.api.addr_canonicalize(&msg.ust_pair_contract)?,
        },
    )?;

    state_store(deps.storage).save(&State {
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_farm_share: Uint128::zero(),
        total_weight: 0u32,
        earning: Uint128::zero(),
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut<TerraQuery>, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::update_config {
            owner,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
            deposit_fee,
        } => update_config(
            deps,
            info,
            owner,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
            deposit_fee,
        ),
        ExecuteMsg::register_asset {
            dp_token,
            weight,
        } => register_asset(
            deps,
            env,
            info,
            dp_token,
            weight,
        ),
        ExecuteMsg::unbond {
            asset_token,
            amount,
        } => unbond(deps, env, info, asset_token, amount),
        ExecuteMsg::withdraw { asset_token, spec_amount, farm_amount } => withdraw(deps, env, info, asset_token, spec_amount, farm_amount),
        ExecuteMsg::compound {} => compound(deps, env, info),
        ExecuteMsg::update_bond { asset_token, amount_to_auto, amount_to_stake } => update_bond(deps, env, info, asset_token, amount_to_auto, amount_to_stake),
        ExecuteMsg::send_fee {} => send_fee(deps, env, info),
    }
}

fn receive_cw20(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::bond {
               staker_addr,
               compound_rate
           }) => bond(
            deps,
            env,
            staker_addr.unwrap_or(cw20_msg.sender),
            info.sender.to_string(),
            cw20_msg.amount,
            compound_rate
        ),
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut<TerraQuery>,
    info: MessageInfo,
    owner: Option<String>,
    controller: Option<String>,
    community_fee: Option<Decimal>,
    platform_fee: Option<Decimal>,
    controller_fee: Option<Decimal>,
    deposit_fee: Option<Decimal>,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        if config.owner == config.spectrum_gov {
            return Err(StdError::generic_err("cannot update owner"));
        }
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    if let Some(controller) = controller {
        config.controller = deps.api.addr_canonicalize(&controller)?;
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

    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

fn register_asset(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    dp_token: String,
    weight: u32,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let dp_token_raw = deps.api.addr_canonicalize(&dp_token)?;

    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let pool_count = pool_info_read(deps.storage)
        .range(None, None, Order::Descending)
        .count();

    if pool_count >= 1 {
        return Err(StdError::generic_err("Already registered one dp token"));
    }

    let mut state = read_state(deps.storage)?;
    deposit_spec_reward(deps.as_ref(), &env, &mut state, &config, false)?;

    let mut pool_info = pool_info_read(deps.storage)
        .may_load(dp_token_raw.as_slice())?
        .unwrap_or_else(|| PoolInfo {
            total_auto_bond_share: Uint128::zero(),
            total_stake_bond_share: Uint128::zero(),
            total_stake_bond_amount: Uint128::zero(),
            weight: 0u32,
            farm_share: Uint128::zero(),
            farm_share_index: Decimal::zero(),
            state_spec_share_index: state.spec_share_index,
            auto_spec_share_index: Decimal::zero(),
            stake_spec_share_index: Decimal::zero(),
        });
    state.total_weight = state.total_weight + weight - pool_info.weight;
    pool_info.weight = weight;

    pool_info_store(deps.storage).save(dp_token_raw.as_slice(), &pool_info)?;
    state_store(deps.storage).save(&state)?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "register_asset"),
        attr("dp_token", dp_token),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TerraQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::pools {} => to_binary(&query_pools(deps)?),
        QueryMsg::reward_info {
            staker_addr,
        } => to_binary(&query_reward_info(deps, env, staker_addr)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
    }
}

fn query_config(deps: Deps<TerraQuery>) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        dp_token: deps.api.addr_humanize(&config.dp_token)?.to_string(),
        reward_token: deps.api.addr_humanize(&config.reward_token)?.to_string(),
        gov_proxy: if let Some(gov_proxy) = config.gov_proxy {
            Some(deps.api.addr_humanize(&gov_proxy)?.to_string())
        } else {
            None
        },
        spectrum_token: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
        spectrum_gov: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        gateway_pool: deps.api.addr_humanize(&config.gateway_pool)?.to_string(),
        platform: deps.api.addr_humanize(&config.platform)?.to_string(),
        controller: deps.api.addr_humanize(&config.controller)?.to_string(),
        community_fee: config.community_fee,
        platform_fee: config.platform_fee,
        controller_fee: config.controller_fee,
        deposit_fee: config.deposit_fee,
        anchor_market: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
        aust_token: deps.api.addr_humanize(&config.aust_token)?.to_string(),
        pair_contract: deps.api.addr_humanize(&config.pair_contract)?.to_string(),
        ust_pair_contract: deps.api.addr_humanize(&config.ust_pair_contract)?.to_string(),
    };

    Ok(resp)
}

fn query_pools(deps: Deps<TerraQuery>) -> StdResult<PoolsResponse> {
    let pools = pool_info_read(deps.storage)
        .range(None, None, Order::Descending)
        .map(|item| {
            let (dp_token, pool_info) = item?;
            Ok(PoolItem {
                asset_token: deps
                    .api
                    .addr_humanize(&CanonicalAddr::from(dp_token))?
                    .to_string(),
                weight: pool_info.weight,
                total_auto_bond_share: pool_info.total_auto_bond_share,
                total_stake_bond_share: pool_info.total_stake_bond_share,
                total_stake_bond_amount: pool_info.total_stake_bond_amount,
                farm_share: pool_info.farm_share,
                state_spec_share_index: pool_info.state_spec_share_index,
                farm_share_index: pool_info.farm_share_index,
                stake_spec_share_index: pool_info.stake_spec_share_index,
                auto_spec_share_index: pool_info.auto_spec_share_index,
            })
        })
        .collect::<StdResult<Vec<PoolItem>>>()?;
    Ok(PoolsResponse { pools })
}

fn query_state(deps: Deps<TerraQuery>) -> StdResult<StateInfo> {
    let state = read_state(deps.storage)?;
    Ok(StateInfo {
        spec_share_index: state.spec_share_index,
        previous_spec_share: state.previous_spec_share,
        total_farm_share: state.total_farm_share,
        total_weight: state.total_weight,
        earning: state.earning,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut<TerraQuery>, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
