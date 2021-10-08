use std::collections::HashSet;
use std::iter::FromIterator;

use crate::state::{config_store, read_config, Config};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::mirror_farm::Cw20HookMsg;
use spectrum_protocol::staker::{ConfigInfo, ExecuteMsg, MigrateMsg, QueryMsg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::ExecuteMsg as PairExecuteMsg;
use terraswap::querier::{query_pair_info, query_token_balance};

// max slippage tolerance is 0.5
fn validate_slippage(slippage_tolerance: Decimal) -> StdResult<()> {
    if slippage_tolerance > Decimal::percent(50) {
        Err(StdError::generic_err("Slippage tolerance must be 0 to 0.5"))
    } else {
        Ok(())
    }
}

// validate contract with allowlist
fn validate_contract(contract: CanonicalAddr, allowlist: HashSet<CanonicalAddr>) -> StdResult<()> {
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
        terraswap_factory: deps.api.addr_canonicalize(&msg.terraswap_factory)?,
        allowlist: HashSet::from_iter(allowlist),
    })?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::bond {
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
            staker_addr,
        } => bond(
            deps,
            env,
            info,
            contract,
            assets,
            slippage_tolerance,
            compound_rate,
            staker_addr,
        ),
        ExecuteMsg::bond_hook {
            contract,
            asset_token,
            staking_token,
            staker_addr,
            prev_staking_token_amount,
            compound_rate,
        } => bond_hook(
            deps,
            env,
            info,
            contract,
            asset_token,
            staking_token,
            staker_addr,
            prev_staking_token_amount,
            compound_rate,
        ),
        ExecuteMsg::zap_to_bond {
            contract,
            provide_asset,
            pair_asset,
            belief_price,
            max_spread,
            compound_rate,
        } => zap_to_bond(
            deps,
            env,
            info,
            contract,
            provide_asset,
            pair_asset,
            belief_price,
            max_spread,
            compound_rate,
        ),
        ExecuteMsg::zap_to_bond_hook {
            contract,
            bond_asset,
            asset_token,
            staker_addr,
            prev_asset_token_amount,
            slippage_tolerance,
            compound_rate,
        } => zap_to_bond_hook(
            deps,
            env,
            info,
            contract,
            bond_asset,
            asset_token,
            staker_addr,
            prev_asset_token_amount,
            slippage_tolerance,
            compound_rate,
        ),
        ExecuteMsg::update_config {
            insert_allowlist,
            remove_allowlist,
        } => update_config(deps, info,  insert_allowlist, remove_allowlist),
    }
}

#[allow(clippy::too_many_arguments)]
fn bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    assets: [Asset; 2],
    slippage_tolerance: Decimal,
    compound_rate: Option<Decimal>,
    staker_addr: Option<String>,
) -> StdResult<Response> {
    validate_slippage(slippage_tolerance)?;

    let config = read_config(deps.storage)?;
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let contract_raw = deps.api.addr_canonicalize(contract.as_str())?;

    validate_contract(contract_raw, config.allowlist)?;

    let mut native_asset_op: Option<Asset> = None;
    let mut token_info_op: Option<(String, Uint128)> = None;
    for asset in assets.iter() {
        match asset.info.clone() {
            AssetInfo::Token { contract_addr } => {
                token_info_op = Some((contract_addr, asset.amount))
            }
            AssetInfo::NativeToken { .. } => {
                if info.sender != env.contract.address {
                    asset.assert_sent_native_token_balance(&info)?;
                }
                native_asset_op = Some(asset.clone())
            }
        }
    }

    // will fail if one of them is missing
    let native_asset = match native_asset_op {
        Some(v) => v,
        None => return Err(StdError::generic_err("Missing native asset")),
    };
    let (token_addr, token_amount) = match token_info_op {
        Some(v) => v,
        None => return Err(StdError::generic_err("Missing token asset")),
    };

    // query pair info to obtain pair contract address
    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];
    let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;

    // get current lp token amount to later compute the received amount
    let prev_staking_token_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&terraswap_pair.liquidity_token)?,
        env.contract.address.clone(),
    )?;

    // compute tax
    let tax_amount = native_asset.compute_tax(&deps.querier)?;
    let native_asset = Asset {
        amount: native_asset.amount.checked_sub(tax_amount)?,
        info: native_asset.info,
    };

    // 1. Transfer token asset to staking contract
    // 2. Increase allowance of token for pair contract
    // 3. Provide liquidity
    // 4. Execute staking hook, will stake in the name of the sender

    let staker = staker_addr.unwrap_or_else(|| info.sender.to_string());

    let mut messages: Vec<CosmosMsg> = vec![];
    if info.sender != env.contract.address {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: token_addr.clone(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: staker.clone(),
                recipient: env.contract.address.to_string(),
                amount: token_amount,
            })?,
            funds: vec![],
        }));
    }
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_addr.clone(),
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pair.contract_addr.clone(),
            amount: token_amount,
            expires: None,
        })?,
        funds: vec![],
    }));
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraswap_pair.contract_addr,
        msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
            assets: if let AssetInfo::NativeToken { .. } = assets[0].info.clone() {
                [native_asset.clone(), assets[1].clone()]
            } else {
                [assets[0].clone(), native_asset.clone()]
            },
            slippage_tolerance: Some(slippage_tolerance),
            receiver: None,
        })?,
        funds: vec![Coin {
            denom: native_asset.info.to_string(),
            amount: native_asset.amount,
        }],
    }));
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::bond_hook {
            contract,
            asset_token: token_addr.clone(),
            staking_token: terraswap_pair.liquidity_token,
            staker_addr: staker,
            prev_staking_token_amount,
            compound_rate,
        })?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "bond"),
        attr("asset_token", token_addr),
        attr("tax_amount", tax_amount),
    ]))
}

#[allow(clippy::too_many_arguments)]
fn bond_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    asset_token: String,
    staking_token: String,
    staker_addr: String,
    prev_staking_token_amount: Uint128,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    // only can be called by itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    // stake all lp tokens received, compare with staking token amount before liquidity provision was executed
    let current_staking_token_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&staking_token)?,
        env.contract.address,
    )?;
    let amount_to_stake = current_staking_token_amount.checked_sub(prev_staking_token_amount)?;

    Ok(
        Response::new().add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token,
            msg: to_binary(&Cw20ExecuteMsg::Send {
                amount: amount_to_stake,
                contract,
                msg: to_binary(&Cw20HookMsg::bond {
                    asset_token,
                    staker_addr: Some(staker_addr),
                    compound_rate,
                })?,
            })?,
            funds: vec![],
        })]),
    )
}

#[allow(clippy::too_many_arguments)]
fn zap_to_bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    provide_asset: Asset,
    pair_asset: AssetInfo,
    belief_price: Option<Decimal>,
    max_spread: Decimal,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    validate_slippage(max_spread)?;

    let config = read_config(deps.storage)?;
    let contract_raw = deps.api.addr_canonicalize(contract.as_str())?;

    validate_contract(contract_raw, config.allowlist)?;

    let denom = match provide_asset.info.clone() {
        AssetInfo::NativeToken { denom } => denom,
        _ => return Err(StdError::generic_err("unauthorized")),
    };
    let asset_token = match pair_asset.clone() {
        AssetInfo::Token { contract_addr } => contract_addr,
        _ => return Err(StdError::generic_err("unauthorized")),
    };

    provide_asset.assert_sent_native_token_balance(&info)?;

    let asset_infos = [provide_asset.info.clone(), pair_asset];

    let bond_amount = provide_asset.amount.multiply_ratio(1u128, 2u128);
    let bond_asset = Asset {
        info: provide_asset.info.clone(),
        amount: bond_amount,
    };
    let tax_amount = bond_asset.compute_tax(&deps.querier)?;
    let swap_asset = Asset {
        info: provide_asset.info,
        amount: bond_amount.checked_sub(tax_amount)?,
    };

    let prev_asset_token_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&asset_token)?,
        env.contract.address.clone(),
    )?;

    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: terraswap_pair.contract_addr,
                msg: to_binary(&PairExecuteMsg::Swap {
                    offer_asset: swap_asset.clone(),
                    max_spread: Some(max_spread),
                    belief_price,
                    to: None,
                })?,
                funds: vec![Coin {
                    denom,
                    amount: swap_asset.amount,
                }],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&ExecuteMsg::zap_to_bond_hook {
                    contract,
                    bond_asset,
                    asset_token: asset_token.clone(),
                    staker_addr: info.sender.to_string(),
                    prev_asset_token_amount,
                    slippage_tolerance: max_spread,
                    compound_rate,
                })?,
                funds: vec![],
            }),
        ])
        .add_attributes(vec![
            attr("action", "zap_to_bond"),
            attr("asset_token", asset_token),
            attr("provide_amount", provide_asset.amount),
        ]))
}

#[allow(clippy::too_many_arguments)]
fn zap_to_bond_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    bond_asset: Asset,
    asset_token: String,
    staker_addr: String,
    prev_asset_token_amount: Uint128,
    slippage_tolerance: Decimal,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    // only can be called by itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    // stake all lp tokens received, compare with staking token amount before liquidity provision was executed
    let current_asset_token_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&asset_token)?,
        env.contract.address.clone(),
    )?;
    let amount_to_bond = current_asset_token_amount.checked_sub(prev_asset_token_amount)?;

    Ok(
        Response::new().add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::bond {
                contract,
                assets: [
                    bond_asset,
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: asset_token,
                        },
                        amount: amount_to_bond,
                    },
                ],
                slippage_tolerance,
                compound_rate,
                staker_addr: Some(staker_addr),
            })?,
            funds: vec![],
        })]),
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
        terraswap_factory: deps
            .api
            .addr_humanize(&config.terraswap_factory)?
            .to_string(),
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
