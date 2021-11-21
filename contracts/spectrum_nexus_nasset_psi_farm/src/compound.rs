use cosmwasm_std::{
    attr, to_binary, Attribute, CanonicalAddr, CosmosMsg, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};

use crate::{
    bond::deposit_farm_share,
    querier::simulate_swap_operations,
    state::{read_config, state_store},
};

use crate::querier::query_nexus_reward_info;

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use nexus_token::governance::Cw20HookMsg as NexusGovCw20HookMsg;
use nexus_token::staking::{
    Cw20HookMsg as NexusStakingCw20HookMsg, ExecuteMsg as NexusStakingExecuteMsg,
};
use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, ExecuteMsg as GovExecuteMsg};
use spectrum_protocol::nexus_nasset_psi_farm::ExecuteMsg;
use terraswap::{pair::{Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg}, querier::reverse_simulate};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};
use terraswap::router::{Cw20HookMsg as TerraswapRouterCw20HookMsg, SimulateSwapOperationsResponse};
use terraswap::{
    asset::{Asset, AssetInfo},
    router::SwapOperation,
};

pub fn compound(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != CanonicalAddr::from(vec![])
        && config.controller != deps.api.addr_canonicalize(info.sender.as_str())?
    {
        return Err(StdError::generic_err("unauthorized"));
    }

    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let nasset_staking = deps.api.addr_humanize(&config.nasset_staking)?;
    let nexus_token = deps.api.addr_humanize(&config.nexus_token)?;
    let nexus_gov = deps.api.addr_humanize(&config.nexus_gov)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;
    let nasset_token = deps.api.addr_humanize(&config.nasset_token)?;
    let terraswap_router = deps.api.addr_humanize(&config.terraswap_router)?;

    let nexus_reward_info = query_nexus_reward_info(
        deps.as_ref(),
        &config.nasset_staking,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        Some(env.block.time.seconds()),
    )?;

    let mut total_psi_stake_amount = Uint128::zero();
    let mut total_psi_commission = Uint128::zero();
    let mut compound_amount: Uint128 = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    // calculate auto-compound, auto-Stake, and commission in PSI
    let mut pool_info = pool_info_read(deps.storage).load(config.nasset_token.as_slice())?;
    let reward = nexus_reward_info.pending_reward;
    if !reward.is_zero() && !nexus_reward_info.bond_amount.is_zero() {
        let commission = reward * total_fee;
        let nexus_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_psi_commission += commission;

        let auto_bond_amount = nexus_reward_info
            .bond_amount
            .checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount =
            nexus_amount.multiply_ratio(auto_bond_amount, nexus_reward_info.bond_amount);
        let stake_amount = nexus_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_psi_stake_amount += stake_amount;
    }
    let mut state = read_state(deps.storage)?;
    deposit_farm_share(
        deps.as_ref(),
        &mut state,
        &mut pool_info,
        &config,
        total_psi_stake_amount,
    )?;
    state_store(deps.storage).save(&state)?;

    // split reinvest amount
    let total_psi_swap_amount = compound_amount.multiply_ratio(1000u128, 1997u128);
    let provide_psi = compound_amount.checked_sub(total_psi_swap_amount)?;

    let nasset_psi_pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory.clone(),
        &[
            AssetInfo::Token {
                contract_addr: nasset_token.to_string(),
            },
            AssetInfo::Token {
                contract_addr: nexus_token.to_string(),
            },
        ],
    )?;

    // find PSI swap rate
    let psi = Asset {
        info: AssetInfo::Token {
            contract_addr: nexus_token.to_string(),
        },
        amount: total_psi_swap_amount,
    };
    let psi_swap_rate_to_nasset = simulate(
        &deps.querier,
        deps.api.addr_validate(&nasset_psi_pair_info.contract_addr)?,
        &psi,
    )?;

    let provide_nasset = psi_swap_rate_to_nasset.return_amount;

    pool_info_store(deps.storage).save(config.nasset_token.as_slice(), &pool_info)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_psi: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: nasset_staking.to_string(),
        funds: vec![],
        msg: to_binary(&NexusStakingExecuteMsg::Withdraw {})?,
    });
    messages.push(withdraw_all_psi);

    if !total_psi_swap_amount.is_zero() {
        let swap_psi: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: nexus_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: nasset_psi_pair_info.contract_addr.clone(),
                amount: total_psi_swap_amount,
                msg: to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_psi);
    }

    if !total_psi_commission.is_zero() {
        let psi_pair_info = query_pair_info(
            &deps.querier,
            terraswap_factory.clone(),
            &[
                AssetInfo::Token {
                    contract_addr: nexus_token.to_string(),
                },
                AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
        )?;

        // find SPEC swap rate
        let net_commission = Asset {
            info: AssetInfo::Token {
                contract_addr: nexus_token.to_string(),
            },
            amount: total_psi_commission,
        };

        let psi_swap_rate_to_uusd = simulate(
            &deps.querier,
            deps.api.addr_validate(&psi_pair_info.contract_addr)?,
            &net_commission)?;
        let spec_swap_rate = simulate_swap_operations(deps.as_ref(), total_psi_commission, &config.terraswap_router, &config.nexus_token, &config.spectrum_token)?;

        let mut state = read_state(deps.storage)?;
        state.earning += psi_swap_rate_to_nasset.return_amount;
        state.earning_spec += spec_swap_rate.amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", psi_swap_rate_to_uusd.return_amount));
        attributes.push(attr("spec_commission", spec_swap_rate.amount));

        let swap_spec = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: nexus_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: terraswap_router.to_string(),
                amount: net_commission.amount.clone(),
                msg: to_binary(&TerraswapRouterCw20HookMsg::ExecuteSwapOperations {
                    operations: vec![
                        SwapOperation::TerraSwap {
                            offer_asset_info: AssetInfo::Token {
                                contract_addr: nexus_token.to_string(),
                            },
                            ask_asset_info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                        },
                        SwapOperation::TerraSwap {
                            offer_asset_info: AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            ask_asset_info: AssetInfo::Token {
                                contract_addr: spectrum_token.to_string(),
                            },
                        },
                    ],
                    minimum_receive: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
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
            .amount
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
            .amount
            .multiply_ratio(thousand * platform_fee, thousand * total_fee);
        if !platform_fee.is_zero() {
            let stake_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: spectrum_gov.to_string(),
                    amount: platform_amount,
                    msg: to_binary(&GovCw20HookMsg::stake_tokens {
                        staker_addr: Some(deps.api.addr_humanize(&config.platform)?.to_string()),
                        days: None,
                    })?,
                })?,
                funds: vec![],
            });
            messages.push(stake_platform_fee);
        }

        if !controller_fee.is_zero() {
            let controller_amount = spec_swap_rate
                .amount
                .checked_sub(community_amount + platform_amount)?;
            let stake_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: spectrum_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: spectrum_gov.to_string(),
                    amount: controller_amount,
                    msg: to_binary(&GovCw20HookMsg::stake_tokens {
                        staker_addr: Some(deps.api.addr_humanize(&config.controller)?.to_string()),
                        days: None,
                    })?,
                })?,
                funds: vec![],
            });
            messages.push(stake_controller_fee);
        }
    }

    if !total_psi_stake_amount.is_zero() {
        let stake_psi = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: nexus_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: nexus_gov.to_string(),
                amount: total_psi_stake_amount,
                msg: to_binary(&NexusGovCw20HookMsg::StakeVotingTokens {})?,
            })?,
        });
        messages.push(stake_psi);
    }

    if !provide_nasset.is_zero() {
        let increase_allowance_psi = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: nexus_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: nasset_psi_pair_info.contract_addr.to_string(),
                amount: provide_psi,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance_psi);

        let increase_allowance_nasset = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: nasset_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: nasset_psi_pair_info.contract_addr.to_string(),
                amount: provide_nasset,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance_nasset);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: nasset_psi_pair_info.contract_addr,
            msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: nasset_token.to_string(),
                        },
                        amount: provide_nasset,
                    },
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: nexus_token.to_string(),
                        },
                        amount: provide_psi,
                    },
                ],
                slippage_tolerance: None,
                receiver: None,
            })?,
            funds: vec![],
        });
        messages.push(provide_liquidity);

        let stake = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::stake {
                asset_token: nasset_token.to_string(),
            })?,
            funds: vec![],
        });
        messages.push(stake);
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("nasset_token", nasset_token));
    attributes.push(attr("provide_nasset_amount", provide_nasset));
    attributes.push(attr("provide_psi_amount", provide_psi));

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    nasset_token: String,
) -> StdResult<Response> {
    // only nexus farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let nasset_staking = deps.api.addr_humanize(&config.nasset_staking)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&nasset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: nasset_staking.to_string(),
                amount,
                msg: to_binary(&NexusStakingCw20HookMsg::Bond {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("nasset_token", nasset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}
