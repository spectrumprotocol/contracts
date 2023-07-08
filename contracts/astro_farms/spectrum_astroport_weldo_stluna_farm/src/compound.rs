#![allow(clippy::assign_op_pattern)]
#![allow(clippy::ptr_offset_with_cast)]

use classic_bindings::TerraQuery;
use cosmwasm_std::{attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, DepsMut, Env, MessageInfo, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery, Decimal, QuerierWrapper, Addr};

use crate::{
    bond::deposit_farm_share,
    querier::{query_astroport_pending_token, astroport_router_simulate_swap},
    state::{read_config, state_store}, model::ExecuteMsg,
};

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use astroport::asset::{Asset, AssetInfo};
use astroport::generator::{
    Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg
};
use astroport::pair::{Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg, PoolResponse, QueryMsg as AstroportPairQueryMsg};
use astroport::router::{SwapOperation, ExecuteMsg as AstroportRouterExecuteMsg};
use astroport::querier::{query_token_balance, simulate};
use moneymarket::market::ExecuteMsg as MoneyMarketExecuteMsg;
use spectrum_protocol::farm_helper::{deduct_tax};
use spectrum_protocol::gov_proxy::Cw20HookMsg as GovProxyCw20HookMsg;
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use crate::bond::deposit_farm2_share;
use uint::construct_uint;

construct_uint! {
    pub struct U256(4);
}

// weLDO -?%> stluna
// astro -> ust -> luna -> stluna -?%-> weldo
// ?% = resolve with optimal swap

// astro -> ust 8%
// astro -> ust -> luna -> stluna 92%

pub fn compound(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    threshold_compound_astro: Uint128,
) -> StdResult<Response> {

    return Err(StdError::generic_err("function disabled"));

    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let stluna_token = deps.api.addr_humanize(&config.stluna_token)?;
    let weldo_token =  deps.api.addr_humanize(&config.weldo_token)?;
    let astro_token = deps.api.addr_humanize(&config.astro_token)?;
    let xastro_proxy = deps.api.addr_humanize(&config.xastro_proxy)?;
    let astro_ust_pair_contract = deps.api.addr_humanize(&config.astro_ust_pair_contract)?;
    let astroport_router = deps.api.addr_humanize(&config.astroport_router)?;

    let uluna = "uluna".to_string();
    let uusd = "uusd".to_string();

    let mut pool_info = pool_info_read(deps.storage).load(config.stluna_token.as_slice())?;

    // This get pending (ASTRO), and pending proxy rewards
    let pending_token_response = query_astroport_pending_token(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator
    )?;

    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;
    let lp_balance = query_token_balance(&deps.querier, staking_token, env.contract.address.clone())?;

    let mut total_weldo_token_stake_amount = Uint128::zero();
    let mut total_weldo_token_commission = Uint128::zero();
    let mut total_astro_token_swap_amount = Uint128::zero();
    let mut total_astro_token_stake_amount = Uint128::zero();
    let mut total_astro_token_commission = Uint128::zero();
    let mut compound_amount_astro = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    let reward = query_token_balance(&deps.querier, weldo_token.clone(), env.contract.address.clone())? + pending_token_response.pending_on_proxy.unwrap_or_else(Uint128::zero);
    let reward_astro = query_token_balance(&deps.querier, astro_token.clone(), env.contract.address.clone())? + pending_token_response.pending;

    // calculate auto-compound, auto-stake, and commission in astro token
    let mut state = read_state(deps.storage)?;
    if !reward_astro.is_zero() && !lp_balance.is_zero() && reward_astro > threshold_compound_astro {
        let commission_astro = reward_astro * total_fee;
        let astro_amount = reward_astro.checked_sub(commission_astro)?;
        // add commission to total swap amount
        total_astro_token_commission += commission_astro;
        total_astro_token_swap_amount += commission_astro;

        let auto_bond_amount_astro = lp_balance.checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount_astro = astro_amount.multiply_ratio(auto_bond_amount_astro, lp_balance);
        let stake_amount_astro = astro_amount.checked_sub(compound_amount_astro)?;

        attributes.push(attr("commission_astro", commission_astro));
        attributes.push(attr("compound_amount_astro", compound_amount_astro));
        attributes.push(attr("stake_amount_astro", stake_amount_astro));

        total_astro_token_stake_amount += stake_amount_astro;

        deposit_farm_share(
            deps.as_ref(),
            &env,
            &mut state,
            &mut pool_info,
            &config,
            total_astro_token_stake_amount,
        )?;
    }

    // calculate auto-compound, auto-stake, and commission in farm token
    if !reward.is_zero() && !lp_balance.is_zero() {
        let commission = reward * total_fee;
        let weldo_token_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_weldo_token_commission += commission;

        let auto_bond_amount = lp_balance.checked_sub(pool_info.total_stake_bond_amount)?;
        let compound_amount = weldo_token_amount.multiply_ratio(auto_bond_amount, lp_balance);
        let stake_amount = weldo_token_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_weldo_token_stake_amount += stake_amount;

        deposit_farm2_share(
            deps.as_ref(),
            &env,
            &mut state,
            &mut pool_info,
            &config,
            total_weldo_token_stake_amount,
        )?;
    }
    state_store(deps.storage).save(&state)?;
    pool_info_store(deps.storage).save(config.stluna_token.as_slice(), &pool_info)?;

    // swap all
    total_astro_token_swap_amount += compound_amount_astro;
    let total_ust_commission_amount_astro = if !total_astro_token_swap_amount.is_zero() {

        // find ASTRO swap rate
        let astro_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: astro_token.clone(),
            },
            amount: total_astro_token_swap_amount,
        };
        let astro_swap_rate = simulate(&deps.querier, astro_ust_pair_contract.clone(), &astro_asset)?;

        let total_ust_return_amount_astro = deduct_tax(
            &deps.querier,
            astro_swap_rate.return_amount,
            uusd.clone(),
        )?;
        attributes.push(attr("total_ust_return_amount_astro", total_ust_return_amount_astro));

        total_ust_return_amount_astro
            .multiply_ratio(total_astro_token_commission, total_astro_token_swap_amount)
    } else {
        Uint128::zero()
    };

    // get rate weldo -> stluna -> uluna -> ust for commission
    let weldo_ust_swap_rate = astroport_router_simulate_swap(
        deps.as_ref(),
        total_weldo_token_commission,
        vec![
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::Token { contract_addr: weldo_token.clone() },
                ask_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
            },
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
                ask_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
            },
            SwapOperation::AstroSwap {
                offer_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
                ask_asset_info: AssetInfo::NativeToken { denom: uusd.clone() },
            }
        ],
        &config.astroport_router
    )?;

    let ust_commission_from_weldo_amount = deduct_tax(
        &deps.querier,
        weldo_ust_swap_rate.amount,
        uusd.clone(),
    )?;
    attributes.push(attr("ust_commission_from_weldo_amount", ust_commission_from_weldo_amount));

    let mut messages: Vec<CosmosMsg> = vec![];

    let manual_claim_pending_token = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.addr_humanize(&config.astroport_generator)?.to_string(),
        funds: vec![],
        msg: to_binary(&AstroportExecuteMsg::Withdraw {
            lp_token: deps.api.addr_humanize(&pool_info.staking_token)?,
            amount: Uint128::zero(),
        })?,
    });
    messages.push(manual_claim_pending_token);

    if !total_weldo_token_commission.is_zero() {
        let ust_amount = deps.querier.query_balance(env.contract.address.clone(), "uusd")?.amount;
        if ust_amount < ust_commission_from_weldo_amount {
            let swap_weldo_to_ust: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: weldo_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: astroport_router.to_string(),
                    amount: total_weldo_token_commission,
                    msg: to_binary(&AstroportRouterExecuteMsg::ExecuteSwapOperations {
                        operations: vec![
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::Token { contract_addr: weldo_token.clone() },
                                ask_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
                            },
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
                                ask_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
                            },
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::NativeToken { denom: uluna },
                                ask_asset_info: AssetInfo::NativeToken { denom: uusd.clone() },
                            },
                        ],
                        minimum_receive: None,
                        to: None,
                        max_spread: Some(Decimal::percent(50))
                    })?,
                })?,
                funds: vec![],
            });
            messages.push(swap_weldo_to_ust);
        }
    }

    if !total_astro_token_swap_amount.is_zero() {
        //swap 100% astro to uusd
        let swap_astro_to_uusd: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astro_ust_pair_contract.to_string(),
                amount: total_astro_token_swap_amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_astro_to_uusd);
    }

    if let Some(gov_proxy) = config.gov_proxy {
        if !total_weldo_token_stake_amount.is_zero() {
            let stake_weldo_token = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: weldo_token.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: deps.api.addr_humanize(&gov_proxy)?.to_string(),
                    amount: total_weldo_token_stake_amount,
                    msg: to_binary(&GovProxyCw20HookMsg::Stake {})?,
                })?,
            });
            messages.push(stake_weldo_token);
        }
    }

    if !total_astro_token_stake_amount.is_zero() {
        let stake_astro_token = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astro_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: xastro_proxy.to_string(),
                amount: total_astro_token_stake_amount,
                msg: to_binary(&GovProxyCw20HookMsg::Stake {})?,
            })?,
        });
        messages.push(stake_astro_token);
    }

    let total_ust_commission_amount = ust_commission_from_weldo_amount + total_ust_commission_amount_astro;
    if !total_ust_commission_amount.is_zero() {
        let net_commission_amount = deduct_tax(
            &deps.querier,
            total_ust_commission_amount,
            uusd.clone(),
        )?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission_amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", net_commission_amount));

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
            msg: to_binary(&MoneyMarketExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: uusd,
                amount: net_commission_amount,
            }],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&GovExecuteMsg::mint {})?,
            funds: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::send_fee {})?,
            funds: vec![],
        }));
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", &stluna_token));

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

pub fn compute_provide_after_swap(
    pool: &PoolResponse,
    offer: &Asset,
    return_amt: Uint128,
    ask_reinvest_amt: Uint128,
) -> StdResult<Uint128> {
    let (offer_amount, ask_amount) = if pool.assets[0].info == offer.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };

    let offer_amount = offer_amount + offer.amount;
    let ask_amount = ask_amount.checked_sub(return_amt)?;

    Ok(ask_reinvest_amt.multiply_ratio(offer_amount, ask_amount))
}

pub fn stake(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
    asset_token: String,
) -> StdResult<Response> {
    // only anchor farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let astroport_generator = deps.api.addr_humanize(&config.astroport_generator)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_generator.to_string(),
                amount,
                msg: to_binary(&AstroportCw20HookMsg::Deposit {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("asset_token", asset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}

pub fn send_fee(
    deps: DepsMut<TerraQuery>,
    env: Env,
    info: MessageInfo,
) -> StdResult<Response> {

    // only farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config = read_config(deps.storage)?;
    let aust_token = deps.api.addr_humanize(&config.aust_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;
    let stluna_token = deps.api.addr_humanize(&config.stluna_token)?;
    let weldo_token =  deps.api.addr_humanize(&config.weldo_token)?;
    let pair_contract = deps.api.addr_humanize(&config.pair_contract)?;
    let astroport_router = deps.api.addr_humanize(&config.astroport_router)?;

    let aust_balance = query_token_balance(&deps.querier, aust_token.clone(), env.contract.address.clone())?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let thousand = Uint128::from(1000u64);
    let total_fee = config.community_fee + config.controller_fee + config.platform_fee;
    let community_amount = aust_balance.multiply_ratio(thousand * config.community_fee, thousand * total_fee);
    if !community_amount.is_zero() {
        let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: spectrum_gov.to_string(),
                amount: community_amount,
            })?,
            funds: vec![],
        });
        messages.push(transfer_community_fee);
    }

    let platform_amount = aust_balance.multiply_ratio(thousand * config.platform_fee, thousand * total_fee);
    if !platform_amount.is_zero() {
        let stake_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&config.platform)?.to_string(),
                amount: platform_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_platform_fee);
    }

    let controller_amount = aust_balance.checked_sub(community_amount + platform_amount)?;
    if !controller_amount.is_zero() {
        let stake_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&config.controller)?.to_string(),
                amount: controller_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_controller_fee);
    }

    let weldo_amount = query_token_balance(&deps.querier, weldo_token.clone(), env.contract.address.clone())?;
    let stluna_amount = query_token_balance(&deps.querier, stluna_token.clone(), env.contract.address.clone())?;

    let weldo_asset_info = AssetInfo::Token {
        contract_addr: weldo_token.clone(),
    };
    let sttoken_asset_info = AssetInfo::Token {
        contract_addr: stluna_token.clone(),
    };
    let (weldo_amount_to_be_swapped, stluna_amount_to_be_swapped, weldo_return_from_optimal_swap, stluna_return_from_optimal_swap) = optimal_swap(
        &deps.querier,
        weldo_amount,
        stluna_amount,
        weldo_asset_info,
        sttoken_asset_info,
        pair_contract.clone(),
        &mut messages
    )?;

    let provide_weldo = weldo_amount.checked_sub(weldo_amount_to_be_swapped)? + weldo_return_from_optimal_swap;
    let provide_stluna = stluna_amount.checked_sub(stluna_amount_to_be_swapped)? + stluna_return_from_optimal_swap;

    if !provide_weldo.is_zero() && !provide_stluna.is_zero() {
        let increase_allowance_weldo = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: weldo_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_contract.to_string(),
                amount: provide_weldo,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance_weldo);

        let increase_allowance_stluna = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: stluna_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_contract.to_string(),
                amount: provide_stluna,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance_stluna);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: weldo_token,
                        },
                        amount: provide_weldo,
                    },
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: stluna_token.clone(),
                        },
                        amount: provide_stluna,
                    },
                ],
                slippage_tolerance: None,
                receiver: None,
                auto_stake: Some(true),
            })?,
            funds: vec![],
        });
        messages.push(provide_liquidity);

        // let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        //     contract_addr: env.contract.address.to_string(),
        //     msg: to_binary(&ExecuteMsg::stake {
        //         asset_token: farm_token.to_string(),
        //     })?,
        //     funds: vec![],
        // });
        // messages.push(stake);
    }

    let ust_amount = deps.querier.query_balance(env.contract.address, "uusd")?.amount;
    if ust_amount >= Uint128::from(100_000000u128) {
        let ust_after_tax = deduct_tax(&deps.querier, ust_amount, "uusd".to_string())?;

        //swap uusd exclude commission to stluna
        let swap_uusd_to_stluna: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astroport_router.to_string(),
            msg: to_binary(&AstroportRouterExecuteMsg::ExecuteSwapOperations {
                operations: vec![
                    SwapOperation::AstroSwap {
                        offer_asset_info: AssetInfo::NativeToken { denom: "uusd".to_string() },
                        ask_asset_info: AssetInfo::NativeToken { denom: "uluna".to_string() },
                    },
                    SwapOperation::AstroSwap {
                        offer_asset_info: AssetInfo::NativeToken { denom: "uluna".to_string() },
                        ask_asset_info: AssetInfo::Token { contract_addr: stluna_token },
                    },
                ],
                minimum_receive: None,
                to: None,
                max_spread: Some(Decimal::percent(50))
            })?,
            funds: vec![
                Coin { denom: "uusd".to_string(), amount: ust_after_tax }
            ],
        });
        messages.push(swap_uusd_to_stluna);
    }

    Ok(Response::new()
        .add_messages(messages))
}

/// Query the Astroport pool, parse response, and return the following 3-tuple:
/// 1. depth of the primary asset
/// 2. depth of the secondary asset
/// 3. total supply of the share token
fn query_pool(
    pair_contract: String,
    querier: &QuerierWrapper<TerraQuery>,
    primary_asset_info: &AssetInfo,
    secondary_asset_info: &AssetInfo,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    let response: PoolResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract,
        msg: to_binary(&AstroportPairQueryMsg::Pool {})?,
    }))?;

    let primary_asset_depth = response
        .assets
        .iter()
        .find(|asset| &asset.info == primary_asset_info)
        .ok_or_else(|| StdError::generic_err("Cannot find primary asset in pool response"))?
        .amount;

    let secondary_asset_depth = response
        .assets
        .iter()
        .find(|asset| &asset.info == secondary_asset_info)
        .ok_or_else(|| StdError::generic_err("Cannot find secondary asset in pool response"))?
        .amount;

    Ok((primary_asset_depth, secondary_asset_depth, response.total_share))
}

/// @notice Generate msg for swapping specified asset
fn swap_msg(pair_contract: String, asset: &Asset, belief_price: Option<Decimal>, max_spread: Option<Decimal>, to: Option<String>) -> StdResult<CosmosMsg> {
    let wasm_msg = match &asset.info {
        AssetInfo::Token {
            contract_addr,
        } => WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract,
                amount: asset.amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    belief_price,
                    max_spread,
                    to,
                })?,
            })?,
            funds: vec![],
        },

        AssetInfo::NativeToken {
            denom,
        } => WasmMsg::Execute {
            contract_addr: pair_contract,
            msg: to_binary(&AstroportPairExecuteMsg::Swap {
                offer_asset: asset.clone(),
                belief_price,
                max_spread,
                to: None,
            })?,
            funds: vec![Coin {
                denom: denom.clone(),
                amount: asset.amount,
            }],
        },
    };

    Ok(CosmosMsg::Wasm(wasm_msg))
}

fn get_swap_amount(
    amount_a: U256,
    amount_b: U256,
    pool_a: U256,
    pool_b: U256,
) -> Uint128 {
    let pool_ax = amount_a + pool_a;
    let pool_bx = amount_b + pool_b;
    let area_ax = pool_ax * pool_b;
    let area_bx = pool_bx * pool_a;

    let a = U256::from(9) * area_ax + U256::from(3988000) * area_bx;
    let b = U256::from(3) * area_ax + area_ax.integer_sqrt() * a.integer_sqrt();
    let result = b / U256::from(2000) / pool_bx - pool_a;

    result.as_u128().into()
}

#[allow(clippy::too_many_arguments)]
fn optimal_swap(
    querier: &QuerierWrapper<TerraQuery>,
    provide_a_amount: Uint128,
    provide_b_amount: Uint128,
    asset_info_a: AssetInfo,
    asset_info_b: AssetInfo,
    pair_contract: Addr,
    messages: &mut Vec<CosmosMsg>,
) -> StdResult<(Uint128, Uint128, Uint128, Uint128)> {
    let (pool_a_amount, pool_b_amount, _) =
        query_pool(pair_contract.to_string(), querier, &asset_info_a, &asset_info_b)?;
    let provide_a_amount = U256::from(provide_a_amount.u128());
    let provide_b_amount = U256::from(provide_b_amount.u128());
    let pool_a_amount = U256::from(pool_a_amount.u128());
    let pool_b_amount = U256::from(pool_b_amount.u128());
    let provide_a_area = provide_a_amount * pool_b_amount;
    let provide_b_area = provide_b_amount * pool_a_amount;
    let mut swap_amount_a = Uint128::zero();
    let mut swap_amount_b = Uint128::zero();
    let mut return_amount_a = Uint128::zero();
    let mut return_amount_b = Uint128::zero();

    #[allow(clippy::comparison_chain)]
    if provide_a_area > provide_b_area {
        let swap_amount = get_swap_amount(provide_a_amount, provide_b_amount, pool_a_amount, pool_b_amount);
        if !swap_amount.is_zero() {
            let swap_asset = Asset {
                info: asset_info_a,
                amount: swap_amount
            };
            return_amount_b =
                simulate(querier, pair_contract.clone(), &swap_asset)
                .map_or(Uint128::zero(), |it| it.return_amount);
            if !return_amount_b.is_zero() {
                swap_amount_a = swap_amount;
                messages.push(swap_msg(
                    pair_contract.to_string(),
                    &swap_asset,
                    None,
                    Some(Decimal::percent(50)),
                    None,
                )?);
            }
        }
    } else if provide_a_area < provide_b_area {
        let swap_amount = get_swap_amount(provide_b_amount, provide_a_amount, pool_b_amount, pool_a_amount);
        if !swap_amount.is_zero() {
            let swap_asset = Asset {
                info: asset_info_b,
                amount: swap_amount
            };
            // in case of uluna, tax was deducted before calling this fn
            return_amount_a =
                simulate(querier, pair_contract.clone(),&swap_asset)
                .map_or(Uint128::zero(), |it| it.return_amount);
            if !return_amount_a.is_zero() {
                swap_amount_b = swap_amount;
                messages.push(swap_msg(
                    pair_contract.to_string(),
                    &swap_asset,
                    None,
                    Some(Decimal::percent(50)),
                    None,
                )?);
            }
        }
    };

    Ok((swap_amount_a, swap_amount_b, return_amount_a, return_amount_b))
}
