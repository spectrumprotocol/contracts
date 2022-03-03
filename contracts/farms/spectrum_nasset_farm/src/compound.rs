use cosmwasm_std::{attr, to_binary, Attribute, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store},
};

use crate::querier::{query_claimable_reward};

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state};

use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg};
use terraswap::querier::{query_token_balance, simulate};
use spectrum_protocol::{
    farm_helper::deduct_tax,
    gov_proxy::{
        Cw20HookMsg as GovProxyCw20HookMsg
    },
};
use moneymarket::market::{ExecuteMsg as MoneyMarketExecuteMsg};
use pylon_gateway::pool_msg::{
    ExecuteMsg as PylonGatewayExecuteMsg
};
use spectrum_protocol::nasset_farm::ExecuteMsg;

pub fn compound(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let pair_contract = deps.api.addr_humanize(&config.pair_contract)?;
    let reward_token = deps.api.addr_humanize(&config.reward_token)?;
    let gateway_pool = deps.api.addr_humanize(&config.nasset_rewards)?;
    let dp_token = deps.api.addr_humanize(&config.nasset_token)?;

    let gov_proxy = if let Some(gov_proxy) = &config.gov_proxy {
        Some(deps.api.addr_humanize(gov_proxy)?)
    } else {
        None
    };

    let reward_info = query_claimable_reward(
        deps.as_ref(),
        &config.nasset_rewards,
        &env.contract.address,
        Some(env.block.time.seconds()),
    )?;
    let dp_token_balance = query_token_balance(&deps.querier, dp_token, env.contract.address.clone())?;

    let mut total_reward_token_stake_amount = Uint128::zero();
    let mut total_reward_token_commission = Uint128::zero();
    let mut compound_amount = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    // calculate auto-compound, auto-stake, and commission in reward_token
    let mut pool_info = pool_info_read(deps.storage).load(config.nasset_token.as_slice())?;
    let reward = reward_info.amount;
    if !reward.is_zero() && !reward_info.amount.is_zero() {
        let commission = reward * total_fee;
        let reward_token_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_reward_token_commission += commission;

        let auto_bond_amount = dp_token_balance
            .checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount =
            reward_token_amount.multiply_ratio(auto_bond_amount, dp_token_balance);
        let stake_amount = reward_token_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_reward_token_stake_amount += stake_amount;
    }
    let mut state = read_state(deps.storage)?;
    deposit_farm_share(
        deps.as_ref(),
        &env,
        &mut state,
        &mut pool_info,
        &config,
        total_reward_token_stake_amount,
    )?;
    state_store(deps.storage).save(&state)?;
    pool_info_store(deps.storage).save(config.nasset_token.as_slice(), &pool_info)?;

    let total_reward_token_swap_amount = compound_amount;

    // find reward_token swap rate
    let reward_token_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: reward_token.to_string(),
        },
        amount: total_reward_token_swap_amount,
    };
    let reward_token_swap_rate_to_dp_token = simulate(
        &deps.querier,
        pair_contract.clone(),
        &reward_token_asset,
    )?;

    let earned_dp_token = reward_token_swap_rate_to_dp_token.return_amount;

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_reward_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: gateway_pool.to_string(),
        funds: vec![],
        msg: to_binary(&PylonGatewayExecuteMsg::Claim { target: None })?,
    });
    messages.push(withdraw_all_reward_token);

    if !total_reward_token_swap_amount.is_zero() {
        let swap_reward_token_to_dp_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract.to_string(),
                amount: total_reward_token_swap_amount,
                msg: to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_reward_token_to_dp_token);
    }

    if !total_reward_token_commission.is_zero() {

        // find SPEC swap rate
        let net_commission = Asset {
            info: AssetInfo::Token {
                contract_addr: reward_token.to_string(),
            },
            amount: total_reward_token_commission,
        };

        let reward_token_swap_rate_to_uusd = simulate(
            &deps.querier,
            deps.api.addr_humanize(&config.ust_pair_contract)?,
            &net_commission)?;

        let net_commission_amount =
            deduct_tax(&deps.querier,
                deduct_tax(&deps.querier, reward_token_swap_rate_to_uusd.return_amount, "uusd".to_string())?,
                "uusd".to_string())?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission_amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", net_commission_amount));

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: deps.api.addr_humanize(&config.ust_pair_contract)?.to_string(),
                amount: total_reward_token_commission,
                msg: to_binary(&TerraswapCw20HookMsg::Swap {
                    to: None,
                    max_spread: None,
                    belief_price: None,
                })?,
            })?,
            funds: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
            msg: to_binary(&MoneyMarketExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: "uusd".to_string(),
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

    if let Some(gov_proxy) = gov_proxy {
        if !total_reward_token_stake_amount.is_zero() {
            let stake_farm_token = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_token.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: gov_proxy.to_string(),
                    amount: total_reward_token_stake_amount,
                    msg: to_binary(&GovProxyCw20HookMsg::Stake {})?,
                })?,
            });
            messages.push(stake_farm_token);
        }
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("reward_token", reward_token));
    attributes.push(attr("earned_dp_token", earned_dp_token));

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
