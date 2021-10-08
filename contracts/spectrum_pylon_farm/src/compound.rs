use cosmwasm_std::{
    attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store},
};

use crate::querier::query_pylon_reward_info;

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use pylon_token::gov::Cw20HookMsg as PylonGovCw20HookMsg;
use pylon_token::staking::{
    Cw20HookMsg as PylonStakingCw20HookMsg, ExecuteMsg as PylonStakingExecuteMsg,
};
use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, ExecuteMsg as GovExecuteMsg};
use spectrum_protocol::pylon_farm::ExecuteMsg;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg, SimulationResponse,
};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};

pub fn compound(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != CanonicalAddr::from(vec![])
        && config.controller != deps.api.addr_canonicalize(info.sender.as_str())?
    {
        return Err(StdError::generic_err("unauthorized"));
    }

    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;
    let pylon_staking = deps.api.addr_humanize(&config.pylon_staking)?;
    let pylon_token = deps.api.addr_humanize(&config.pylon_token)?;
    let pylon_gov = deps.api.addr_humanize(&config.pylon_gov)?;
    let spectrum_token = deps.api.addr_humanize(&config.spectrum_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let pylon_reward_info = query_pylon_reward_info(
        deps.as_ref(),
        &config.pylon_staking,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        Some(env.block.height),
    )?;

    let mut total_mine_swap_amount = Uint128::zero();
    let mut total_mine_stake_amount = Uint128::zero();
    let mut total_mine_commission = Uint128::zero();
    let mut compound_amount: Uint128 = Uint128::zero();

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

    // calculate auto-compound, auto-Stake, and commission in MINE
    let mut pool_info = pool_info_read(deps.storage).load(&config.pylon_token.as_slice())?;
    let reward = pylon_reward_info.pending_reward;
    if !reward.is_zero() && !pylon_reward_info.bond_amount.is_zero() {
        let commission = reward * total_fee;
        let pylon_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_mine_commission += commission;
        total_mine_swap_amount += commission;

        let auto_bond_amount = pylon_reward_info
            .bond_amount
            .checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount =
            pylon_amount.multiply_ratio(auto_bond_amount, pylon_reward_info.bond_amount);
        let stake_amount = pylon_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_mine_stake_amount += stake_amount;
    }
    let mut state = read_state(deps.storage)?;
    deposit_farm_share(
        deps.as_ref(),
        &mut state,
        &mut pool_info,
        &config,
        total_mine_stake_amount,
    )?;
    state_store(deps.storage).save(&state)?;

    // get reinvest amount
    let reinvest_allowance = pool_info.reinvest_allowance + compound_amount;
    // split reinvest amount
    let swap_amount = reinvest_allowance.multiply_ratio(1u128, 2u128);
    // add commission to reinvest MINE to total swap amount
    total_mine_swap_amount += swap_amount;

    let mine_pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory.clone(),
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: pylon_token.to_string(),
            },
        ],
    )?;

    // find MINE swap rate
    let mine = Asset {
        info: AssetInfo::Token {
            contract_addr: pylon_token.to_string(),
        },
        amount: total_mine_swap_amount,
    };
    let mine_swap_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&mine_pair_info.contract_addr)?,
        &mine,
    )?;
    let return_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: mine_swap_rate.return_amount,
    };

    let total_ust_return_amount = return_asset.deduct_tax(&deps.querier)?.amount;
    let total_ust_commission_amount = if total_mine_swap_amount != Uint128::zero() {
        total_ust_return_amount.multiply_ratio(total_mine_commission, total_mine_swap_amount)
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
    let swap_mine_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&mine_pair_info.contract_addr)?,
        &net_reinvest_asset,
    )?;
    // calculate provided MINE from provided UST
    let provide_mine = swap_mine_rate.return_amount + swap_mine_rate.commission_amount;

    pool_info.reinvest_allowance = swap_amount.checked_sub(provide_mine)?;
    pool_info_store(deps.storage).save(&config.pylon_token.as_slice(), &pool_info)?;

    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_mine: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pylon_staking.to_string(),
        funds: vec![],
        msg: to_binary(&PylonStakingExecuteMsg::Withdraw {})?,
    });
    messages.push(withdraw_all_mine);

    if !total_mine_swap_amount.is_zero() {
        let swap_mine: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pylon_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: mine_pair_info.contract_addr.clone(),
                amount: total_mine_swap_amount,
                msg: to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_mine);
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

    if !total_mine_stake_amount.is_zero() {
        let stake_mine = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pylon_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pylon_gov.to_string(),
                amount: total_mine_stake_amount,
                msg: to_binary(&PylonGovCw20HookMsg::StakeVotingTokens {})?,
            })?,
        });
        messages.push(stake_mine);
    }

    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pylon_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: mine_pair_info.contract_addr.to_string(),
            amount: provide_mine,
            expires: None,
        })?,
        funds: vec![],
    });

    let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mine_pair_info.contract_addr,
        msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: pylon_token.to_string(),
                    },
                    amount: provide_mine,
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

    let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::stake {
            asset_token: pylon_token.to_string(),
        })?,
        funds: vec![],
    });

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", pylon_token));
    attributes.push(attr("reinvest_allowance", reinvest_allowance));
    attributes.push(attr("provide_token_amount", provide_mine));
    attributes.push(attr("provide_ust_amount", net_reinvest_ust));
    attributes.push(attr(
        "remaining_reinvest_allowance",
        pool_info.reinvest_allowance,
    ));

    messages.push(increase_allowance);
    messages.push(provide_liquidity);
    messages.push(stake);
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
    // only pylon farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let pylon_staking = deps.api.addr_humanize(&config.pylon_staking)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pylon_staking.to_string(),
                amount,
                msg: to_binary(&PylonStakingCw20HookMsg::Bond {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("asset_token", asset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}
