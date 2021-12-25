use std::collections::HashSet;
use std::iter::FromIterator;

use crate::state::{config_store, read_config, Config};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, to_binary, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, Addr};
use cw20::{Cw20ExecuteMsg};
use spectrum_protocol::pylon_liquid_farm::Cw20HookMsg as PylonLiquidCw20HookMsg;
use spectrum_protocol::staker_single_asset::{ConfigInfo, ExecuteMsg, MigrateMsg, QueryMsg, SwapOperation};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{ExecuteMsg as PairExecuteMsg, Cw20HookMsg as PairCw20HookMsg};
use terraswap::querier::{query_balance, query_token_balance};

// max slippage tolerance is 0.5
fn validate_slippage(slippage_tolerance: Decimal) -> StdResult<()> {
    if slippage_tolerance > Decimal::percent(50) {
        Err(StdError::generic_err("Slippage tolerance must be 0 to 0.5"))
    } else {
        Ok(())
    }
}

// validate contract with allowlist
fn validate_contract(contract: CanonicalAddr, allowlist: &HashSet<CanonicalAddr>) -> StdResult<()> {
    if !allowlist.contains(&contract) {
        Err(StdError::generic_err("not allowed"))
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    let allowlist = msg
        .allowlist
        .into_iter()
        .map(|w| deps.api.addr_canonicalize(&w))
        .collect::<StdResult<Vec<CanonicalAddr>>>()?;

    config_store(deps.storage).save(&Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        allowlist: HashSet::from_iter(allowlist),
    })?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::zap_to_bond {
            contract,
            provide_asset,
            swap_operations,
            max_spread,
            compound_rate,
        } => zap_to_bond(
            deps,
            env,
            info,
            contract,
            provide_asset,
            swap_operations,
            max_spread,
            compound_rate,
        ),
        ExecuteMsg::zap_to_bond_hook {
            contract,
            prev_asset,
            staker_addr,
            swap_operations,
            max_spread,
            compound_rate,
        } => zap_to_bond_hook(
            deps,
            env,
            info,
            contract,
            prev_asset,
            staker_addr,
            swap_operations,
            max_spread,
            compound_rate,
        ),
        ExecuteMsg::update_config {
            insert_allowlist,
            remove_allowlist,
        } => update_config(deps, info, insert_allowlist, remove_allowlist),
    }
}

fn query_asset_balance(
    deps: Deps,
    asset_info: &AssetInfo,
    account_addr: &Addr,
) -> StdResult<Uint128> {
    match asset_info {
        AssetInfo::Token { contract_addr } => query_token_balance(
            &deps.querier,
            deps.api.addr_validate(contract_addr)?,
            account_addr.clone(),
        ),
        AssetInfo::NativeToken { denom } => query_balance(
            &deps.querier,
            account_addr.clone(),
            denom.clone(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn zap_to_bond_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    prev_asset: Asset,
    staker_addr: String,
    swap_operations: Vec<SwapOperation>,
    max_spread: Decimal,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    // only can be called by itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let current_staking_token_amount = query_asset_balance(
        deps.as_ref(),
        &prev_asset.info,
        &env.contract.address)?;
    let amount = current_staking_token_amount.checked_sub(prev_asset.amount)?;

    swap_operation(
        deps,
        env,
        contract,
        Asset {
            info: prev_asset.info,
            amount,
        },
        staker_addr,
        swap_operations,
        max_spread,
        compound_rate,
    )
}

#[allow(clippy::too_many_arguments)]
fn swap_operation(
    deps: DepsMut,
    env: Env,
    contract: String,
    provide_asset: Asset,
    staker_addr: String,
    swap_operations: Vec<SwapOperation>,
    max_spread: Decimal,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    let splitted = swap_operations.split_first();
    let tax = provide_asset.compute_tax(&deps.querier)?;
    let amount = provide_asset.amount.checked_sub(tax)?;
    match splitted {
        None => {
            let contract_addr = match provide_asset.info {
                AssetInfo::Token { contract_addr } => contract_addr,
                AssetInfo::NativeToken { .. } => return Err(StdError::generic_err("native token not supported")),
            };
            Ok(Response::new().add_message(
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg: to_binary(&Cw20ExecuteMsg::Send {
                        amount,
                        contract,
                        msg: to_binary(&PylonLiquidCw20HookMsg::bond {
                            staker_addr: Some(staker_addr),
                            compound_rate,
                        })?,
                    })?,
                    funds: vec![],
                })
            ))
        },
        Some((swap, tails)) => {
            let prev_balance = query_asset_balance(
                deps.as_ref(),
                &swap.asset_info,
                &env.contract.address)?;
            let swap_message = match provide_asset.info.clone() {
                AssetInfo::Token { contract_addr } => CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg: to_binary(&Cw20ExecuteMsg::Send {
                        contract: swap.pair_contract.clone(),
                        amount,
                        msg: to_binary(&PairCw20HookMsg::Swap {
                            to: None,
                            max_spread: Some(max_spread),
                            belief_price: swap.belief_price,
                        })?,
                    })?,
                    funds: vec![],
                }),
                AssetInfo::NativeToken { denom } => CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: swap.pair_contract.clone(),
                    msg: to_binary(&PairExecuteMsg::Swap {
                        to: None,
                        max_spread: Some(max_spread),
                        belief_price: swap.belief_price,
                        offer_asset: Asset {
                            amount,
                            info: provide_asset.info.clone(),
                        },
                    })?,
                    funds: vec![Coin { denom, amount }]
                }),
            };
            Ok(Response::new().add_messages(vec![
                swap_message,
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_binary(&ExecuteMsg::zap_to_bond_hook {
                        contract,
                        prev_asset: Asset {
                            info: swap.asset_info.clone(),
                            amount: prev_balance,
                        },
                        staker_addr,
                        swap_operations: tails.to_vec(),
                        max_spread,
                        compound_rate,
                    })?,
                    funds: vec![]
                })
            ]))
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn zap_to_bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    provide_asset: Asset,
    swap_operations: Vec<SwapOperation>,
    max_spread: Decimal,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    validate_slippage(max_spread)?;
    provide_asset.assert_sent_native_token_balance(&info)?;

    let config = read_config(deps.storage)?;
    let contract_raw = deps.api.addr_canonicalize(contract.as_str())?;

    validate_contract(contract_raw, &config.allowlist)?;

    if let AssetInfo::Token { .. } = provide_asset.info {
        return Err(StdError::generic_err("not support provide_asset as token"));
    }

    swap_operation(
        deps,
        env,
        contract,
        provide_asset,
        info.sender.to_string(),
        swap_operations,
        max_spread,
        compound_rate,
    )
}

fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    insert_allowlist: Option<Vec<String>>,
    remove_allowlist: Option<Vec<String>>,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(add_allowlist) = insert_allowlist {
        for contract in add_allowlist.iter() {
            config.allowlist.insert(deps.api.addr_canonicalize(contract)?);
        }
    }

    if let Some(remove_allowlist) = remove_allowlist {
        for contract in remove_allowlist.iter() {
            config.allowlist.remove(&deps.api.addr_canonicalize(contract)?);
        }
    }

    config_store(deps.storage).save(&config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        allowlist: config
            .allowlist
            .into_iter()
            .map(|w| deps.api.addr_humanize(&w).map(|addr| addr.to_string()))
            .collect::<StdResult<Vec<String>>>()?,
    };

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
