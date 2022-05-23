use std::collections::HashMap;

use cosmwasm_std::{
    attr, to_binary, Attribute, Coin, CosmosMsg, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg, Decimal,
};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store},
};

use crate::querier::query_mirror_reward_info;

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state};
use mirror_protocol::gov::Cw20HookMsg as MirrorGovCw20HookMsg;
use mirror_protocol::staking::ExecuteMsg as MirrorExecuteMsg;
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    Cw20HookMsg as TerraswapCw20HookMsg,
};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};
use spectrum_protocol::farm_helper::deduct_tax;
use spectrum_protocol::mirror_farm::ExecuteMsg;
use moneymarket::market::{ExecuteMsg as MoneyMarketExecuteMsg};

// harvest all
pub fn harvest_all(mut deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let mirror_staking = deps.api.addr_humanize(&config.mirror_staking)?;
    let mirror_token = deps.api.addr_humanize(&config.mirror_token)?;
    let mirror_gov = deps.api.addr_humanize(&config.mirror_gov)?;

    let mirror_reward_infos = query_mirror_reward_info(
        deps.as_ref(),
        mirror_staking.to_string(),
        env.contract.address.to_string(),
    )?;

    let mut total_mir_swap_amount = Uint128::zero();
    let mut total_mir_stake_amount = Uint128::zero();
    let mut total_mir_commission = Uint128::zero();
    let mut swap_amount_map: HashMap<String, Uint128> = HashMap::new();
    let mut stake_amount_pairs: Vec<(String, Uint128)> = vec![];

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;
    for mirror_reward_info in mirror_reward_infos.reward_infos.iter() {
        let reward = mirror_reward_info.pending_reward;
        if reward.is_zero() || mirror_reward_info.bond_amount.is_zero() {
            continue;
        }

        let commission = reward * total_fee;
        let mirror_amount = reward.checked_sub(commission)?;
        total_mir_commission += commission;
        total_mir_swap_amount += commission;

        let asset_token_raw = deps
            .api
            .addr_canonicalize(&mirror_reward_info.asset_token)?;
        let pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
        let auto_bond_amount = mirror_reward_info
            .bond_amount
            .checked_sub(pool_info.total_stake_bond_amount)?;

        let swap_amount =
            mirror_amount.multiply_ratio(auto_bond_amount, mirror_reward_info.bond_amount);
        let stake_amount = mirror_amount.checked_sub(swap_amount)?;

        swap_amount_map.insert(mirror_reward_info.asset_token.clone(), swap_amount);
        if !stake_amount.is_zero() {
            stake_amount_pairs.push((mirror_reward_info.asset_token.clone(), stake_amount));
        }
        if mirror_reward_info.asset_token != mirror_token {
            total_mir_swap_amount += swap_amount;
        }
        total_mir_stake_amount += stake_amount
    }

    attributes.push(attr("total_mir_commission", total_mir_commission));
    attributes.push(attr("total_mir_swap_amount", total_mir_swap_amount));
    attributes.push(attr("total_mir_stake_amount", total_mir_stake_amount));

    deposit_farm_share(deps.branch(), &env, &config, stake_amount_pairs)?;

    let mir_pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory,
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: mirror_token.to_string(),
            },
        ],
    )?;
    //Find Swap Rate
    let mir = Asset {
        info: AssetInfo::Token {
            contract_addr: mirror_token.to_string(),
        },
        amount: total_mir_swap_amount,
    };
    let mir_swap_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&mir_pair_info.contract_addr)?,
        &mir,
    )?;

    let total_ust_return_amount = deduct_tax(&deps.querier, mir_swap_rate.return_amount, config.base_denom.clone())?;
    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let total_ust_commission_amount =
        total_ust_return_amount.multiply_ratio(total_mir_commission, total_mir_swap_amount);

    for mirror_reward_info in mirror_reward_infos.reward_infos.iter() {
        let asset_token_raw = deps
            .api
            .addr_canonicalize(&mirror_reward_info.asset_token)?;
        let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
        let swap_amount = swap_amount_map
            .remove(&mirror_reward_info.asset_token)
            .unwrap_or_else(Uint128::zero);

        if mirror_reward_info.asset_token == mirror_token {
            pool_info.reinvest_allowance += swap_amount;
        } else {
            let reinvest_allowance =
                total_ust_return_amount.multiply_ratio(swap_amount, total_mir_swap_amount);
            pool_info.reinvest_allowance += reinvest_allowance;
        }
        pool_info_store(deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    }

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_mir: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_staking.to_string(),
        funds: vec![],
        msg: to_binary(&MirrorExecuteMsg::Withdraw { asset_token: None })?,
    });
    messages.push(withdraw_all_mir);

    if !total_mir_swap_amount.is_zero() {
        let swap_mir: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: mir_pair_info.contract_addr,
                amount: total_mir_swap_amount,
                msg: to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: Some(Decimal::percent(50)),
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_mir);
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

    if !total_mir_stake_amount.is_zero() {
        let stake_mir = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: mirror_gov.to_string(),
                amount: total_mir_stake_amount,
                msg: to_binary(&MirrorGovCw20HookMsg::StakeVotingTokens {})?,
            })?,
        });
        messages.push(stake_mir);
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
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
