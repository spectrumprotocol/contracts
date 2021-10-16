use cosmwasm_std::{
    attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store},
};

use crate::querier::query_terraworld_reward_info;

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};

use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, ExecuteMsg as GovExecuteMsg};
use spectrum_protocol::terraworld_farm::ExecuteMsg;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg, SimulationResponse,
};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};
use terraworld_token::gov::{
    Cw20HookMsg as TerraworldGovCw20HookMsg, ExecuteMsg as TerraworldGovExecuteMsg,
    QueryMsg as TerraworldGovQueryMsg, StakerInfoResponse as TerraworldStakerInfoResponse,
};
use terraworld_token::staking::{
    Cw20HookMsg as TerraworldStakingCw20HookMsg, ExecuteMsg as TerraworldStakingExecuteMsg,
};

pub fn compound(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    threshold_compound_gov: Uint128,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != CanonicalAddr::from(vec![])
        && config.controller != deps.api.addr_canonicalize(info.sender.as_str())?
    {
        return Err(StdError::generic_err("unauthorized"));
    }

    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let terraworld_staking = deps.api.addr_humanize(&config.terraworld_staking)?;
    let terraworld_token = deps.api.addr_humanize(&config.terraworld_token)?;
    let terraworld_gov = deps.api.addr_humanize(&config.terraworld_gov)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let terraworld_reward_info = query_terraworld_reward_info(
        deps.as_ref(),
        &config.terraworld_staking,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        Some(env.block.height),
    )?;

    let mut total_twd_swap_amount = Uint128::zero();
    let mut total_twd_stake_amount = Uint128::zero();
    let mut total_twd_commission = Uint128::zero();
    let mut compound_amount: Uint128 = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let mut messages: Vec<CosmosMsg> = vec![];

    let staked: TerraworldStakerInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.addr_humanize(&config.terraworld_gov)?.to_string(),
            msg: to_binary(&TerraworldGovQueryMsg::StakerInfo {
                staker: env.contract.address.to_string(),
                block_height: Some(env.block.height),
            })?,
        }))?;

    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    if staked.pending_reward > threshold_compound_gov {
        let withdraw_pending_reward_gov: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.terraworld_gov)?.to_string(),
            funds: vec![],
            msg: to_binary(&TerraworldGovExecuteMsg::Withdraw {})?,
        });
        attributes.push(attr("gov_pending_reward", staked.pending_reward));
        messages.push(withdraw_pending_reward_gov);

        let reward = staked.pending_reward;
        let commission = reward * total_fee;
        let gov_reward = reward.checked_sub(commission)?;
        total_twd_commission += commission;
        total_twd_swap_amount += commission;
        total_twd_stake_amount += gov_reward;
    }

    // calculate auto-compound, auto-Stake, and commission in TWD
    let mut pool_info = pool_info_read(deps.storage).load(config.terraworld_token.as_slice())?;
    let reward = terraworld_reward_info.pending_reward;
    if !reward.is_zero() && !terraworld_reward_info.bond_amount.is_zero() {
        let commission = reward * total_fee;
        let terraworld_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_twd_commission += commission;
        total_twd_swap_amount += commission;

        let auto_bond_amount = terraworld_reward_info
            .bond_amount
            .checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount =
            terraworld_amount.multiply_ratio(auto_bond_amount, terraworld_reward_info.bond_amount);
        let stake_amount = terraworld_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_twd_stake_amount += stake_amount;
    }
    let mut state = read_state(deps.storage)?;
    deposit_farm_share(&staked, &mut state, &mut pool_info, total_twd_stake_amount)?;
    state_store(deps.storage).save(&state)?;

    // get reinvest amount
    let reinvest_allowance = pool_info.reinvest_allowance + compound_amount;
    // split reinvest amount
    let swap_amount = reinvest_allowance.multiply_ratio(1u128, 2u128);
    // add commission to reinvest TWD to total swap amount
    total_twd_swap_amount += swap_amount;

    let twd_pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory.clone(),
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: terraworld_token.to_string(),
            },
        ],
    )?;

    // find TWD swap rate
    let twd = Asset {
        info: AssetInfo::Token {
            contract_addr: terraworld_token.to_string(),
        },
        amount: total_twd_swap_amount,
    };
    let twd_swap_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&twd_pair_info.contract_addr)?,
        &twd,
    )?;
    let return_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: twd_swap_rate.return_amount,
    };

    let total_ust_return_amount = return_asset.deduct_tax(&deps.querier)?.amount;
    let total_ust_commission_amount = if total_twd_swap_amount != Uint128::zero() {
        total_ust_return_amount.multiply_ratio(total_twd_commission, total_twd_swap_amount)
    } else {
        Uint128::zero()
    };
    let total_ust_reinvest_amount =
        total_ust_return_amount.checked_sub(total_ust_commission_amount)?;

    // deduct tax for provided UST
    let net_reinvest_ust = deduct_tax(
        deps.as_ref(),
        total_ust_reinvest_amount,
        config.base_denom.clone(),
    );
    let net_reinvest_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: net_reinvest_ust,
    };
    let swap_twd_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&twd_pair_info.contract_addr)?,
        &net_reinvest_asset,
    )?;
    // calculate provided TWD from provided UST
    let provide_twd = swap_twd_rate.return_amount + swap_twd_rate.commission_amount;

    pool_info.reinvest_allowance = swap_amount.checked_sub(provide_twd)?;
    pool_info_store(deps.storage).save(config.terraworld_token.as_slice(), &pool_info)?;

    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let withdraw_all_twd: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: terraworld_staking.to_string(),
        funds: vec![],
        msg: to_binary(&TerraworldStakingExecuteMsg::Withdraw {})?,
    });
    messages.push(withdraw_all_twd);

    if !total_twd_swap_amount.is_zero() {
        let swap_twd: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraworld_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: twd_pair_info.contract_addr.clone(),
                amount: total_twd_swap_amount,
                msg: to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_twd);
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

        // find SPEC swap rate
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

        let spec_swap_rate: SimulationResponse = simulate(
            &deps.querier,
            deps.api.addr_validate(&spec_pair_info.contract_addr)?,
            &net_commission,
        )?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission.amount;
        state.earning_spec += spec_swap_rate.return_amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", net_commission.amount));
        attributes.push(attr("spec_commission", spec_swap_rate.return_amount));

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
                        days: None,
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
                        days: None,
                    })?,
                })?,
                funds: vec![],
            });
            messages.push(stake_controller_fee);
        }
    }

    if !total_twd_stake_amount.is_zero() {
        let stake_twd = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraworld_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: terraworld_gov.to_string(),
                amount: total_twd_stake_amount,
                msg: to_binary(&TerraworldGovCw20HookMsg::Bond {})?,
            })?,
        });
        messages.push(stake_twd);
    }

    if !provide_twd.is_zero() {
        let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: terraworld_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: twd_pair_info.contract_addr.to_string(),
                amount: provide_twd,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: twd_pair_info.contract_addr,
            msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: terraworld_token.to_string(),
                        },
                        amount: provide_twd,
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
                asset_token: terraworld_token.to_string(),
            })?,
            funds: vec![],
        });
        messages.push(stake);
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", terraworld_token));
    attributes.push(attr("reinvest_allowance", reinvest_allowance));
    attributes.push(attr("provide_token_amount", provide_twd));
    attributes.push(attr("provide_ust_amount", net_reinvest_ust));
    attributes.push(attr(
        "remaining_reinvest_allowance",
        pool_info.reinvest_allowance,
    ));

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

fn deduct_tax(deps: Deps, amount: Uint128, base_denom: String) -> Uint128 {
    let asset = Asset {
        info: AssetInfo::NativeToken {
            denom: base_denom.clone(),
        },
        amount,
    };
    let after_tax = Asset {
        info: AssetInfo::NativeToken { denom: base_denom },
        amount: asset.deduct_tax(&deps.querier).unwrap().amount,
    };
    after_tax.amount
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: String,
) -> StdResult<Response> {
    // only terraworld farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let terraworld_staking = deps.api.addr_humanize(&config.terraworld_staking)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: terraworld_staking.to_string(),
                amount,
                msg: to_binary(&TerraworldStakingCw20HookMsg::Bond {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("asset_token", asset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}
