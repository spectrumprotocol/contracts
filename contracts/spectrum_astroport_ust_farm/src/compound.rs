use cosmwasm_std::{
    attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::{
    bond::deposit_farm_share,
    querier::{query_astroport_pending_token, query_astroport_pool_balance},
    state::{read_config, state_store},
};

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use astroport::asset::{Asset, AssetInfo};
use astroport::generator::{
    Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg, PendingTokenResponse,
};
use astroport::pair::{
    Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg, PoolResponse,
    QueryMsg as AstroportPairQueryMsg,
};
use astroport::querier::{query_token_balance, simulate};
use moneymarket::market::ExecuteMsg as MoneyMarketExecuteMsg;
use spectrum_protocol::anchor_farm::ExecuteMsg;
use spectrum_protocol::farm_helper::{compute_provide_after_swap, deduct_tax};
use spectrum_protocol::{
    farm_helper::compute_provide_after_swap_astroport,
    gov_proxy::{
        Cw20HookMsg as GovProxyCw20HookMsg, ExecuteMsg as GovProxyExecuteMsg, StakerInfoGovResponse,
    },
};
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

    let pair_contract = deps.api.addr_humanize(&config.pair_contract)?;
    let astro_ust_pair_contract = deps.api.addr_humanize(&config.astro_ust_pair_contract)?;
    let astroport_generator = deps.api.addr_humanize(&config.astroport_generator)?;
    let farm_token = deps.api.addr_humanize(&config.farm_token)?;
    let astro_token = deps.api.addr_humanize(&config.astro_token)?;

    let gov_proxy = if let Some(gov_proxy) = config.gov_proxy {
        Some(deps.api.addr_humanize(&gov_proxy)?)
    } else {
        None
    };

    let mut pool_info = pool_info_read(deps.storage).load(config.farm_token.as_slice())?;

    // This get pending (ASTRO), and pending proxy rewards
    let pending_token = query_astroport_pending_token(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator
    )?;
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut attributes: Vec<Attribute> = vec![];

    let manual_claim_pending_token = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.addr_humanize(&config.astroport_generator)?.to_string(),
        funds: vec![],
        msg: to_binary(&AstroportExecuteMsg::Withdraw {
            lp_token: deps.api.addr_humanize(&pool_info.staking_token)?,
            amount: Uint128::zero(),
        })?,
    });
    messages.push(manual_claim_pending_token);
    attributes.push(attr("action", "manual_claim_pending_token"));
    attributes.push(attr("pending_token_response.pending_on_proxy", pending_token_response
        .pending_on_proxy
        .unwrap_or_else(Uint128::zero)));
    attributes.push(attr("pending_token_response.pending", pending_token_response.pending));

    // TODO query env.contract farm token and ASTRO DONE
    // TODO set reinvest allowance for both farm token and ASTRO (need new reinvest_allowance_astro?) DONE
    // TODO query_astroport_pending_token and do manual claim DONE
    // TODO logic to compound and stake ASTRO

    let reward = query_token_balance(&deps.querier, farm_token.clone(), env.contract.address.clone())? + pending_token.pending_on_proxy?;
    let reward_astro = query_token_balance(&deps.querier, xastro_token.clone(), env.contract.address.clone())? + pending_token.pending;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let mut total_farm_token_swap_amount = Uint128::zero();
    let mut total_farm_token_stake_amount = Uint128::zero();
    let mut total_farm_token_commission = Uint128::zero();
    let mut total_astro_token_swap_amount = Uint128::zero();
    let mut total_astro_token_stake_amount = Uint128::zero();
    let mut total_astro_token_commission = Uint128::zero();
    let mut compound_amount = Uint128::zero();
    let mut compound_amount_astro = Uint128::zero();

    let mut state = read_state(deps.storage)?;

    // calculate auto-compound, auto-stake, and commission in astro token
    if !reward_astro.is_zero() && !lp_balance.is_zero() && threshold_compound_astro > reward_astro{
        let total_fee_astro = config.community_fee + config.platform_fee + config.controller_fee;
        let commission_astro = reward_astro * total_fee_astro;
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
    }
    deposit_farm_share(
        deps.as_ref(),
        &env,
        &mut state,
        &mut pool_info,
        &config,
        total_astro_token_stake_amount,
    )?;

    // calculate auto-compound, auto-stake, and commission in farm token
    if !reward.is_zero() && !lp_balance.is_zero() {
        let total_fee = config.community_fee + config.platform_fee + config.controller_fee;
        let commission = reward * total_fee;
        let farm_token_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_farm_token_commission += commission;
        total_farm_token_swap_amount += commission;

        let auto_bond_amount = lp_balance.checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount = farm_token_amount.multiply_ratio(auto_bond_amount, lp_balance);
        let stake_amount = farm_token_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_farm_token_stake_amount += stake_amount;
    }

    deposit_farm2_share(
        deps.as_ref(),
        &env,
        &mut state,
        &mut pool_info,
        &config,
        total_farm_token_stake_amount,
    )?;
    state_store(deps.storage).save(&state)?;
    pool_info_store(deps.storage).save(config.farm_token.as_slice(), &pool_info)?;

    // get reinvest amount
    //TODO apply ASTRO
    let reinvest_allowance_astro = reward_astro;
    let reinvest_allowance = reward;
    let reinvest_amount_astro = reinvest_allowance_astro + compound_amount_astro;
    let reinvest_amount = reinvest_allowance + compound_amount;

    // split reinvest amount
    // TODO rethink if this is correct
    let swap_amount_astro = reinvest_amount_astro; //Does not support ASTRO-UST
    // let swap_amount_astro = if farm_token == deps.api.addr_humanize(&config.astro_token)? {
    //     reinvest_amount_astro.multiply_ratio(1u128, 2u128);
    // } else {
    //     reinvest_amount_astro
    // };

    let swap_amount = reinvest_amount.multiply_ratio(1u128, 2u128);
    // add commission to reinvest farm token to total swap amount
    total_farm_token_swap_amount += swap_amount;

    // find ASTRO swap rate
    let astro_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: astro_token,
        },
        amount: total_astro_token_swap_amount,
    };
    let astro_token_swap_rate = simulate(&deps.querier, pair_contract.clone(), &astro_token_asset)?;
    let total_ust_return_amount_astro = deduct_tax(
        &deps.querier,
        astro_token_swap_rate.return_amount,
        config.base_denom.clone(),
    )?;
    attributes.push(attr("total_ust_return_amount_astro", total_ust_return_amount_astro));
    // find farm token swap rate
    let farm_token_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: farm_token,
        },
        amount: total_farm_token_swap_amount,
    };
    let farm_token_swap_rate = simulate(&deps.querier, pair_contract.clone(), &farm_token_asset)?;
    let total_ust_return_amount = deduct_tax(
        &deps.querier,
        farm_token_swap_rate.return_amount,
        config.base_denom.clone(),
    )?;
    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let total_ust_commission_amount = if total_farm_token_swap_amount != Uint128::zero() {
        total_ust_return_amount
            .multiply_ratio(total_farm_token_commission, total_farm_token_swap_amount)
    } else {
        Uint128::zero()
    };
    let total_ust_reinvest_amount =
        total_ust_return_amount.checked_sub(total_ust_commission_amount)?;

    let total_ust_commission_amount_astro = if total_farm_token_swap_amount_astro != Uint128::zero() {
        total_ust_return_amount_astro
            .multiply_ratio(total_astro_token_commission, total_astro_token_swap_amount)
    } else {
        Uint128::zero()
    };
    let total_ust_reinvest_amount =
        total_ust_return_amount.checked_sub(total_ust_commission_amount)?;

    // deduct tax for provided UST
    let net_reinvest_ust_astro = deduct_tax(
        &deps.querier,
        total_ust_reinvest_amount_astro,
        config.base_denom.clone(),
    )?;
    let net_reinvest_ust = deduct_tax(
        &deps.querier,
        total_ust_reinvest_amount,
        config.base_denom.clone(),
    )?;

    // let pool_astro: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
    //     contract_addr: astro_ust_pair_contract.to_string(),
    //     msg: to_binary(&AstroportPairQueryMsg::Pool {})?,
    // }))?;
    let pool: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&AstroportPairQueryMsg::Pool {})?,
    }))?;

    let provide_farm_token = compute_provide_after_swap_astroport(
        &pool,
        &farm_token_asset,
        farm_token_swap_rate.return_amount + astro_token_swap_rate.return_amount,
        net_reinvest_ust + net_reinvest_ust_astro,
    )?;

    if !total_farm_token_swap_amount.is_zero() {
        let swap_farm_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: farm_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract.to_string(),
                amount: total_farm_token_swap_amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_farm_token);
    }

    if !total_astro_token_swap_amount.is_zero() {
        let swap_astro_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
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
        messages.push(swap_astro_token);
    }

    let mut net_commission_astro_and_farm = Uint128::zero();
    if !total_ust_commission_amount.is_zero() {
        // find SPEC swap rate
        let net_commission_amount = deduct_tax(
            &deps.querier,
            total_ust_commission_amount,
            config.base_denom.clone(),
        )?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission_amount;
        net_commission_astro_and_farm = net_commission_astro_and_farm + net_commission_amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", net_commission_amount));
    }

    if !total_ust_commission_amount_astro.is_zero() {
        // find SPEC swap rate
        let net_commission_amount_astro = deduct_tax(
            &deps.querier,
            total_ust_commission_amount_astro,
            config.base_denom.clone(),
        )?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission_amount_astro;
        net_commission_astro_and_farm = net_commission_astro_and_farm + net_commission_amount_astro;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission_astro", net_commission_amount_astro));
    }

    if !net_commission_astro_and_farm.is_zero(){
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&MoneyMarketExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: config.base_denom.clone(),
                amount: net_commission_astro_and_farm,
            }],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::send_fee {})?,
            funds: vec![],
        }));
    }

    if let Some(gov_proxy) = gov_proxy {
        if !total_farm_token_stake_amount.is_zero() {
            let stake_farm_token = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: farm_token.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: gov_proxy.to_string(),
                    amount: total_farm_token_stake_amount,
                    msg: to_binary(&GovProxyCw20HookMsg::Stake {})?,
                })?,
            });
            messages.push(stake_farm_token);
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

    if !provide_farm_token.is_zero() {
        let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: farm_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_contract.to_string(),
                amount: provide_farm_token,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: &farm_token,
                        },
                        amount: provide_farm_token,
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: config.base_denom.clone(),
                        },
                        amount: net_reinvest_ust + net_reinvest_ust_astro,
                    },
                ],
                slippage_tolerance: None,
                receiver: None,
                auto_stake: Some(false),
            })?,
            funds: vec![Coin {
                denom: config.base_denom,
                amount: net_reinvest_ust + net_reinvest_ust_astro,
            }],
        });
        messages.push(provide_liquidity);

        let stake = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::stake {
                asset_token: farm_token.to_string(),
            })?,
            funds: vec![],
        });
        messages.push(stake);
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", &farm_token));
    attributes.push(attr("reinvest_amount", reinvest_amount));
    attributes.push(attr("reinvest_amount_astro", reinvest_amount_astro));
    attributes.push(attr("provide_farm_token", provide_farm_token));
    attributes.push(attr("provide_ust_amount", net_reinvest_ust));
    attributes.push(attr("provide_ust_amount_from_astro", net_reinvest_ust_astro));
    
    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
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

pub fn send_fee(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    // only farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let aust_token = deps.api.addr_humanize(&config.aust_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let aust_balance =
        query_token_balance(&deps.querier, aust_token.clone(), env.contract.address)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let thousand = Uint128::from(1000u64);
    let total_fee = config.community_fee + config.controller_fee + config.platform_fee;
    let community_amount =
        aust_balance.multiply_ratio(thousand * config.community_fee, thousand * total_fee);
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

    let platform_amount =
        aust_balance.multiply_ratio(thousand * config.platform_fee, thousand * total_fee);
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
    Ok(Response::new().add_messages(messages))
}
