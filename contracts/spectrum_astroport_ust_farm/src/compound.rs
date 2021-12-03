use cosmwasm_std::{
    attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store}, querier::query_astroport_pool_balance,
};

use crate::querier::{query_astroport_pending_token};

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use spectrum_protocol::gov_proxy::{
    StakerInfoGovResponse, ExecuteMsg as GovProxyExecuteMsg
};
use astroport::generator::{
    ExecuteMsg as AstroportExecuteMsg, PendingTokenResponse, Cw20HookMsg as AstroportCw20HookMsg
};
use spectrum_protocol::astroport_ust_farm::ExecuteMsg;
use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, ExecuteMsg as GovExecuteMsg};
use astroport::asset::{Asset, AssetInfo};
use astroport::pair::{
    Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg, SimulationResponse,
};
use astroport::querier::{query_pair_info, simulate};

pub fn compound(deps: DepsMut, env: Env, info: MessageInfo, threshold_compound_astro: Uint128) -> StdResult<Response> {

    // let lp_asset_info = AssetInfo::cw20(&config.pair.share_token);
    // let prev_lp_amount = lp_asset_info.query_balfarm_tokene(&deps.querier, &env.contract.address)?;

    let config = read_config(deps.storage)?;

    if config.controller != CanonicalAddr::from(vec![])
        && config.controller != deps.api.addr_canonicalize(info.sender.as_str())?
    {
        return Err(StdError::generic_err("unauthorized"));
    }

    let terraswap_factory = deps.api.addr_humanize(&config.astroport_factory)?;
    let astroport_generator = deps.api.addr_humanize(&config.astroport_generator)?;
    let astroport_token = deps.api.addr_humanize(&config.farm_token)?;
    //let gov_proxy = deps.api.addr_humanize(&config.gov_proxy)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let mut pool_info = pool_info_read(deps.storage).load(config.farm_token.as_slice())?;


    let reward_info = query_astroport_pending_token(
        deps.as_ref(),
        &pool_info.staking_token,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        &config.astroport_generator
    )?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        &config.astroport_generator
    )?;

    let mut total_farm_token_swap_amount = Uint128::zero();
    let mut total_farm_token_stake_amount = Uint128::zero();
    let mut total_farm_token_commission = Uint128::zero();
    let mut compound_amount: Uint128 = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    // calculate auto-compound, auto-Stake, and commission in farm token
    let reward = reward_info.pending;
    if !reward.is_zero() && !lp_balance.is_zero() {
        let commission = reward * total_fee;
        let astroport_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_farm_token_commission += commission;
        total_farm_token_swap_amount += commission;

        let auto_bond_amount = reward_info
            .bond_amount
            .checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount =
            astroport_amount.multiply_ratio(auto_bond_amount, reward_info.bond_amount);
        let stake_amount = astroport_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_farm_token_stake_amount += stake_amount;
    }

    // if staked.pending_reward > threshold_compound_gov {
    //     let withdraw_pending_reward_gov: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
    //         contract_addr: deps.api.addr_humanize(&config.terraworld_gov)?.to_string(),
    //         funds: vec![],
    //         msg: to_binary(&TerraworldGovExecuteMsg::Withdraw {})?,
    //     });
    //     attributes.push(attr("gov_pending_reward", staked.pending_reward));
    //     messages.push(withdraw_pending_reward_gov);

    //     let reward = staked.pending_reward;
    //     let commission = reward * total_fee;
    //     let gov_reward = reward.checked_sub(commission)?;
    //     total_twd_commission += commission;
    //     total_twd_swap_amount += commission;
    //     total_twd_stake_amount += gov_reward;
    // }


    let mut state = read_state(deps.storage)?;
    deposit_farm_share(
        deps.as_ref(),
        &mut state,
        &mut pool_info,
        &config,
        total_farm_token_stake_amount,
    )?;
    state_store(deps.storage).save(&state)?;

    // get reinvest amount
    let reinvest_allowfarm_tokene = pool_info.reinvest_allowfarm_tokene + compound_amount;
    // split reinvest amount
    let swap_amount = reinvest_allowfarm_tokene.multiply_ratio(1u128, 2u128);
    // add commission to reinvest farm token to total swap amount
    total_farm_token_swap_amount += swap_amount;

    let farm_token_pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory.clone(),
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: astroport_token.to_string(),
            },
        ],
    )?;

    // find farm token swap rate
    let farm_token = Asset {
        info: AssetInfo::Token {
            contract_addr: astroport_token.to_string(),
        },
        amount: total_farm_token_swap_amount,
    };
    let farm_token_swap_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&farm_token_pair_info.contract_addr)?,
        &farm_token,
    )?;
    let return_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: farm_token_swap_rate.return_amount,
    };

    let total_ust_return_amount = return_asset.deduct_tax(&deps.querier)?.amount;
    let total_ust_commission_amount = if total_farm_token_swap_amount != Uint128::zero() {
        total_ust_return_amount.multiply_ratio(total_farm_token_commission, total_farm_token_swap_amount)
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
    let swap_farm_token_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&farm_token_pair_info.contract_addr)?,
        &net_reinvest_asset,
    )?;
    // calculate provided farm token from provided UST
    let provide_farm_token = swap_farm_token_rate.return_amount + swap_farm_token_rate.commission_amount;

    pool_info.reinvest_allowfarm_tokene = swap_amount.checked_sub(provide_farm_token)?;
    pool_info_store(deps.storage).save(config.farm_token.as_slice(), &pool_info)?;

    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_farm_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: astroport_generator.to_string(),
        funds: vec![],
        msg: to_binary(&astroportStakingExecuteMsg::Withdraw {})?,
    });
    messages.push(withdraw_all_farm_token);

    if !total_farm_token_swap_amount.is_zero() {
        let swap_farm_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astroport_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: farm_token_pair_info.contract_addr.clone(),
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

    if !total_farm_token_stake_amount.is_zero() {
        let stake_farm_token = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astroport_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_gov.to_string(),
                amount: total_farm_token_stake_amount,
                msg: to_binary(&astroportGovCw20HookMsg::StakeVotingTokens {})?,
            })?,
        });
        messages.push(stake_farm_token);
    }

    if !provide_farm_token.is_zero() {
        let increase_allowfarm_tokene = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astroport_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowfarm_tokene {
                spender: farm_token_pair_info.contract_addr.to_string(),
                amount: provide_farm_token,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowfarm_tokene);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: farm_token_pair_info.contract_addr,
            msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: astroport_token.to_string(),
                        },
                        amount: provide_farm_token,
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: config.base_denom.clone(),
                        },
                        amount: net_reinvest_ust,
                    },
                ],
                slippage_tolerfarm_tokene: None,
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
                asset_token: astroport_token.to_string(),
            })?,
            funds: vec![],
        });
        messages.push(stake);
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", astroport_token));
    attributes.push(attr("reinvest_allowfarm_tokene", reinvest_allowfarm_tokene));
    attributes.push(attr("provide_token_amount", provide_farm_token));
    attributes.push(attr("provide_ust_amount", net_reinvest_ust));
    attributes.push(attr(
        "remaining_reinvest_allowfarm_tokene",
        pool_info.reinvest_allowfarm_tokene,
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
    // only astroport farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let astroport_staking = deps.api.addr_humanize(&config.astroport_generator)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balfarm_tokene(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_staking.to_string(),
                amount,
                msg: to_binary(&astroportStakingCw20HookMsg::Bond {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("asset_token", asset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}

//claim reward function which withdraw zero from generator
pub fn manual_claim_reward(
    deps: Deps,
    env: Env,
    lp_token: CanonicalAddr,
    staker: CanonicalAddr,
    astroport_generator: CanonicalAddr,
) -> StdResult<Response> {
    let pending_token_response: PendingTokenResponse = query_astroport_pending_token(deps, &lp_token, &staker, &astroport_generator)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&astroport_generator)?.to_string(),
            funds: vec![],
            msg: to_binary(&AstroportExecuteMsg::Withdraw {
                lp_token: deps.api.addr_humanize(&lp_token)?,
                amount: Uint128::zero()
            })?,
        })])
        .add_attributes(vec![
            attr("action", "claim_reward"),
            attr("lp_token", lp_token.to_string()),
            attr("pending_token_response.pending_on_proxy", pending_token_response.pending_on_proxy.unwrap_or_else(Uint128::zero)),
            attr("pending_token_response.pending", pending_token_response.pending),
        ]))
}