use std::collections::{HashMap, HashSet};
use cosmwasm_std::{Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Order, QuerierWrapper, Response, StdError, StdResult, to_binary, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::querier::{query_token_balance, simulate};
use terraswap::pair::{ExecuteMsg as PairExecuteMsg, Cw20HookMsg as PairHookMsg};
use spectrum_protocol::farm_helper::deduct_tax;
use moneymarket::market::{ExecuteMsg as MoneyMarketExecuteMsg};
use spectrum_protocol::gov::{ExecuteMsg as GovExecuteMsg};

use crate::bond::update_claimable;
use crate::hub::{HubCw20HookMsg, HubExecuteMsg, query_hub_claimable, query_hub_current_batch, query_hub_histories, query_hub_parameters, query_hub_state};
use crate::model::{ExecuteMsg, SimulateCollectResponse, SwapOperation};
use crate::stader::{query_stader_batch, query_stader_claimable, query_stader_config, query_stader_state, StaderCw20HookMsg, StaderExecuteMsg};
use crate::state::{Burn, burn_store, burns_read, hub_read, HubType, read_config, read_state, state_store};

pub fn burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    swap_operations: Vec<SwapOperation>,
    min_profit: Option<Decimal>,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut state = read_state(deps.storage)?;
    let balance = deps.querier.query_balance(env.contract.address, "uluna")?;
    if amount > state.get_burnable_amount(balance.amount) {
        return Err(StdError::generic_err("cannot burn more than available minus claimable amount"));
    }

    // swap
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut asset = Asset {
        amount,
        info: AssetInfo::NativeToken { denom: "uluna".to_string() },
    };
    for swap_operation in swap_operations.iter() {
        if asset.is_native_token() {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: swap_operation.pair_address.clone(),
                msg: to_binary(&PairExecuteMsg::Swap {
                    offer_asset: asset.clone(),
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
                funds: vec![
                    Coin { denom: asset.to_string(), amount: asset.amount },
                ],
            }));
        } else {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: asset.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: swap_operation.pair_address.clone(),
                    amount: asset.amount,
                    msg: to_binary(&PairHookMsg::Swap {
                        max_spread: None,
                        belief_price: None,
                        to: None,
                    })?,
                })?,
                funds: vec![],
            }))
        }

        let simulate_result = simulate(
            &deps.querier,
            deps.api.addr_validate(&swap_operation.pair_address)?,
            &asset)?;
        asset = Asset {
            amount: simulate_result.return_amount,
            info: swap_operation.to_asset_info.clone(),
        };
    }

    // validate
    let last_swap = swap_operations.last()
        .ok_or_else(|| StdError::generic_err("require swap"))?;
    let token = last_swap.to_asset_info.to_string();
    let token_raw = deps.api.addr_canonicalize(&token)?;
    let hub = hub_read(deps.storage, token_raw.as_slice())?
        .ok_or_else(|| StdError::generic_err("batch not found"))?;
    let batch_id = if hub.hub_type == HubType::lunax {
        let stader_config = query_stader_config(
            &deps.querier,
            hub.hub_address.to_string())?;
        let stader_state = query_stader_state(
            &deps.querier,
            hub.hub_address.to_string())?;
        let target_luna = asset.amount * stader_state.state.exchange_rate * (Decimal::one() - stader_config.config.protocol_withdraw_fee);
        let expected_profit = amount * min_profit.unwrap_or_default();
        if target_luna.checked_sub(amount).unwrap_or_default() < expected_profit {
            return Err(StdError::generic_err("target luna is less than expected"));
        }

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hub.token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: hub.hub_address.to_string(),
                msg: to_binary(&StaderCw20HookMsg::QueueUndelegate {})?,
                amount,
            })?,
            funds: vec![],
        }));

        stader_state.state.current_undelegation_batch_id
    } else {
        let hub_state = query_hub_state(
            &deps.querier,
            hub.hub_address.to_string())?;
        let exchange_rate = match hub.hub_type {
            HubType::bluna => hub_state.bluna_exchange_rate,
            HubType::cluna => hub_state.exchange_rate,
            HubType::stluna => hub_state.stluna_exchange_rate,
            _ => {
                return Err(StdError::generic_err("unexpected"));
            }
        };
        let parameters = query_hub_parameters(
            &deps.querier,
            hub.hub_address.to_string(),
        )?;
        let target_luna = if exchange_rate < parameters.er_threshold {
            asset.amount * exchange_rate * (Decimal::one() - parameters.peg_recovery_fee)
        } else {
            asset.amount * exchange_rate
        };
        let expected_profit = amount * min_profit.unwrap_or_default();
        if target_luna.checked_sub(amount).unwrap_or_default() < expected_profit {
            return Err(StdError::generic_err("target luna is less than expected"));
        }

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hub.token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: hub.hub_address.to_string(),
                msg: to_binary(&HubCw20HookMsg::Unbond {})?,
                amount,
            })?,
            funds: vec![],
        }));

        let current_batch = query_hub_current_batch(
            &deps.querier,
            hub.hub_address.to_string(),
        )?;
        current_batch.id
    };

    state.burn_counter += 1u64;
    let burn = Burn {
        id: state.burn_counter,
        batch_id,
        input_amount: amount,
        start_burn: env.block.time.seconds(),
        end_burn: env.block.time.seconds() + config.burn_period,
        hub_type: hub.hub_type,
        hub_address: hub.hub_address,
    };
    burn_store(deps.storage).save(&burn.id.to_be_bytes(), &burn)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("batch_id", batch_id.to_string())
    )
}

pub fn collect(
    deps: DepsMut,
    env: Env,
) -> StdResult<Response> {
    let burns = query_burns(deps.as_ref())?;
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut collected_ids: HashSet<u64> = HashSet::new();
    let (total_input_amount, total_collectable_amount) =
        collect_internal(&deps.querier, &env, &burns, &mut messages, &mut collected_ids)?;
    if total_input_amount.is_zero() {
        return Ok(Response::default());
    }

    for collected_id in collected_ids {
        burn_store(deps.storage).remove(&collected_id.to_be_bytes());
    }

    let balance = deps.querier.query_balance(env.contract.address.to_string(), "uluna")?;

    Ok(Response::new()
        .add_messages(messages)
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::collect_hook {
                prev_balance: balance.amount,
                total_input_amount,
                total_collectable_amount,
            })?,
            funds: vec![],
        }))
        .add_attribute("total_input_amount", total_input_amount.to_string())
        .add_attribute("total_collectable_amount", total_collectable_amount.to_string())
    )
}

pub fn collect_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    prev_balance: Uint128,
    total_input_amount: Uint128,
    total_collectable_amount: Uint128,
) -> StdResult<Response> {

    // must be self
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    // validate collected amount
    let balance = deps.querier.query_balance(env.contract.address, "uluna")?;
    let collected_amount = balance.amount.checked_sub(prev_balance)?;
    if collected_amount < total_collectable_amount {
        return Err(StdError::generic_err("collect less than expect"));
    }

    // earning
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;
    if collected_amount >= total_input_amount {
        let total_fee = config.controller_fee + config.community_fee + config.platform_fee;
        let earn = collected_amount.checked_sub(total_input_amount)?;
        let fee = earn * total_fee;
        state.perf_fee += fee;
        state.total_bond_amount += earn.checked_sub(fee)?;
    } else {
        let loss = total_input_amount.checked_sub(collected_amount)?;
        state.total_bond_amount = state.total_bond_amount.checked_sub(loss)?;
    }
    update_claimable(balance.amount, &mut state)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::default())
}

fn collect_internal(
    querier: &QuerierWrapper,
    env: &Env,
    burns: &[Burn],
    messages: &mut Vec<CosmosMsg>,
    collected_ids: &mut HashSet<u64>,
) -> StdResult<(Uint128, Uint128)> {

    let now = env.block.time.seconds();
    let mut total_input_amount = Uint128::zero();
    let mut total_collectable_amount = Uint128::zero();
    let mut stader_batch_map: HashMap<u64, bool> = HashMap::new();
    let mut hub_batch_map: HashMap<HubType, u64> = HashMap::new();

    for burn in burns {
        if burn.end_burn > now {
            continue;
        }

        if burn.hub_type == HubType::lunax {

            // same batch check
            let same_batch_result = stader_batch_map.get(&burn.batch_id);
            if let Some(success) = same_batch_result {
                if *success {
                    total_input_amount += burn.input_amount;
                    collected_ids.insert(burn.id);
                }
                continue;
            }
            stader_batch_map.insert(burn.batch_id, false);

            // readiness check
            let batch = query_stader_batch(
                querier,
                burn.hub_address.to_string(),
                burn.batch_id)?;
            if let Some(batch) = batch.batch {
                if !batch.reconciled {
                    continue;
                }
            } else {
                continue;
            }

            // claim
            let record = query_stader_claimable(
                querier,
                burn.hub_address.to_string(),
                env.contract.address.to_string(),
                burn.batch_id)?;
            total_input_amount += burn.input_amount;
            total_collectable_amount += record.user_withdrawal_amount;
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: burn.hub_address.to_string(),
                msg: to_binary(&StaderExecuteMsg::WithdrawFundsToWallet {
                    batch_id: burn.batch_id,
                })?,
                funds: vec![],
            }));
            stader_batch_map.insert(burn.batch_id, true);
            collected_ids.insert(burn.id);
        } else {

            // claimed batch check
            let hub_type = if burn.hub_type == HubType::stluna { HubType::bluna } else { burn.hub_type };
            let hub_result = hub_batch_map.get(&hub_type);
            if let Some(last_success_batch) = hub_result {
                if *last_success_batch >= burn.batch_id {
                    total_input_amount += burn.input_amount;
                    collected_ids.insert(burn.id);
                }
                continue;
            }

            // get last released
            let histories = query_hub_histories(querier, burn.hub_address.to_string(), burn.batch_id)?;
            let last_released = histories.history.into_iter()
                .filter(|it| it.released)
                .last();
            let last_success_batch = match last_released {
                None => 0u64,
                Some(last_released) => last_released.batch_id
            };
            hub_batch_map.insert(hub_type, last_success_batch);
            if last_success_batch < burn.batch_id {
                continue;
            }

            let claimable = query_hub_claimable(querier, burn.hub_address.to_string(), env.contract.address.to_string())?;
            total_input_amount += burn.input_amount;
            total_collectable_amount += claimable.withdrawable;
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: burn.hub_address.to_string(),
                msg: to_binary(&HubExecuteMsg::WithdrawUnbonded {})?,
                funds: vec![],
            }));
            collected_ids.insert(burn.id);
        }
    }

    Ok((total_input_amount, total_collectable_amount))
}

pub fn collect_fee(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut state = read_state(deps.storage)?;
    let perf_fee = state.perf_fee;
    let deposit_fee = state.deposit_fee;
    let net_commission_amount = perf_fee + deposit_fee;
    state.perf_fee = Uint128::zero();
    state.deposit_fee = Uint128::zero();

    let ust_pair_contract = deps.api.addr_humanize(&config.ust_pair_contract)?;

    // swap
    let mut messages: Vec<CosmosMsg> = vec![];
    let offer_asset = Asset {
        amount: net_commission_amount,
        info: AssetInfo::NativeToken { denom: "uluna".to_string() },
    };
    let simulate = simulate(&deps.querier, ust_pair_contract.clone(), &offer_asset)?;
    let earning = deduct_tax(&deps.querier, simulate.return_amount, "uusd".to_string())?;
    let deposit_fee_ratio = Decimal::from_ratio(deposit_fee, net_commission_amount);
    let earning_for_deposit_fee = earning * deposit_fee_ratio;
    let earning_for_perf_fee = earning.checked_sub(earning_for_deposit_fee)?;
    state.perf_earning += earning_for_perf_fee;
    state.deposit_earning += earning_for_deposit_fee;

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: ust_pair_contract.to_string(),
        msg: to_binary(&PairExecuteMsg::Swap {
            offer_asset,
            to: None,
            max_spread: None,
            belief_price: None,
        })?,
        funds: vec![
            Coin { denom: "uluna".to_string(), amount: net_commission_amount },
        ],
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
        msg: to_binary(&ExecuteMsg::send_fee {
            deposit_fee_ratio,
        })?,
        funds: vec![],
    }));

    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(messages))
}

pub fn send_fee(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    deposit_fee_ratio: Decimal,
) -> StdResult<Response> {

    // only farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config = read_config(deps.storage)?;
    let aust_token = deps.api.addr_humanize(&config.aust_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let aust_balance = query_token_balance(&deps.querier, aust_token.clone(), env.contract.address)?;
    let balance_for_deposit_fee = aust_balance * deposit_fee_ratio;
    let balance_for_perf_fee = aust_balance.checked_sub(balance_for_deposit_fee)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let thousand = Uint128::from(1000u64);
    let total_fee = config.community_fee + config.controller_fee + config.platform_fee;
    let community_amount = balance_for_perf_fee.multiply_ratio(thousand * config.community_fee, thousand * total_fee);
    if !community_amount.is_zero() || !balance_for_deposit_fee.is_zero() {
        let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: spectrum_gov.to_string(),
                amount: community_amount + balance_for_deposit_fee,
            })?,
            funds: vec![],
        });
        messages.push(transfer_community_fee);
    }

    let platform_amount = balance_for_perf_fee.multiply_ratio(thousand * config.platform_fee, thousand * total_fee);
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

    let controller_amount = balance_for_perf_fee.checked_sub(community_amount + platform_amount)?;
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

pub fn query_burns(deps: Deps) -> StdResult<Vec<Burn>> {
    burns_read(deps.storage).range(None, None, Order::Ascending)
        .map(|item| {
            let (_, v) = item?;
            Ok(v)
        })
        .collect()
}

pub fn simulate_collect(deps: Deps, env: Env) -> StdResult<SimulateCollectResponse> {
    let burns = burns_read(deps.storage).range(None, None, Order::Ascending)
        .map(|item| {
            let (_, v) = item?;
            Ok(v)
        })
        .collect::<StdResult<Vec<Burn>>>()?;
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut collected_ids: HashSet<u64> = HashSet::new();
    let (total_input_amount, total_collectable_amount) =
        collect_internal(&deps.querier, &env, &burns, &mut messages, &mut collected_ids)?;

    let balance = deps.querier.query_balance(env.contract.address.to_string(), "uluna")?;
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    if total_collectable_amount >= total_input_amount {
        let total_fee = config.controller_fee + config.community_fee + config.platform_fee;
        let earn = total_collectable_amount.checked_sub(total_input_amount)?;
        let fee = earn * total_fee;
        state.perf_fee += fee;
        state.total_bond_amount += earn.checked_sub(fee)?;
    } else {
        let loss = total_input_amount.checked_sub(total_collectable_amount)?;
        state.total_bond_amount = state.total_bond_amount.checked_sub(loss)?;
    }

    let new_balance = balance.amount + total_collectable_amount;
    update_claimable(new_balance, &mut state)?;

    Ok(SimulateCollectResponse {
        can_collect: !total_input_amount.is_zero(),
        total_bond_amount: state.total_bond_amount,
        burnable: state.get_burnable_amount(new_balance),
        unbonded_index: state.unbonded_index,
        remaining_burns: burns.into_iter()
            .filter(|it| !collected_ids.contains(&it.id))
            .collect(),
    })
}
