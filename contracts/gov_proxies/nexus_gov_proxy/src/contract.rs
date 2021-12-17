#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{from_binary, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};

use crate::{
    state::{read_config, store_config, Config},
    proxy::{query_staker_info_gov}
};

use cw20::Cw20ReceiveMsg;

use spectrum_protocol::gov_proxy::{
    ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg,
};
use crate::proxy::{
    stake, unstake
};
use crate::querier::query_nexus_gov;
use crate::state::{Account, account_store, read_state, State, state_store};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    store_config(
        deps.storage,
        &Config {
            farm_token: deps.api.addr_canonicalize(&msg.farm_token)?,
            farm_gov: deps.api.addr_canonicalize(&msg.farm_gov)?,
        },
    )?;

    state_store(deps.storage).save(&State {
        total_share: Uint128::zero(),
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, msg),
        ExecuteMsg::Unstake { amount} => unstake(deps, env, info, amount),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&msg.msg) {
        Ok(Cw20HookMsg::Stake {}) => stake(
            deps,
            env,
            msg.sender,
            msg.amount,
        ),
        Err(_) => Err(StdError::generic_err("data should be given")),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::Staker { address } => to_binary(&query_staker_info_gov(deps, env, address)?)
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        farm_token: deps.api.addr_humanize(&config.farm_token)?.to_string(),
        farm_gov: deps.api.addr_humanize(&config.farm_gov)?.to_string(),
    };
    Ok(resp)
}

fn query_state(deps: Deps) -> StdResult<State> {
    read_state(deps.storage)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;
    let gov_response = query_nexus_gov(deps.as_ref(), &config.farm_gov, &env.contract.address)?;
    if !gov_response.balance.is_zero() {
        let mut state = read_state(deps.storage)?;
        state.total_share = gov_response.balance;
        let account = Account {
            share: gov_response.balance,
        };
        state_store(deps.storage).save(&state)?;
        account_store(deps.storage)
            .save(deps.api.addr_canonicalize(&msg.farm_contract)?.as_slice(), &account)?;
    }

    Ok(Response::default())
}
