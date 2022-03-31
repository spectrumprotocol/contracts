#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{from_binary, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, CosmosMsg, WasmMsg};

use crate::{
    state::{read_config, store_config, Config, state_store, State, read_state},
    proxy::{query_staker_info_gov}
};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use spectrum_protocol::gov_proxy::{Cw20HookMsg, ExecuteMsg, QueryMsg};
use crate::proxy::{
    stake, unstake
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use astroport::querier::query_token_balance;
use astroport::staking::{Cw20HookMsg as XAstroCw20HookMsg};
use crate::querier::query_xastro_gov;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub xastro_token: String,
    pub farm_token: String,
    pub farm_gov: String,
}

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
            xastro_token: deps.api.addr_canonicalize(&msg.xastro_token)?,
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
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Unstake { amount} => unstake(deps, env, info, amount),
    }
}

fn receive_cw20(
    deps: DepsMut,
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
        xastro_token: deps.api.addr_humanize(&config.xastro_token)?.to_string(),
        farm_token: deps.api.addr_humanize(&config.farm_token)?.to_string(),
        farm_gov: deps.api.addr_humanize(&config.farm_gov)?.to_string(),
    };
    Ok(resp)
}

fn query_state(deps: Deps) -> StdResult<State> {
    read_state(deps.storage)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: ConfigInfo) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    let xastro_token = deps.api.addr_humanize(&config.xastro_token)?;
    let xastro_gov = deps.api.addr_humanize(&config.farm_gov)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let xastro_balance = query_token_balance(&deps.querier, xastro_token.clone(), env.contract.address.clone())?;
    let astro_balance = query_xastro_gov(deps.as_ref(), &config, &env.contract.address)?;

    // leave from old staking
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: xastro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: xastro_gov.to_string(),
            msg: to_binary(&XAstroCw20HookMsg::Leave {})?,
            amount: xastro_balance,
        })?,
        funds: vec![],
    }));

    // update config
    config.farm_gov = deps.api.addr_canonicalize(&msg.farm_gov)?;
    config.xastro_token = deps.api.addr_canonicalize(&msg.xastro_token)?;

    let farm_token = deps.api.addr_humanize(&config.farm_token)?;
    let xastro_gov = deps.api.addr_humanize(&config.farm_gov)?;

    // enter new staking
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: farm_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: xastro_gov.to_string(),
            msg: to_binary(&XAstroCw20HookMsg::Enter {})?,
            amount: astro_balance.balance,
        })?,
    }));

    store_config(deps.storage, &config)?;

    Ok(Response::new()
        .add_messages(messages))
}
