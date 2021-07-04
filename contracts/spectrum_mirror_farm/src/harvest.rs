use std::collections::HashMap;

use cosmwasm_std::{
    log, to_binary, Api, CanonicalAddr, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, LogAttribute, Querier, StdError, Storage, Uint128, WasmMsg,
};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store},
};

use crate::querier::query_mirror_reward_info;

use cw20::Cw20HandleMsg;

use crate::state::{pool_info_read, pool_info_store, read_state};
use mirror_protocol::gov::Cw20HookMsg as MirrorGovCw20HookMsg;
use mirror_protocol::staking::HandleMsg as MirrorHandleMsg;
use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, HandleMsg as GovHandleMsg};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    Cw20HookMsg as TerraswapCw20HookMsg, HandleMsg as TerraswapHandleMsg, SimulationResponse,
};
use terraswap::querier::{query_pair_info, simulate};

// harvest all
pub fn harvest_all<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> HandleResult {
    let config = read_config(&deps.storage)?;

    if config.controller != CanonicalAddr::default()
        && config.controller != deps.api.canonical_address(&env.message.sender)?
    {
        return Err(StdError::unauthorized());
    }

    let terraswap_factory = deps.api.human_address(&config.terraswap_factory)?;
    let mirror_staking = deps.api.human_address(&config.mirror_staking)?;
    let mirror_token = deps.api.human_address(&config.mirror_token)?;
    let mirror_gov = deps.api.human_address(&config.mirror_gov)?;
    let spectrum_token = deps.api.human_address(&config.spectrum_token)?;
    let spectrum_gov = deps.api.human_address(&config.spectrum_gov)?;

    let mirror_reward_infos =
        query_mirror_reward_info(&deps, &mirror_staking, &env.contract.address)?;

    let mut total_mir_swap_amount = Uint128::zero();
    let mut total_mir_stake_amount = Uint128::zero();
    let mut total_mir_commission = Uint128::zero();
    let mut swap_amount_map: HashMap<HumanAddr, Uint128> = HashMap::new();
    let mut stake_amount_pairs: Vec<(HumanAddr, Uint128)> = vec![];

    let mut logs: Vec<LogAttribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = if config.platform == CanonicalAddr::default() {
        Decimal::zero()
    } else {
        config.platform_fee
    };
    let controller_fee = if config.controller == CanonicalAddr::default() {
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
        let mirror_amount = (reward - commission)?;
        total_mir_commission += commission;
        total_mir_swap_amount += commission;

        let asset_token_raw = deps
            .api
            .canonical_address(&mirror_reward_info.asset_token)?;
        let pool_info = pool_info_read(&deps.storage).load(asset_token_raw.as_slice())?;
        let auto_bond_amount =
            (mirror_reward_info.bond_amount - pool_info.total_stake_bond_amount)?;

        let swap_amount =
            mirror_amount.multiply_ratio(auto_bond_amount, mirror_reward_info.bond_amount);
        let stake_amount = (mirror_amount - swap_amount)?;

        // logs.push(log("asset_token", mirror_reward_info.asset_token.as_str()));
        // logs.push(log("reward", reward.to_string()));
        // logs.push(log("commission", commission.to_string()));
        // logs.push(log("mirror_amount", mirror_amount.to_string()));
        // logs.push(log("swap_amount", swap_amount.to_string()));
        // logs.push(log("stake_amount", stake_amount.to_string()));

        swap_amount_map.insert(mirror_reward_info.asset_token.clone(), swap_amount);
        if !stake_amount.is_zero() {
            stake_amount_pairs.push((mirror_reward_info.asset_token.clone(), stake_amount));
        }
        if mirror_reward_info.asset_token != mirror_token {
            total_mir_swap_amount += swap_amount;
        }
        total_mir_stake_amount += stake_amount
    }

    logs.push(log(
        "total_mir_commission",
        total_mir_commission.to_string(),
    ));
    logs.push(log(
        "total_mir_swap_amount",
        total_mir_swap_amount.to_string(),
    ));
    logs.push(log(
        "total_mir_stake_amount",
        total_mir_stake_amount.to_string(),
    ));

    deposit_farm_share(deps, &config, stake_amount_pairs)?;

    let mir_pair_info = query_pair_info(
        &deps,
        &terraswap_factory,
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: mirror_token.clone(),
            },
        ],
    )?;
    //Find Swap Rate
    let mir = Asset {
        info: AssetInfo::Token {
            contract_addr: mirror_token.clone(),
        },
        amount: total_mir_swap_amount,
    };
    let mir_swap_rate = simulate(&deps, &mir_pair_info.contract_addr, &mir)?;
    //

    let return_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: mir_swap_rate.return_amount,
    };
    let total_ust_return_amount = return_asset.deduct_tax(deps)?.amount;
    let total_ust_commission_amount =
        total_ust_return_amount.multiply_ratio(total_mir_commission, total_mir_swap_amount);

    // logs.push(log(
    //     "return_amount",
    //     mir_swap_rate.return_amount.to_string(),
    // ));
    logs.push(log(
        "total_ust_return_amount",
        total_ust_return_amount.to_string(),
    ));
    // logs.push(log(
    //     "total_ust_commission_amount",
    //     total_ust_commission_amount.to_string(),
    // ));

    for mirror_reward_info in mirror_reward_infos.reward_infos.iter() {
        let asset_token_raw = deps
            .api
            .canonical_address(&mirror_reward_info.asset_token)?;
        let mut pool_info = pool_info_read(&deps.storage).load(asset_token_raw.as_slice())?;
        let swap_amount = swap_amount_map
            .remove(&mirror_reward_info.asset_token)
            .unwrap();

        // logs.push(log("asset_token", mirror_reward_info.asset_token.as_str()));
        // logs.push(log("swap_amount", swap_amount.to_string()));
        if mirror_reward_info.asset_token == mirror_token {
            pool_info.reinvest_allowance += swap_amount;
        } else {
            let reinvest_allowance =
                total_ust_return_amount.multiply_ratio(swap_amount, total_mir_swap_amount);
            pool_info.reinvest_allowance += reinvest_allowance;
            // logs.push(log("reinvest_allowance", reinvest_allowance.to_string()));
        }
        pool_info_store(&mut deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    }

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_mir: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mirror_staking,
        send: vec![],
        msg: to_binary(&MirrorHandleMsg::Withdraw { asset_token: None })?,
    });
    messages.push(withdraw_all_mir);

    if !total_mir_swap_amount.is_zero() {
        let swap_mir: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_token.clone(),
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: mir_pair_info.contract_addr,
                amount: total_mir_swap_amount,
                msg: Some(to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?),
            })?,
            send: vec![],
        });
        messages.push(swap_mir);
    }

    if !total_ust_commission_amount.is_zero() {
        let spec_pair_info = query_pair_info(
            &deps,
            &terraswap_factory,
            &[
                AssetInfo::NativeToken {
                    denom: config.base_denom.clone(),
                },
                AssetInfo::Token {
                    contract_addr: spectrum_token.clone(),
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
            amount: commission.deduct_tax(deps)?.amount,
        };

        let mut state = read_state(&deps.storage)?;
        state.earning += net_commission.amount;
        state_store(&mut deps.storage).save(&state)?;

        let spec_swap_rate: SimulationResponse =
            simulate(&deps, &spec_pair_info.contract_addr, &net_commission)?;
        //

        logs.push(log("net_commission", net_commission.amount.to_string()));
        logs.push(log(
            "spec_commission",
            spec_swap_rate.return_amount.to_string(),
        ));

        let swap_spec = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spec_pair_info.contract_addr,
            msg: to_binary(&TerraswapHandleMsg::Swap {
                offer_asset: net_commission.clone(),
                max_spread: None,
                belief_price: None,
                to: None,
            })?,
            send: vec![Coin {
                denom: config.base_denom.clone(),
                amount: net_commission.amount,
            }],
        });
        messages.push(swap_spec);

        let mint = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: spectrum_gov.clone(),
            msg: to_binary(&GovHandleMsg::mint {})?,
            send: vec![],
        });
        messages.push(mint);

        let thousand = Uint128::from(1000u64);
        let community_amount = spec_swap_rate
            .return_amount
            .multiply_ratio(thousand * community_fee, thousand * total_fee);
        if !community_fee.is_zero() {
            let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.clone(),
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: spectrum_gov.clone(),
                    amount: community_amount,
                })?,
                send: vec![],
            });
            messages.push(transfer_community_fee);
        }

        let platform_amount = spec_swap_rate
            .return_amount
            .multiply_ratio(thousand * platform_fee, thousand * total_fee);
        if !platform_fee.is_zero() {
            let stake_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.clone(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: spectrum_gov.clone(),
                    amount: platform_amount,
                    msg: Some(to_binary(&GovCw20HookMsg::stake_tokens {
                        staker_addr: Some(deps.api.human_address(&config.platform)?),
                    })?),
                })?,
                send: vec![],
            });
            messages.push(stake_platform_fee);
        }

        if !controller_fee.is_zero() {
            let controller_amount =
                (spec_swap_rate.return_amount - (community_amount + platform_amount))?;
            let stake_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.clone(),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: spectrum_gov.clone(),
                    amount: controller_amount,
                    msg: Some(to_binary(&GovCw20HookMsg::stake_tokens {
                        staker_addr: Some(deps.api.human_address(&config.controller)?),
                    })?),
                })?,
                send: vec![],
            });
            messages.push(stake_controller_fee);
        }
    }

    if !total_mir_stake_amount.is_zero() {
        let stake_mir = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mirror_token.clone(),
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: mirror_gov,
                amount: total_mir_stake_amount,
                msg: Some(to_binary(&MirrorGovCw20HookMsg::StakeVotingTokens {})?),
            })?,
        });
        messages.push(stake_mir);
    }
    let response = HandleResponse {
        messages,
        log: logs,
        data: None,
    };
    Ok(response)
}
