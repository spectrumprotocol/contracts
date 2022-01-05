#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, CosmosMsg, WasmMsg};
use spectrum_protocol::deployer::{CodeInfo, ConfigInfo, ContractInfo, ExecuteMsg, MigrateMsg, QueryMsg};
use crate::state::{code_store, Config, config_store, read_codes, read_config};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    let config = Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        manager: deps.api.addr_canonicalize(&msg.manager)?,
        operator: deps.api.addr_canonicalize(&msg.operator)?,
        time_lock: msg.time_lock,
    };

    config_store(deps.storage).save(&config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::add_contract { contract_addr, code_id } => add_contract(deps, env, info, contract_addr, code_id),
        ExecuteMsg::update_contract { contract_addr, add_code_id, remove_code_ids }
            => update_contract(deps, env, info, contract_addr, add_code_id, remove_code_ids),
        ExecuteMsg::migrate { contract_addr, code_id, msg } => execute_migrate(deps, env, info, contract_addr, code_id, msg),
        ExecuteMsg::update_config { owner, operator, time_lock } => update_config(deps, info, owner, operator, time_lock),
    }
}

fn add_contract(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract_addr: String,
    code_id: u64,
) -> StdResult<Response> {

    let config = read_config(deps.storage)?;
    if config.manager != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    code_store(deps.storage, deps.api.addr_canonicalize(&contract_addr)?)
        .save(&code_id.to_be_bytes(), &CodeInfo {
            code_id,
            created_time: env.block.time.seconds(),
        })?;

    Ok(Response::default())
}

fn update_contract(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract_addr: String,
    new_code_id: Option<u64>,
    remove_code_ids: Option<Vec<u64>>,
) -> StdResult<Response> {

    let config = read_config(deps.storage)?;
    if config.manager != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(new_code_id) = new_code_id {
        code_store(deps.storage, deps.api.addr_canonicalize(&contract_addr)?)
            .save(&new_code_id.to_be_bytes(), &CodeInfo {
                code_id: new_code_id,
                created_time: env.block.time.seconds(),
            })?;
    }

    if let Some(remove_code_ids) = remove_code_ids {
        for remove_code_id in remove_code_ids.into_iter() {
            code_store(deps.storage, deps.api.addr_canonicalize(&contract_addr)?)
                .remove(&remove_code_id.to_be_bytes());
        }
    }

    Ok(Response::default())
}

fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    operator: Option<String>,
    time_lock: Option<u64>,
) -> StdResult<Response> {

    let mut config = config_store(deps.storage).load()?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    if let Some(operator) = operator {
        config.operator = deps.api.addr_canonicalize(&operator)?;
    }

    if let Some(time_lock) = time_lock {
        config.time_lock = time_lock;
    }

    config_store(deps.storage).save(&config)?;

    Ok(Response::default())
}

fn execute_migrate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract_addr: String,
    code_id: u64,
    msg: Binary,
) -> StdResult<Response> {

    let config = config_store(deps.storage).load()?;
    if config.operator != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let code_info = code_store(deps.storage, deps.api.addr_canonicalize(&contract_addr)?)
        .load(&code_id.to_be_bytes())?;

    if code_info.created_time + config.time_lock > env.block.time.seconds() {
        return Err(StdError::generic_err("contract is in timelock period"))
    }

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Migrate {
            contract_addr,
            new_code_id: code_id,
            msg,
        }))
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::contract { contract_addr } => to_binary(&query_contract(deps, contract_addr)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    Ok(ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        manager: deps.api.addr_humanize(&config.manager)?.to_string(),
        operator: deps.api.addr_humanize(&config.operator)?.to_string(),
        time_lock: config.time_lock,
    })
}

fn query_contract(deps: Deps, contract_addr: String) -> StdResult<ContractInfo> {
    let codes = read_codes(deps.storage, deps.api.addr_canonicalize(&contract_addr)?)?;
    Ok(ContractInfo {
        codes,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
