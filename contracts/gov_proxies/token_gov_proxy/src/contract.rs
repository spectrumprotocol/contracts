use classic_bindings::TerraQuery;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{from_binary, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};

use crate::{
    state::{read_config, store_config, Config},
    proxy::{query_staker_info_gov}
};

use cw20::Cw20ReceiveMsg;

use spectrum_protocol::gov_proxy::{
    Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg,
};
use crate::proxy::{
    stake, unstake
};
use crate::state::{read_state, State, state_store};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub farm_token: String,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<TerraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    store_config(
        deps.storage,
        &Config {
            farm_token: deps.api.addr_canonicalize(&msg.farm_token)?,
        },
    )?;

    state_store(deps.storage).save(&State {
        total_share: Uint128::zero(),
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut<TerraQuery>, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Unstake { amount} => unstake(deps, env, info, amount),
    }
}

fn receive_cw20(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&msg.msg) {
        Ok(Cw20HookMsg::Stake {}) => stake(
            deps,
            env,
            info,
            msg.sender,
            msg.amount,
        ),
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<TerraQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::Staker { address } => to_binary(&query_staker_info_gov(deps, env, address)?)
    }
}

fn query_config(deps: Deps<TerraQuery>) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        farm_token: deps.api.addr_humanize(&config.farm_token)?.to_string(),
    };
    Ok(resp)
}

fn query_state(deps: Deps<TerraQuery>) -> StdResult<State> {
    read_state(deps.storage)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut<TerraQuery>, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
