use cosmwasm_std::{
    log, to_binary, Api, CanonicalAddr, Coin, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HandleResult, String, LogAttribute, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store},
};

use crate::querier::query_pylon_reward_info;

use cw20::Cw20HandleMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use pylon_token::gov::Cw20HookMsg as PylonGovCw20HookMsg;
use pylon_token::staking::{
    Cw20HookMsg as PylonStakingCw20HookMsg, HandleMsg as PylonStakingHandleMsg,
};
use spectrum_protocol::gov::{Cw20HookMsg as GovCw20HookMsg, HandleMsg as GovHandleMsg};
use spectrum_protocol::pylon_farm::HandleMsg;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{
    Cw20HookMsg as TerraswapCw20HookMsg, HandleMsg as TerraswapHandleMsg, SimulationResponse,
};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};

pub fn compound<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let config = read_config(&deps.storage)?;

    if config.controller != CanonicalAddr::default()
        && config.controller != deps.api.canonical_address(&env.message.sender)?
    {
        return Err(StdError::unauthorized());
    }

    let terraswap_factory = deps.api.human_address(&config.terraswap_factory)?;
    let pylon_staking = deps.api.human_address(&config.pylon_staking)?;
    let pylon_token = deps.api.human_address(&config.pylon_token)?;
    let pylon_gov = deps.api.human_address(&config.pylon_gov)?;
    let spectrum_token = deps.api.human_address(&config.spectrum_token)?;
    let spectrum_gov = deps.api.human_address(&config.spectrum_gov)?;

    let pylon_reward_info = query_pylon_reward_info(
        &deps,
        &config.pylon_staking,
        &deps.api.canonical_address(&env.contract.address)?,
        env.block.height,
    )?;

    let mut total_mine_swap_amount = Uint128::zero();
    let mut total_mine_stake_amount = Uint128::zero();
    let mut total_mine_commission = Uint128::zero();
    let mut compound_amount: Uint128 = Uint128::zero();

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

    // calculate auto-compound, auto-Stake, and commission in MINE
    let mut pool_info = pool_info_read(&deps.storage).load(&config.pylon_token.as_slice())?;
    let reward = pylon_reward_info.pending_reward;
    if !reward.is_zero() && !pylon_reward_info.bond_amount.is_zero() {
        let commission = reward * total_fee;
        let pylon_amount = (reward - commission)?;
        // add commission to total swap amount
        total_mine_commission += commission;
        total_mine_swap_amount += commission;

        let auto_bond_amount = (pylon_reward_info.bond_amount - pool_info.total_stake_bond_amount)?;
        compound_amount =
            pylon_amount.multiply_ratio(auto_bond_amount, pylon_reward_info.bond_amount);
        let stake_amount = (pylon_amount - compound_amount)?;

        // logs.push(log("reward", reward.to_string()));
        logs.push(log("commission", commission.to_string()));
        // logs.push(log("pylon_amount", pylon_amount.to_string()));
        logs.push(log("compound_amount", compound_amount.to_string()));
        logs.push(log("stake_amount", stake_amount.to_string()));

        total_mine_stake_amount += stake_amount;
    }

    deposit_farm_share(deps, &mut pool_info, &config, total_mine_stake_amount)?;

    // get reinvest amount
    let reinvest_allowance = pool_info.reinvest_allowance + compound_amount;
    // split reinvest amount
    let swap_amount = reinvest_allowance.multiply_ratio(1u128, 2u128);
    // add commission to reinvest MINE to total swap amount
    total_mine_swap_amount += swap_amount;

    let mine_pair_info = query_pair_info(
        &deps,
        &terraswap_factory,
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: pylon_token.clone(),
            },
        ],
    )?;

    // find MINE swap rate
    let mine = Asset {
        info: AssetInfo::Token {
            contract_addr: pylon_token.clone(),
        },
        amount: total_mine_swap_amount,
    };
    let mine_swap_rate = simulate(&deps, &mine_pair_info.contract_addr, &mine)?;
    let return_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: mine_swap_rate.return_amount,
    };

    let total_ust_return_amount = return_asset.deduct_tax(deps)?.amount;
    let total_ust_commission_amount = if total_mine_swap_amount != Uint128::zero() {
        total_ust_return_amount.multiply_ratio(total_mine_commission, total_mine_swap_amount)
    } else {
        Uint128::zero()
    };
    let total_ust_reinvest_amount = (total_ust_return_amount - total_ust_commission_amount)?;

    // deduct tax for provided UST
    let net_reinvest_ust = deduct_tax(deps, total_ust_reinvest_amount, config.base_denom.clone());
    let net_reinvest_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: net_reinvest_ust,
    };
    let swap_mine_rate = simulate(&deps, &mine_pair_info.contract_addr, &net_reinvest_asset)?;
    // calculate provided MINE from provided UST
    let provide_mine = swap_mine_rate.return_amount + swap_mine_rate.commission_amount;

    pool_info.reinvest_allowance = (swap_amount - provide_mine)?;
    pool_info_store(&mut deps.storage).save(&config.pylon_token.as_slice(), &pool_info)?;

    logs.push(log(
        "total_ust_return_amount",
        total_ust_return_amount.to_string(),
    ));

    let mut messages: Vec<CosmosMsg> = vec![];
    let withdraw_all_mine: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pylon_staking,
        send: vec![],
        msg: to_binary(&PylonStakingHandleMsg::Withdraw {})?,
    });
    messages.push(withdraw_all_mine);

    if !total_mine_swap_amount.is_zero() {
        let swap_mine: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pylon_token.clone(),
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: mine_pair_info.contract_addr.clone(),
                amount: total_mine_swap_amount,
                msg: Some(to_binary(&TerraswapCw20HookMsg::Swap {
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })?),
            })?,
            send: vec![],
        });
        messages.push(swap_mine);
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
            amount: commission.deduct_tax(deps)?.amount,
        };

        let mut state = read_state(&deps.storage)?;
        state.earning += net_commission.amount;
        state_store(&mut deps.storage).save(&state)?;

        let spec_swap_rate: SimulationResponse =
            simulate(&deps, &spec_pair_info.contract_addr, &net_commission)?;

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

    if !total_mine_stake_amount.is_zero() {
        let stake_mine = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pylon_token.clone(),
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: pylon_gov,
                amount: total_mine_stake_amount,
                msg: Some(to_binary(&PylonGovCw20HookMsg::StakeVotingTokens {})?),
            })?,
        });
        messages.push(stake_mine);
    }

    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pylon_token.clone(),
        msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
            spender: mine_pair_info.contract_addr.clone(),
            amount: provide_mine,
            expires: None,
        })?,
        send: vec![],
    });

    let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mine_pair_info.contract_addr,
        msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: pylon_token.clone(),
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
        })?,
        send: vec![Coin {
            denom: config.base_denom,
            amount: net_reinvest_ust,
        }],
    });

    let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address,
        msg: to_binary(&HandleMsg::stake {
            asset_token: pylon_token.clone(),
        })?,
        send: vec![],
    });

    logs.push(log("action", "compound"));
    logs.push(log("asset_token", pylon_token.as_str()));
    logs.push(log("reinvest_allowance", reinvest_allowance.to_string()));
    logs.push(log("provide_token_amount", provide_mine.to_string()));
    logs.push(log("provide_ust_amount", net_reinvest_ust.to_string()));
    logs.push(log(
        "remaining_reinvest_allowance",
        pool_info.reinvest_allowance.to_string(),
    ));

    messages.push(increase_allowance);
    messages.push(provide_liquidity);
    messages.push(stake);

    let response = HandleResponse {
        messages,
        log: logs,
        data: None,
    };

    Ok(response)
}

fn deduct_tax<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    amount: Uint128,
    base_denom: String,
) -> Uint128 {
    let asset = Asset {
        info: AssetInfo::NativeToken {
            denom: base_denom.clone(),
        },
        amount,
    };
    let after_tax = Asset {
        info: AssetInfo::NativeToken {
            denom: base_denom.clone(),
        },
        amount: asset.deduct_tax(deps).unwrap().amount,
    };
    after_tax.amount
}

pub fn stake<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_token: String,
) -> HandleResult {
    // only pylon farm contract can execute this message
    if env.message.sender != env.contract.address {
        return Err(StdError::unauthorized());
    }
    let config: Config = read_config(&deps.storage)?;
    let pylon_staking = deps.api.human_address(&config.pylon_staking)?;
    let asset_token_raw: CanonicalAddr = deps.api.canonical_address(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(&deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.human_address(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps, &staking_token, &env.contract.address)?;

    let response = HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.clone(),
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: pylon_staking,
                amount,
                msg: Some(to_binary(&PylonStakingCw20HookMsg::Bond {})?),
            })?,
        })],
        log: vec![
            log("action", "stake"),
            log("asset_token", asset_token.as_str()),
            log("staking_token", staking_token.as_str()),
            log("amount", amount.to_string()),
        ],
        data: None,
    };
    Ok(response)
}
