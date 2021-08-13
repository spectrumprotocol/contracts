use std::collections::HashMap;

use cosmwasm_std::{
    attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, Decimal, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
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
use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, ExecuteMsg as GovExecuteMsg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg, SimulationResponse,
};
use terraswap::querier::{query_pair_info, simulate};

// harvest all
pub fn harvest_all(mut deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != CanonicalAddr::from(vec![])
        && config.controller != deps.api.addr_canonicalize(&info.sender.as_str())?
    {
        return Err(StdError::generic_err("unauthorized"));
    }

    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let mirror_staking = deps.api.addr_humanize(&config.mirror_staking)?;
    let mirror_token = deps.api.addr_humanize(&config.mirror_token)?;
    let mirror_gov = deps.api.addr_humanize(&config.mirror_gov)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

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
    let platform_fee = if config.platform == CanonicalAddr::from(vec![]) {
        Decimal::zero()
    } else {
        config.platform_fee
    };
    let controller_fee = if config.controller == CanonicalAddr::from(vec![]) {
        Decimal::zero()
    } else {
        config.controller_fee
    };
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

    attributes.push(attr(
        "total_mir_commission",
        total_mir_commission.to_string(),
    ));
    attributes.push(attr(
        "total_mir_swap_amount",
        total_mir_swap_amount.to_string(),
    ));
    attributes.push(attr(
        "total_mir_stake_amount",
        total_mir_stake_amount.to_string(),
    ));

    deposit_farm_share(deps.branch(), &config, stake_amount_pairs)?;

    let mir_pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory.clone(),
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
    //

    let return_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: mir_swap_rate.return_amount,
    };
    let total_ust_return_amount = return_asset.deduct_tax(&deps.querier)?.amount;
    let total_ust_commission_amount =
        total_ust_return_amount.multiply_ratio(total_mir_commission, total_mir_swap_amount);

    attributes.push(attr(
        "total_ust_return_amount",
        total_ust_return_amount.to_string(),
    ));

    for mirror_reward_info in mirror_reward_infos.reward_infos.iter() {
        let asset_token_raw = deps
            .api
            .addr_canonicalize(&mirror_reward_info.asset_token)?;
        let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
        let swap_amount = swap_amount_map
            .remove(&mirror_reward_info.asset_token)
            .unwrap();

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
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_mir);
    }

    if !total_ust_commission_amount.is_zero() {
        let spec_pair_info = query_pair_info(
            &deps.querier,
            terraswap_factory,
            &[
                AssetInfo::NativeToken {
                    denom: config.base_denom.clone(),
                },
                AssetInfo::Token {
                    contract_addr: spectrum_token.to_string(),
                },
            ],
        )?;

        //Find Spec Swap Rate
        let commission = Asset {
            info: AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            amount: total_ust_commission_amount,
        };
        let net_commission = Asset {
            info: AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            amount: commission.deduct_tax(&deps.querier)?.amount,
        };

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission.amount;
        state_store(deps.storage).save(&state)?;

        let spec_swap_rate: SimulationResponse = simulate(
            &deps.querier,
            deps.api.addr_validate(&spec_pair_info.contract_addr)?,
            &net_commission,
        )?;
        //

        attributes.push(attr("net_commission", net_commission.amount.to_string()));
        attributes.push(attr(
            "spec_commission",
            spec_swap_rate.return_amount.to_string(),
        ));

        let swap_spec = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spec_pair_info.contract_addr,
            msg: to_binary(&TerraswapExecuteMsg::Swap {
                offer_asset: net_commission.clone(),
                max_spread: None,
                belief_price: None,
                to: None,
            })?,
            funds: vec![Coin {
                denom: config.base_denom.clone(),
                amount: net_commission.amount,
            }],
        });
        messages.push(swap_spec);

        let mint = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_gov.to_string(),
            msg: to_binary(&GovExecuteMsg::mint {})?,
            funds: vec![],
        });
        messages.push(mint);

        let thousand = Uint128::from(1000u64);
        let community_amount = spec_swap_rate
            .return_amount
            .multiply_ratio(thousand * community_fee, thousand * total_fee);
        if !community_fee.is_zero() {
            let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: spectrum_gov.to_string(),
                    amount: community_amount,
                })?,
                funds: vec![],
            });
            messages.push(transfer_community_fee);
        }

        let platform_amount = spec_swap_rate
            .return_amount
            .multiply_ratio(thousand * platform_fee, thousand * total_fee);
        if !platform_fee.is_zero() {
            let stake_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: spectrum_gov.to_string(),
                    amount: platform_amount,
                    msg: to_binary(&GovCw20HookMsg::stake_tokens {
                        staker_addr: Some(deps.api.addr_humanize(&config.platform)?.to_string()),
                    })?,
                })?,
                funds: vec![],
            });
            messages.push(stake_platform_fee);
        }

        if !controller_fee.is_zero() {
            let controller_amount = spec_swap_rate
                .return_amount
                .checked_sub(community_amount + platform_amount)?;
            let stake_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: spectrum_gov.to_string(),
                    amount: controller_amount,
                    msg: to_binary(&GovCw20HookMsg::stake_tokens {
                        staker_addr: Some(deps.api.addr_humanize(&config.controller)?.to_string()),
                    })?,
                })?,
                funds: vec![],
            });
            messages.push(stake_controller_fee);
        }
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
