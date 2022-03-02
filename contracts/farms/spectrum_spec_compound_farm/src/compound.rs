use cosmwasm_std::{attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, QueryRequest, WasmQuery};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store},
};

use crate::querier::query_spec_reward_info;

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use spectrum_protocol::spec_farm::{
    Cw20HookMsg as SpecStakingCw20HookMsg, ExecuteMsg as SpecStakingExecuteMsg,
};
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg, Cw20HookMsg as GovCw20HookMsg};
use spectrum_protocol::orion_farm::ExecuteMsg;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg, QueryMsg as TerraswapQueryMsg, PoolResponse};
use terraswap::querier::{query_token_balance, simulate};
use spectrum_protocol::farm_helper::{compute_provide_after_swap, deduct_tax};
use moneymarket::market::{ExecuteMsg as MoneyMarketExecuteMsg};

pub fn compound(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let pair_contract = deps.api.addr_humanize(&config.pair_contract)?;
    let spectrum_staking = deps.api.addr_humanize(&config.spectrum_staking)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let spec_reward_info = query_spec_reward_info(
        deps.as_ref(),
        &config.spectrum_staking,
        &env.contract.address,
        &config.spectrum_token
    )?;

    let mut total_spec_swap_amount = Uint128::zero();
    let mut total_spec_stake_amount = Uint128::zero();
    let mut total_spec_commission = Uint128::zero();
    let mut compound_amount = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    // calculate auto-compound, auto-Stake, and commission in SPEC
    let mut pool_info = pool_info_read(deps.storage).load(config.spectrum_token.as_slice())?;
    let reward = spec_reward_info.reward_infos.iter().find(|it| it.asset_token == spectrum_token.to_string()).ok_or_else(|| StdError::not_found("pool"))?.pending_spec_reward;
    let spec_reward_info_bond_amount = spec_reward_info.reward_infos.iter().find(|it| it.asset_token == spectrum_token.to_string()).ok_or_else(|| StdError::not_found("pool"))?.bond_amount;
    if !reward.is_zero() && !spec_reward_info_bond_amount.is_zero() {
        let commission = reward * total_fee;
        let spec_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_spec_commission += commission;
        total_spec_swap_amount += commission;

        let auto_bond_amount = spec_reward_info_bond_amount
            .checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount =
            spec_amount.multiply_ratio(auto_bond_amount, spec_reward_info_bond_amount);
        let stake_amount = spec_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_spec_stake_amount += stake_amount;
    }
    let mut state = read_state(deps.storage)?;
    deposit_farm_share(
        deps.as_ref(),
        &env,
        &mut state,
        &mut pool_info,
        &config,
        total_spec_stake_amount,
    )?;
    state_store(deps.storage).save(&state)?;
    pool_info_store(deps.storage).save(config.spectrum_token.as_slice(), &pool_info)?;

    // get reinvest amount
    let reinvest_allowance = query_token_balance(&deps.querier, spectrum_token.clone(), env.contract.address.clone())?;
    let reinvest_amount = reinvest_allowance + compound_amount;
    // split reinvest amount
    let swap_amount = reinvest_amount.multiply_ratio(1u128, 2u128);
    // add commission to reinvest SPEC to total swap amount
    total_spec_swap_amount += swap_amount;

    // find SPEC swap rate
    let orion = Asset {
        info: AssetInfo::Token {
            contract_addr: spectrum_token.to_string(),
        },
        amount: total_spec_swap_amount,
    };
    let orion_swap_rate = simulate(
        &deps.querier,
        pair_contract.clone(),
        &orion,
    )?;

    let total_ust_return_amount = deduct_tax(&deps.querier, orion_swap_rate.return_amount, config.base_denom.clone())?;
    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let total_ust_commission_amount = if total_spec_swap_amount != Uint128::zero() {
        total_ust_return_amount.multiply_ratio(total_spec_commission, total_spec_swap_amount)
    } else {
        Uint128::zero()
    };
    let total_ust_reinvest_amount =
        total_ust_return_amount.checked_sub(total_ust_commission_amount)?;

    // deduct tax for provided UST
    let net_reinvest_ust = deduct_tax(
        &deps.querier,
        total_ust_reinvest_amount,
        config.base_denom.clone(),
    )?;
    let pool: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&TerraswapQueryMsg::Pool {})?,
    }))?;

    let provide_spec = compute_provide_after_swap(
        &pool,
        &orion,
        orion_swap_rate.return_amount,
        net_reinvest_ust
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_spec: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: spectrum_staking.to_string(),
        funds: vec![],
        msg: to_binary(&SpecStakingExecuteMsg::withdraw { asset_token: Some(spectrum_token.to_string()), spec_amount: None })?,
    });
    messages.push(withdraw_all_spec);

    if !total_spec_swap_amount.is_zero() {
        let swap_orion: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract.to_string(),
                amount: total_spec_swap_amount,
                msg: to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_orion);
    }

    if !total_ust_commission_amount.is_zero() {

        // find SPEC swap rate
        let net_commission_amount = deduct_tax(&deps.querier, total_ust_commission_amount, config.base_denom.clone())?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission_amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", net_commission_amount));

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
            msg: to_binary(&MoneyMarketExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: config.base_denom.clone(),
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

    if !total_spec_stake_amount.is_zero() {
        let stake_orion = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: spectrum_gov.to_string(),
                amount: total_spec_stake_amount,
                msg: to_binary(&GovCw20HookMsg::stake_tokens { staker_addr: None, days: Some(0) })?,
            })?,
        });
        messages.push(stake_orion);
    }

    if !provide_spec.is_zero() {
        let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_contract.to_string(),
                amount: provide_spec,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: spectrum_token.to_string(),
                        },
                        amount: provide_spec,
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: config.base_denom.clone(),
                        },
                        amount: net_reinvest_ust,
                    },
                ],
                slippage_tolerance: None,
                receiver: None,
            })?,
            funds: vec![Coin {
                denom: config.base_denom,
                amount: net_reinvest_ust,
            }],
        });
        messages.push(provide_liquidity);

        let stake = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::stake {
                asset_token: spectrum_token.to_string(),
            })?,
            funds: vec![],
        });
        messages.push(stake);
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", spectrum_token));
    attributes.push(attr("reinvest_amount", reinvest_amount));
    attributes.push(attr("provide_token_amount", provide_spec));
    attributes.push(attr("provide_ust_amount", net_reinvest_ust));

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
    // only orion farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let spectrum_staking = deps.api.addr_humanize(&config.spectrum_staking)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: spectrum_staking.to_string(),
                amount,
                msg: to_binary(&SpecStakingCw20HookMsg::bond { staker_addr: None, asset_token: asset_token.to_string() })?,
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
