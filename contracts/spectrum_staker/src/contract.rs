#![allow(clippy::assign_op_pattern)]
#![allow(clippy::ptr_offset_with_cast)]

use std::collections::HashSet;
use std::iter::FromIterator;

use crate::state::{config_store, read_config, Config};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, from_binary, to_binary, BankMsg, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, QueryRequest, WasmQuery};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use spectrum_protocol::mirror_farm::Cw20HookMsg as MirrorCw20HookMsg;
use spectrum_protocol::staker::{ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg, SimulateZapToBondResponse};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, PoolResponse, QueryMsg as PairQueryMsg};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};
use terraswap::router::{
    SwapOperation, Cw20HookMsg as TerraswapRouterCw20HookMsg,
};
use uint::construct_uint;

construct_uint! {
	pub struct U256(4);
}

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
        terraswap_factory: deps.api.addr_canonicalize(&msg.terraswap_factory)?,
        terraswap_router: deps.api.addr_validate(&msg.terraswap_router)?.to_string(),
        allowlist: HashSet::from_iter(allowlist),
    })?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::receive(msg) => receive_cw20(deps, env, info, msg),
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
            pair_asset_b,
            belief_price,
            belief_price_b,
            max_spread,
            compound_rate,
        } => zap_to_bond(
            deps,
            env,
            info,
            contract,
            provide_asset,
            pair_asset,
            pair_asset_b,
            belief_price,
            belief_price_b,
            max_spread,
            compound_rate,
        ),
        ExecuteMsg::update_config {
            insert_allowlist,
            remove_allowlist,
        } => update_config(deps, info, insert_allowlist, remove_allowlist),
        ExecuteMsg::zap_to_unbond_hook {
            staker_addr,
            prev_asset_a,
            prev_asset_b,
            belief_price,
            max_spread,
            minimum_receive,
        } => zap_to_unbond_hook(
            deps,
            env,
            info,
            staker_addr,
            prev_asset_a,
            prev_asset_b,
            belief_price,
            max_spread,
            minimum_receive,
        ),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::zap_to_unbond {
            sell_asset,
            target_asset,
            belief_price,
            max_spread,
            minimum_receive,
        }) => zap_to_unbond(
            deps,
            env,
            info,
            cw20_msg.sender,
            cw20_msg.amount,
            sell_asset,
            target_asset,
            belief_price,
            max_spread,
            minimum_receive,
        ),
        Err(_) => Err(StdError::generic_err("data should be given")),
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

    validate_contract(contract_raw, &config.allowlist)?;

    let mut native_asset_op: Option<Asset> = None;
    let mut token_a_op: Option<(String, Uint128)> = None;
    let mut token_b_op: Option<(String, Uint128)> = None;
    for asset in assets.iter() {
        match asset.info.clone() {
            AssetInfo::Token { contract_addr } => {
                if token_a_op.is_none() {
                    token_a_op = Some((contract_addr, asset.amount));
                } else {
                    token_b_op = Some((contract_addr, asset.amount));
                }
            }
            AssetInfo::NativeToken { .. } => {
                if info.sender != env.contract.address {
                    asset.assert_sent_native_token_balance(&info)?;
                }
                native_asset_op = Some(asset.clone());
            }
        }
    }

    // will fail if one of them is missing
    let (token_a_addr, token_a_amount) = match token_a_op {
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

    // 1. Transfer token asset to staking contract
    // 2. Increase allowance of token for pair contract
    // 3. Provide liquidity
    // 4. Execute staking hook, will stake in the name of the sender

    let staker = staker_addr.unwrap_or_else(|| info.sender.to_string());

    let mut messages: Vec<CosmosMsg> = vec![];
    if info.sender != env.contract.address {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: token_a_addr.clone(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: staker.clone(),
                recipient: env.contract.address.to_string(),
                amount: token_a_amount,
            })?,
            funds: vec![],
        }));
    }
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_a_addr.clone(),
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pair.contract_addr.clone(),
            amount: token_a_amount,
            expires: None,
        })?,
        funds: vec![],
    }));

    if let Some((token_b_addr, token_b_amount)) = token_b_op {
        if info.sender != env.contract.address {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_b_addr.clone(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: staker.clone(),
                    recipient: env.contract.address.to_string(),
                    amount: token_b_amount,
                })?,
                funds: vec![],
            }));
        }
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: token_b_addr.clone(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: terraswap_pair.contract_addr.clone(),
                amount: token_b_amount,
                expires: None,
            })?,
            funds: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair.contract_addr,
            msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                assets: assets.clone(),
                slippage_tolerance: Some(slippage_tolerance),
                receiver: None,
            })?,
            funds: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::bond_hook {
                contract,
                asset_token: token_b_addr,
                staking_token: terraswap_pair.liquidity_token,
                staker_addr: staker,
                prev_staking_token_amount,
                compound_rate,
            })?,
            funds: vec![],
        }));
    } else if let Some(native_asset) = native_asset_op {
        let tax_amount = native_asset.compute_tax(&deps.querier)?;
        let native_asset = Asset {
            amount: native_asset.amount.checked_sub(tax_amount)?,
            info: native_asset.info,
        };
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
                asset_token: token_a_addr,
                staking_token: terraswap_pair.liquidity_token,
                staker_addr: staker,
                prev_staking_token_amount,
                compound_rate,
            })?,
            funds: vec![],
        }));
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "bond"),
        attr("asset_token_a", assets[0].info.to_string()),
        attr("asset_token_b", assets[1].info.to_string()),
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
                msg: to_binary(&MirrorCw20HookMsg::bond {
                    asset_token,
                    staker_addr: Some(staker_addr),
                    compound_rate,
                })?,
            })?,
            funds: vec![],
        })]),
    )
}

pub(crate) fn compute_swap_amount(
    amount_a: Uint128,
    amount_b: Uint128,
    pool_a: Uint128,
    pool_b: Uint128,
) -> Uint128 {
    let amount_a = U256::from(amount_a.u128());
    let amount_b = U256::from(amount_b.u128());
    let pool_a = U256::from(pool_a.u128());
    let pool_b = U256::from(pool_b.u128());

    let pool_ax = amount_a + pool_a;
    let pool_bx = amount_b + pool_b;
    let area_ax = pool_ax * pool_b;
    let area_bx = pool_bx * pool_a;

    let a = U256::from(9) * area_ax + U256::from(3988000) * area_bx;
    let b = U256::from(3) * area_ax + area_ax.integer_sqrt() * a.integer_sqrt();
    let result = b / U256::from(2000) / pool_bx - pool_a;

    result.as_u128().into()
}

fn get_swap_amount(
    pool: &PoolResponse,
    asset: &Asset,
) -> Uint128 {
    if pool.assets[0].info == asset.info {
        compute_swap_amount(asset.amount, Uint128::zero(), pool.assets[0].amount, pool.assets[1].amount)
    } else {
        compute_swap_amount(asset.amount, Uint128::zero(), pool.assets[1].amount, pool.assets[0].amount)
    }
}

fn apply_pool(
    pool: &mut PoolResponse,
    swap_asset: &Asset,
    return_amount: Uint128,
) {
    if pool.assets[0].info == swap_asset.info {
        pool.assets[0].amount += swap_asset.amount;
        pool.assets[1].amount -= return_amount;
    } else {
        pool.assets[1].amount += swap_asset.amount;
        pool.assets[0].amount -= return_amount;
    }
}

#[allow(clippy::too_many_arguments)]
fn zap_to_bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    provide_asset: Asset,
    pair_asset_a: AssetInfo,
    pair_asset_b: Option<AssetInfo>,
    belief_price_a: Option<Decimal>,
    belief_price_b: Option<Decimal>,
    max_spread: Decimal,
    compound_rate: Option<Decimal>,
) -> StdResult<Response> {
    validate_slippage(max_spread)?;
    provide_asset.assert_sent_native_token_balance(&info)?;

    let config = read_config(deps.storage)?;
    let contract_raw = deps.api.addr_canonicalize(contract.as_str())?;

    validate_contract(contract_raw, &config.allowlist)?;

    let (messages, _, _) = compute_zap_to_bond(
        deps.as_ref(),
        env,
        &config,
        contract,
        provide_asset.clone(),
        pair_asset_a.clone(),
        pair_asset_b.clone(),
        belief_price_a,
        belief_price_b,
        Some(max_spread),
        compound_rate,
        Some(info.sender.to_string()),
    )?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "zap_to_bond"),
            attr("asset_token_a", pair_asset_a.to_string()),
            attr("asset_token_b", pair_asset_b.unwrap_or_else(|| provide_asset.info.clone()).to_string()),
            attr("provide_amount", provide_asset.amount),
        ]))
}

#[allow(clippy::too_many_arguments)]
fn compute_zap_to_bond(
    deps: Deps,
    env: Env,
    config: &Config,
    contract: String,
    provide_asset: Asset,
    pair_asset_a: AssetInfo,
    pair_asset_b: Option<AssetInfo>,
    belief_price_a: Option<Decimal>,
    belief_price_b: Option<Decimal>,
    max_spread: Option<Decimal>,
    compound_rate: Option<Decimal>,
    staker_addr: Option<String>,
) -> StdResult<(Vec<CosmosMsg>, [Asset; 2], PoolResponse)> {
    let denom = match provide_asset.info.clone() {
        AssetInfo::NativeToken { denom } => denom,
        _ => return Err(StdError::generic_err("not support provide_asset as token")),
    };
    let asset_token_a = match pair_asset_a.clone() {
        AssetInfo::Token { contract_addr } => contract_addr,
        _ => return Err(StdError::generic_err("not support pair_asset as native coin")),
    };
    if let Some(AssetInfo::NativeToken { .. }) = pair_asset_b {
        return Err(StdError::generic_err("not support pair_asset_b as native coin"));
    }

    // if asset b is provided, swap all
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let asset_pair_a = [provide_asset.info.clone(), pair_asset_a.clone()];
    let terraswap_pair_a = query_pair_info(&deps.querier, terraswap_factory.clone(), &asset_pair_a)?;
    let (swap_amount, pair_contract, pool) = if let Some(pair_asset_b) = pair_asset_b.clone() {
        let asset_pair_b = [pair_asset_a.clone(), pair_asset_b];
        let terraswap_pair_b = query_pair_info(&deps.querier, terraswap_factory, &asset_pair_b)?;
        let pool: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: terraswap_pair_b.contract_addr.clone(),
            msg: to_binary(&PairQueryMsg::Pool {})?,
        }))?;
        (provide_asset.amount, terraswap_pair_b.contract_addr, pool)
    } else {
        let pool: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: terraswap_pair_a.contract_addr.clone(),
            msg: to_binary(&PairQueryMsg::Pool {})?,
        }))?;
        let swap_amount = get_swap_amount(&pool, &provide_asset);
        (swap_amount, terraswap_pair_a.contract_addr.clone(), pool)
    };
    let mut pool = pool;
    let swap_asset = Asset {
        info: provide_asset.info.clone(),
        amount: swap_amount,
    };
    let mut bond_asset = Asset {
        info: provide_asset.info.clone(),
        amount: provide_asset.amount.checked_sub(swap_asset.amount)?,
    };
    let tax_amount = swap_asset.compute_tax(&deps.querier)?;
    let swap_asset = Asset {
        info: provide_asset.info,
        amount: swap_amount.checked_sub(tax_amount)?,
    };

    // swap ust -> A
    let simulate_a = simulate(
        &deps.querier,
        deps.api.addr_validate(&terraswap_pair_a.contract_addr)?,
        &swap_asset)?;
    if pair_asset_b.is_none() {
        apply_pool(&mut pool, &swap_asset, simulate_a.return_amount);
    }
    let mut amount_a = simulate_a.return_amount;
    let mut messages: Vec<CosmosMsg> = vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair_a.contract_addr,
            msg: to_binary(&PairExecuteMsg::Swap {
                offer_asset: swap_asset.clone(),
                max_spread,
                belief_price: belief_price_a,
                to: None,
            })?,
            funds: vec![Coin {
                denom,
                amount: swap_asset.amount,
            }],
        }),
    ];

    if let Some(pair_asset_b) = pair_asset_b {
        let swap_asset_a = Asset {
            info: pair_asset_a.clone(),
            amount: amount_a,
        };
        let swap_asset_a = Asset {
            info: pair_asset_a,
            amount: get_swap_amount(&pool, &swap_asset_a),
        };
        amount_a = amount_a.checked_sub(swap_asset_a.amount)?;
        let simulate_b = simulate(
            &deps.querier,
            deps.api.addr_validate(&pair_contract)?,
            &swap_asset_a)?;
        bond_asset = Asset {
            info: pair_asset_b,
            amount: simulate_b.return_amount,
        };
        apply_pool(&mut pool, &swap_asset_a, simulate_b.return_amount);
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract,
            msg: to_binary(&PairExecuteMsg::Swap {
                offer_asset: swap_asset_a,
                max_spread,
                belief_price: belief_price_b,
                to: None,
            })?,
            funds: vec![],
        }));
    }

    let assets = [Asset {
        info: AssetInfo::Token {
            contract_addr: asset_token_a,
        },
        amount: amount_a,
    }, bond_asset];
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::bond {
            contract,
            assets: assets.clone(),
            slippage_tolerance: max_spread.unwrap_or_else(|| Decimal::percent(50)),
            compound_rate,
            staker_addr,
        })?,
        funds: vec![],
    }));

    Ok((messages, assets, pool))
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

#[allow(clippy::too_many_arguments)]
fn zap_to_unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    staker_addr: String,
    amount: Uint128,
    sell_asset: AssetInfo,
    target_asset: AssetInfo,
    belief_price: Option<Decimal>,
    max_spread: Decimal,
    minimum_receive: Option<Uint128>,
) -> StdResult<Response> {
    validate_slippage(max_spread)?;

    let asset_token = match sell_asset.clone() {
        AssetInfo::Token { contract_addr } => contract_addr,
        _ => return Err(StdError::generic_err("not support sell_asset as native coin")),
    };

    let config = read_config(deps.storage)?;
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let asset_infos = [target_asset.clone(), sell_asset.clone()];
    let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;

    if terraswap_pair.liquidity_token != info.sender {
        return Err(StdError::generic_err("invalid lp token"));
    }

    let current_token_a_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&asset_token)?,
        env.contract.address.clone(),
    )?;

    let current_token_b_amount = match target_asset.clone() {
        AssetInfo::NativeToken { denom } => deps
            .querier
            .query_balance(env.contract.address.to_string(), denom)?
            .amount,
        AssetInfo::Token { contract_addr } => query_token_balance(
            &deps.querier,
            deps.api.addr_validate(&contract_addr)?,
            env.contract.address.clone())?
    };

    Ok(Response::new().add_messages(vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraswap_pair.liquidity_token,
            msg: to_binary(&Cw20ExecuteMsg::Send {
                amount,
                contract: terraswap_pair.contract_addr,
                msg: to_binary(&PairCw20HookMsg::WithdrawLiquidity {})?,
            })?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::zap_to_unbond_hook {
                staker_addr,
                prev_asset_a: Asset {
                    amount: current_token_a_amount,
                    info: sell_asset,
                },
                prev_asset_b: Asset {
                    amount: current_token_b_amount,
                    info: target_asset,
                },
                belief_price,
                max_spread,
                minimum_receive,
            })?,
            funds: vec![],
        }),
    ]))
}

#[allow(clippy::too_many_arguments)]
fn zap_to_unbond_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    staker_addr: String,
    prev_asset_a: Asset,
    prev_asset_b: Asset,
    belief_price: Option<Decimal>,
    max_spread: Decimal,
    minimum_receive: Option<Uint128>,
) -> StdResult<Response> {
    // only can be called by itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let asset_token = match prev_asset_a.info.clone() {
        AssetInfo::Token { contract_addr } => contract_addr,
        _ => return Err(StdError::generic_err("not support sell_asset as native coin")),
    };

    let current_token_a_amount = query_token_balance(
        &deps.querier,
        deps.api.addr_validate(&asset_token)?,
        env.contract.address.clone(),
    )?;

    let config = read_config(deps.storage)?;
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    match prev_asset_b.info.clone() {
        AssetInfo::NativeToken { denom } => {
            let asset_infos = [prev_asset_b.info.clone(), prev_asset_a.info];
            let terraswap_pair = query_pair_info(&deps.querier, terraswap_factory, &asset_infos)?;
            let current_token_b_amount = deps
                .querier
                .query_balance(env.contract.address.to_string(), denom.clone())?
                .amount;
            let transfer_asset = Asset {
                info: prev_asset_b.info.clone(),
                amount: current_token_b_amount.checked_sub(prev_asset_b.amount)?,
            };
            let tax_amount = transfer_asset.compute_tax(&deps.querier)?;
            Ok(Response::new().add_messages(vec![
                CosmosMsg::Bank(BankMsg::Send {
                    to_address: staker_addr.clone(),
                    amount: vec![Coin {
                        denom,
                        amount: transfer_asset.amount.checked_sub(tax_amount)?,
                    }],
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: asset_token,
                    msg: to_binary(&Cw20ExecuteMsg::Send {
                        contract: terraswap_pair.contract_addr,
                        amount: current_token_a_amount.checked_sub(prev_asset_a.amount)?,
                        msg: to_binary(&PairCw20HookMsg::Swap {
                            to: Some(staker_addr),
                            belief_price,
                            max_spread: Some(max_spread),
                        })?,
                    })?,
                    funds: vec![],
                }),
            ]))
        },
        AssetInfo::Token { contract_addr } => {
            let uusd_info = AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            };
            let asset_infos_a = [uusd_info.clone(), prev_asset_a.info.clone()];
            let terraswap_pair_a = query_pair_info(&deps.querier, terraswap_factory, &asset_infos_a)?;
            let current_token_b_amount = query_token_balance(
                &deps.querier,
                deps.api.addr_validate(&contract_addr)?,
                env.contract.address)?;
            Ok(Response::new().add_messages(vec![
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: asset_token,
                    msg: to_binary(&Cw20ExecuteMsg::Send {
                        contract: terraswap_pair_a.contract_addr,
                        amount: current_token_a_amount.checked_sub(prev_asset_a.amount)?,
                        msg: to_binary(&PairCw20HookMsg::Swap {
                            to: Some(staker_addr.clone()),
                            belief_price,
                            max_spread: Some(max_spread),
                        })?,
                    })?,
                    funds: vec![],
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg: to_binary(&Cw20ExecuteMsg::Send {
                        contract: config.terraswap_router,
                        amount: current_token_b_amount.checked_sub(prev_asset_b.amount)?,
                        msg: to_binary(&TerraswapRouterCw20HookMsg::ExecuteSwapOperations {
                            operations: vec![
                                SwapOperation::TerraSwap {
                                    offer_asset_info: prev_asset_b.info,
                                    ask_asset_info: prev_asset_a.info.clone(),
                                },
                                SwapOperation::TerraSwap {
                                    offer_asset_info: prev_asset_a.info,
                                    ask_asset_info: uusd_info,
                                },
                            ],
                            minimum_receive,
                            to: Some(staker_addr),
                        })?,
                    })?,
                    funds: vec![],
                }),
            ]))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::simulate_zap_to_bond {
            provide_asset,
            pair_asset,
            pair_asset_b,
        } => to_binary(&simulate_zap_to_bond(deps, env, provide_asset, pair_asset, pair_asset_b)?),
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
        terraswap_router: config.terraswap_router,
        allowlist: config
            .allowlist
            .into_iter()
            .map(|w| deps.api.addr_humanize(&w).map(|addr| addr.to_string()))
            .collect::<StdResult<Vec<String>>>()?,
    };

    Ok(resp)
}

fn simulate_zap_to_bond(
    deps: Deps,
    env: Env,
    provide_asset: Asset,
    pair_asset_a: AssetInfo,
    pair_asset_b: Option<AssetInfo>,
) -> StdResult<SimulateZapToBondResponse> {
    let config = read_config(deps.storage)?;

    let (_, [asset_a, asset_b], pool) = compute_zap_to_bond(
        deps,
        env,
        &config,
        "".to_string(),
        provide_asset,
        pair_asset_a,
        pair_asset_b,
        None,
        None,
        None,
        None,
        None,
    )?;

    let (pool_a, pool_b) = if pool.assets[0].info.clone() == asset_a.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };
    let lp_amount = std::cmp::min(
        asset_a.amount.multiply_ratio(pool.total_share, pool_a),
        asset_b.amount.multiply_ratio(pool.total_share, pool_b),
    );

    Ok(SimulateZapToBondResponse {
        lp_amount,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    config.terraswap_router = msg.terraswap_router;
    config_store(deps.storage).save(&config)?;
    Ok(Response::default())
}
