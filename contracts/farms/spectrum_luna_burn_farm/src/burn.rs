use cosmwasm_std::{Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError, StdResult, to_binary, Uint128, WasmMsg};
use cw20::Cw20ExecuteMsg;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::querier::simulate;
use terraswap::pair::{ExecuteMsg as PairExecuteMsg, Cw20HookMsg as PairHookMsg};
use crate::hub::{query_hub_current_batch, query_hub_state};
use crate::model::SwapOperation;
use crate::stader::query_stader_state;
use crate::state::{Burn, burn_store, hub_read, HubType, read_config, read_state, state_store};

pub fn burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    swap_operations: Vec<SwapOperation>,
) -> StdResult<Response> {

    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
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
        let stader_state = query_stader_state(
            &deps.querier,
            hub.hub_address.to_string())?;
        let target_luna = asset.amount * stader_state.exchange_rate;
        if target_luna <= amount {
            return Err(StdError::generic_err("loss"));
        }

        stader_state.current_undelegation_batch_id
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
        let target_luna = asset.amount * exchange_rate;
        if target_luna <= amount {
            return Err(StdError::generic_err("loss"));
        }

        let current_batch = query_hub_current_batch(
            &deps.querier,
            hub.hub_address.to_string(),
        )?;
        current_batch.id
    };

    let mut state = read_state(deps.storage)?;
    state.burn_counter += 1u64;
    let burn = Burn {
        batch_id,
        input_amount: amount,
        start_burn: env.block.time.seconds(),
        end_burn: env.block.time.seconds() + config.burn_period,
        hub_type: hub.hub_type,
        hub_address: hub.hub_address,
    };
    burn_store(deps.storage).save(&state.burn_counter.to_be_bytes(), &burn)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("batch_id", batch_id.to_string())
    )
}
