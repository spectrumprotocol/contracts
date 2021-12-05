#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_binary, to_binary, Binary, CanonicalAddr, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, Response, StdError, StdResult, Uint128,
};

use crate::state::{
    config_store, pool_info_read, pool_info_store, read_config, read_state, state_store, Config,
    PoolInfo, State,
};

use crate::bond::{bond, deposit_reward, query_reward_info, unbond, withdraw};
use cw20::Cw20ReceiveMsg;
use spectrum_protocol::spec_farm::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, PoolItem, PoolsResponse, QueryMsg, StateInfo,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {

    config_store(deps.storage).save(&Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        spectrum_gov: deps.api.addr_canonicalize(&msg.spectrum_gov)?,
        spectrum_token: deps.api.addr_canonicalize(&msg.spectrum_token)?,
    })?;

    state_store(deps.storage).save(&State {
        contract_addr: deps.api.addr_canonicalize(env.contract.address.as_str())?,
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_weight: 0u32,
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, _env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::receive(msg) => receive_cw20(deps, info, msg),
        ExecuteMsg::register_asset {
            asset_token,
            staking_token,
            weight,
        } => register_asset(deps, info, asset_token, staking_token, weight),
        ExecuteMsg::withdraw { asset_token, spec_amount } => withdraw(deps, info, asset_token, spec_amount),
        ExecuteMsg::unbond {
            asset_token,
            amount,
        } => unbond(deps, info, asset_token, amount),
        ExecuteMsg::update_config {
            owner,
        } => update_config(deps, info, owner),
    }
}

fn receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::bond {
            staker_addr,
            asset_token,
        }) => bond(
            deps,
            info,
            staker_addr.unwrap_or(cw20_msg.sender),
            asset_token,
            cw20_msg.amount,
        ),
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

fn register_asset(
    deps: DepsMut,
    info: MessageInfo,
    asset_token: String,
    staking_token: String,
    weight: u32,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut state = read_state(deps.storage)?;
    deposit_reward(deps.as_ref(), &mut state, &config, false)?;

    let mut pool_info = pool_info_read(deps.storage)
        .may_load(asset_token_raw.as_slice())?
        .unwrap_or_else(|| PoolInfo {
            staking_token: deps.api.addr_canonicalize(&staking_token).unwrap(),
            total_bond_amount: Uint128::zero(),
            weight: 0u32,
            state_spec_share_index: state.spec_share_index,
            spec_share_index: Decimal::zero(),
        });
    state.total_weight = state.total_weight + weight - pool_info.weight;
    pool_info.weight = weight;

    pool_info_store(deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    state_store(deps.storage).save(&state)?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "register_asset"),
        attr("asset_token", asset_token),
    ]))
}

fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        if config.owner == config.spectrum_gov {
            return Err(StdError::generic_err("cannot update owner"));
        }
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    config_store(deps.storage).save(&config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::pools {} => to_binary(&query_pools(deps)?),
        QueryMsg::reward_info {
            staker_addr,
            asset_token,
        } => to_binary(&query_reward_info(deps, staker_addr, asset_token)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        spectrum_token: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
        spectrum_gov: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
    };

    Ok(resp)
}

fn query_pools(deps: Deps) -> StdResult<PoolsResponse> {
    let pools = pool_info_read(deps.storage)
        .range(None, None, Order::Descending)
        .map(|item| {
            let (asset_token, pool_info) = item?;
            Ok(PoolItem {
                asset_token: deps
                    .api
                    .addr_humanize(&CanonicalAddr::from(asset_token))?
                    .to_string(),
                staking_token: deps
                    .api
                    .addr_humanize(&pool_info.staking_token)?
                    .to_string(),
                weight: pool_info.weight,
                total_bond_amount: pool_info.total_bond_amount,
                state_spec_share_index: pool_info.state_spec_share_index,
                spec_share_index: pool_info.spec_share_index,
            })
        })
        .collect::<StdResult<Vec<PoolItem>>>()?;
    Ok(PoolsResponse { pools })
}

fn query_state(deps: Deps) -> StdResult<StateInfo> {
    let state = read_state(deps.storage)?;
    Ok(StateInfo {
        previous_spec_share: state.previous_spec_share,
        spec_share_index: state.spec_share_index,
        total_weight: state.total_weight,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
