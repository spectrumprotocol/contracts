use cosmwasm_std::{attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, Decimal};

use crate::{
    bond::deposit_farm_share,
    querier::{query_astroport_pending_token, query_astroport_pool_balance, astroport_router_simulate_swap},
    state::{read_config, state_store}, model::ExecuteMsg,
};

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use astroport::asset::{Asset, AssetInfo};
use astroport::generator::{
    Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg
};
use astroport::pair::{Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg, PoolResponse};
use astroport::router::{SwapOperation, ExecuteMsg as AstroportRouterExecuteMsg};
use astroport::querier::{query_token_balance, simulate};
use moneymarket::market::ExecuteMsg as MoneyMarketExecuteMsg;
use spectrum_protocol::farm_helper::{deduct_tax};
use spectrum_protocol::gov_proxy::Cw20HookMsg as GovProxyCw20HookMsg;
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use crate::bond::deposit_farm2_share;

pub fn compound(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    threshold_compound_astro: Uint128,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let stluna_token = deps.api.addr_humanize(&config.stluna_token)?;
    let weldo_token =  deps.api.addr_humanize(&config.weldo_token)?;
    let astro_token = deps.api.addr_humanize(&config.astro_token)?;
    let xastro_proxy = deps.api.addr_humanize(&config.xastro_proxy)?;
    let astro_ust_pair_contract = deps.api.addr_humanize(&config.astro_ust_pair_contract)?;
    let pair_contract = deps.api.addr_humanize(&config.pair_contract)?;
    let astroport_router = deps.api.addr_humanize(&config.astroport_router)?;
    let stluna_weldo_pair_contract = deps.api.addr_humanize(&config.stluna_weldo_pair_contract)?;
    let uluna_uusd_pair_contract = deps.api.addr_humanize(&config.uluna_uusd_pair_contract)?;

    let uluna = "uluna".to_string();
    let uusd = "uusd".to_string();

    let mut pool_info = pool_info_read(deps.storage).load(config.stluna_token.as_slice())?;

    // This get pending (ASTRO), and pending proxy rewards
    let pending_token_response = query_astroport_pending_token(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let mut total_weldo_token_swap_amount = Uint128::zero();
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
        total_weldo_token_swap_amount += commission + compound_amount;

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
    let (total_ust_reinvest_amount_astro, total_ust_commission_amount_astro) =
        if !total_astro_token_swap_amount.is_zero() {
        // find ASTRO swap rate
        let astro_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: astro_token.clone(),
            },
            amount: total_astro_token_swap_amount,
        };
        let astro_swap_rate =
            simulate(&deps.querier, astro_ust_pair_contract.clone(), &astro_asset)?;
        let total_ust_return_amount_astro =
            deduct_tax(&deps.querier, astro_swap_rate.return_amount, uusd.clone())?;
        attributes.push(attr("total_ust_return_amount_astro", total_ust_return_amount_astro));

        let total_ust_commission_amount_astro = total_ust_return_amount_astro
            .multiply_ratio(total_astro_token_commission, total_astro_token_swap_amount);

        let total_ust_reinvest_amount_astro =
            total_ust_return_amount_astro.checked_sub(total_ust_commission_amount_astro)?;

        (total_ust_reinvest_amount_astro, total_ust_commission_amount_astro)
    } else {
        (Uint128::zero(), Uint128::zero())
    };

    // find weLDO to stLuna swap rate
    let weldo_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: weldo_token.clone(),
        },
        amount: total_weldo_token_swap_amount,
    };
    let weldo_token_swap_rate = simulate(&deps.querier, stluna_weldo_pair_contract.clone(), &weldo_asset)?;

    let total_stluna_return_amount = weldo_token_swap_rate.return_amount;
    attributes.push(attr("total_stluna_return_amount", total_stluna_return_amount));

    let total_stluna_commission_amount = if total_weldo_token_swap_amount != Uint128::zero() {
        total_stluna_return_amount.multiply_ratio(total_weldo_token_commission, total_weldo_token_swap_amount)
    } else {
        Uint128::zero()
    };
    attributes.push(attr("total_stluna_commission_amount", total_stluna_commission_amount));

    let total_ust_commission_amount = if !total_stluna_commission_amount.is_zero() {
        let stluna_ust_swap_rate = astroport_router_simulate_swap(
            deps.as_ref(),
            total_stluna_commission_amount,
            vec![
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
                    ask_asset_info: AssetInfo::NativeToken { denom: uluna.clone() }
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
                    ask_asset_info: AssetInfo::NativeToken { denom: uusd.clone() },
                }
            ],
            &config.astroport_router
        )?;

        deduct_tax(
            &deps.querier,
            stluna_ust_swap_rate.amount,
            uusd.clone(),
        )?
    } else {
        Uint128::zero()
    };
    attributes.push(attr("total_ust_commission_amount", total_ust_commission_amount));

    let total_stluna_reinvest_amount = total_stluna_return_amount.checked_sub(total_stluna_commission_amount)?;
    attributes.push(attr("total_stluna_reinvest_amount", total_stluna_reinvest_amount));

    let ust_reinvest_asset_astro = Asset {
        info: AssetInfo::NativeToken {
            denom: uusd.clone(),
        },
        amount: deduct_tax(&deps.querier, total_ust_reinvest_amount_astro, uusd.clone())?,
    };

    let ust_to_luna_swap_rate = simulate(&deps.querier, uluna_uusd_pair_contract.clone(), &ust_reinvest_asset_astro)?;
    let total_luna_reinvest_amount = ust_to_luna_swap_rate.return_amount;
    attributes.push(attr("total_luna_reinvest_amount", total_luna_reinvest_amount));

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

    if !total_weldo_token_swap_amount.is_zero() {
        let swap_weldo_token_to_stluna: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: weldo_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: stluna_weldo_pair_contract.to_string(),
                amount: total_weldo_token_swap_amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    max_spread: Some(Decimal::percent(50)),
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_weldo_token_to_stluna);

        if !total_stluna_commission_amount.is_zero() {
            let swap_stluna_to_ust: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: stluna_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: astroport_router.to_string(),
                    amount: total_stluna_commission_amount,
                    msg: to_binary(&AstroportRouterExecuteMsg::ExecuteSwapOperations {
                        operations: vec![
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::Token { contract_addr: stluna_token.clone() },
                                ask_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
                            },
                            SwapOperation::AstroSwap {
                                offer_asset_info: AssetInfo::NativeToken { denom: uluna.clone() },
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
            messages.push(swap_stluna_to_ust);
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

        //swap uusd exclude commission to uluna
        let uusd_from_astro_exclude_commission = Asset {
            info: AssetInfo::NativeToken {
                denom: uusd.clone(),
            },
            amount: total_ust_reinvest_amount_astro,
        };
        let swap_uusd_to_uluna: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: uluna_uusd_pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::Swap {
                to: None,
                max_spread: Some(Decimal::percent(50)),
                belief_price: None,
                offer_asset: uusd_from_astro_exclude_commission
            })?,
            funds: vec![Coin { denom: uusd.clone(), amount: total_ust_reinvest_amount_astro }]
        });
        messages.push(swap_uusd_to_uluna);
    }

    // let sttoken_asset_info = AssetInfo::Token {
    //     contract_addr: stluna_token.clone(),
    // };
    // let uluna_asset_info = AssetInfo::NativeToken {
    //     denom: uluna.clone(),
    // };
    // let (stluna_amount_to_be_swapped, uluna_amount_to_be_swapped, stluna_return_from_optimal_swap, uluna_return_from_optimal_swap) = optimal_swap(
    //     &deps.querier,
    //     total_stluna_reinvest_amount,
    //     total_luna_reinvest_amount,
    //     sttoken_asset_info,
    //     uluna_asset_info,
    //     pair_contract.clone(),
    //     &mut messages
    // )?;
    //
    // let provide_stluna = total_stluna_reinvest_amount.checked_sub(stluna_amount_to_be_swapped)? + stluna_return_from_optimal_swap;
    // let provide_uluna = total_luna_reinvest_amount.checked_sub(uluna_amount_to_be_swapped)? + uluna_return_from_optimal_swap;

    let provide_stluna = total_stluna_reinvest_amount;
    let provide_uluna = total_luna_reinvest_amount;

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

    if !provide_stluna.is_zero() || !provide_uluna.is_zero() {
        if !provide_stluna.is_zero() {
            let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: stluna_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: pair_contract.to_string(),
                    amount: provide_stluna,
                    expires: None,
                })?,
                funds: vec![],
            });
            messages.push(increase_allowance);
        }

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: stluna_token.clone(),
                        },
                        amount: provide_stluna,
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: uluna.clone(),
                        },
                        amount: provide_uluna,
                    },
                ],
                slippage_tolerance: None,
                receiver: None,
                auto_stake: Some(true),
            })?,
            funds: if provide_uluna.is_zero() {
                vec![]
            } else {
                vec![Coin {
                    denom: uluna,
                    amount: provide_uluna,
                }]
            },
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

    let total_ust_commission_amount = total_ust_commission_amount + total_ust_commission_amount_astro;
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
    deps: DepsMut,
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
    deps: DepsMut,
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

    let aust_balance = query_token_balance(&deps.querier, aust_token.clone(), env.contract.address)?;

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
    Ok(Response::new()
        .add_messages(messages))
}

// /// @notice Generate msg for swapping specified asset
// fn swap_msg(pair_contract: String, asset: &Asset, belief_price: Option<Decimal>, max_spread: Option<Decimal>, to: Option<String>) -> StdResult<CosmosMsg> {
//     let wasm_msg = match &asset.info {
//         AssetInfo::Token {
//             contract_addr,
//         } => WasmMsg::Execute {
//             contract_addr: contract_addr.to_string(),
//             msg: to_binary(&Cw20ExecuteMsg::Send {
//                 contract: pair_contract,
//                 amount: asset.amount,
//                 msg: to_binary(&AstroportPairCw20HookMsg::Swap {
//                     belief_price,
//                     max_spread,
//                     to,
//                 })?,
//             })?,
//             funds: vec![],
//         },
//
//         AssetInfo::NativeToken {
//             denom,
//         } => WasmMsg::Execute {
//             contract_addr: pair_contract,
//             msg: to_binary(&AstroportPairExecuteMsg::Swap {
//                 offer_asset: asset.clone(),
//                 belief_price,
//                 max_spread,
//                 to: None,
//             })?,
//             funds: vec![Coin {
//                 denom: denom.clone(),
//                 amount: asset.amount,
//             }],
//         },
//     };
//
//     Ok(CosmosMsg::Wasm(wasm_msg))
// }

// fn optimal_swap(
//     querier: &QuerierWrapper,
//     provide_a_amount: Uint128,
//     provide_b_amount: Uint128,
//     asset_info_a: AssetInfo,
//     asset_info_b: AssetInfo,
//     pair_contract: Addr,
//     messages: &mut Vec<CosmosMsg>,
// ) -> StdResult<(Uint128, Uint128, Uint128, Uint128)> {
//     if !provide_a_amount.is_zero() && !provide_b_amount.is_zero() {
//         return Ok((Uint128::zero(), Uint128::zero(), Uint128::zero(), Uint128::zero()));
//     }
//
//     if !provide_a_amount.is_zero() {
//         let swap_amount = provide_a_amount.multiply_ratio(10000u128, 19995u128);
//         let offer_asset = Asset {
//             info: asset_info_a,
//             amount: swap_amount
//         };
//         messages.push(swap_msg(
//             pair_contract.to_string(),
//             &offer_asset,
//             None,
//             Some(Decimal::percent(50)),
//             None,
//         )?);
//         let simulate = simulate(
//             querier,
//             pair_contract,
//             &offer_asset,
//         )?;
//         Ok((swap_amount, Uint128::zero(), Uint128::zero(), simulate.return_amount))
//     } else {
//         let swap_amount = provide_b_amount.multiply_ratio(10000u128, 19995u128);
//         let offer_asset = Asset {
//             info: asset_info_b,
//             amount: swap_amount
//         };
//         messages.push(swap_msg(
//             pair_contract.to_string(),
//             &offer_asset,
//             None,
//             Some(Decimal::percent(50)),
//             None,
//         )?);
//         let simulate = simulate(
//             querier,
//             pair_contract,
//             &offer_asset,
//         )?;
//         Ok((Uint128::zero(), swap_amount, simulate.return_amount, Uint128::zero()))
//     }
// }
