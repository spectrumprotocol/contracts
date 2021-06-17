use cosmwasm_std::{
    log, to_binary, Api, CanonicalAddr, Coin, CosmosMsg, Env, Extern, HandleResponse, HandleResult,
    HumanAddr, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use crate::state::{read_config, Config, PoolInfo};

use cw20::Cw20HandleMsg;
use spectrum_protocol::mirror_farm::HandleMsg;

use crate::state::{pool_info_read, pool_info_store};
use mirror_protocol::staking::Cw20HookMsg as MirrorCw20HookMsg;

use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, HandleMsg as TerraswapHandleMsg};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};

pub fn try_re_invest<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_token: HumanAddr,
) -> StdResult<HandleResponse> {
    let config = read_config(&deps.storage)?;

    if config.controller != CanonicalAddr::default()
        && config.controller != deps.api.canonical_address(&env.message.sender)?
    {
        return Err(StdError::unauthorized());
    }

    if asset_token == deps.api.human_address(&config.mirror_token)? {
        re_invest_mir(deps, env, config, asset_token)
    } else {
        re_invest_asset(deps, env, config, asset_token)
    }
}

pub fn re_invest_asset<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    config: Config,
    asset_token: HumanAddr,
) -> StdResult<HandleResponse> {
    let terraswap_factory = deps.api.human_address(&config.terraswap_factory)?;

    let asset_token_raw = deps.api.canonical_address(&asset_token)?;

    let mut pool_info = pool_info_read(&deps.storage).load(asset_token_raw.as_slice())?;

    let reinvest_allowance = pool_info.reinvest_allowance;
    pool_info.reinvest_allowance = Uint128::zero();
    pool_info_store(&mut deps.storage).save(&asset_token_raw.as_slice(), &pool_info)?;

    let reinvest_allowance_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: reinvest_allowance,
    };
    let net_reinvest_allowance_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: reinvest_allowance_asset.deduct_tax(deps)?.amount,
    };

    let net_swap_amount = net_reinvest_allowance_asset
        .amount
        .multiply_ratio(1u128, 2u128);

    let net_swap_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: net_swap_amount,
    };

    let pair_info = query_pair_info(
        &deps,
        &terraswap_factory,
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: asset_token.clone(),
            },
        ],
    )?;

    let swap_rate = simulate(&deps, &pair_info.contract_addr.clone(), &net_swap_asset)?;

    let swap_asset_token = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr.clone(),
        msg: to_binary(&TerraswapHandleMsg::Swap {
            offer_asset: net_swap_asset,
            max_spread: None,
            belief_price: None,
            to: None,
        })?,
        send: vec![Coin {
            denom: config.base_denom.clone(),
            amount: net_swap_amount,
        }],
    });

    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: asset_token.clone(),
        msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
            spender: pair_info.contract_addr.clone(),
            amount: swap_rate.return_amount,
            expires: None,
        })?,
        send: vec![],
    });

    let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr,
        msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: asset_token.clone(),
                    },
                    amount: swap_rate.return_amount,
                },
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: config.base_denom.clone(),
                    },
                    amount: net_swap_amount,
                },
            ],
            slippage_tolerance: None,
        })?,
        send: vec![Coin {
            denom: config.base_denom,
            amount: net_swap_amount,
        }],
    });

    let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address,
        msg: to_binary(&HandleMsg::stake {
            asset_token: asset_token.clone(),
        })?,
        send: vec![],
    });

    let response = HandleResponse {
        messages: vec![
            swap_asset_token,
            increase_allowance,
            provide_liquidity,
            stake,
        ],
        log: vec![
            log("action", "re-invest"),
            log("asset_token", asset_token.as_str()),
            log("reinvest_allowance", reinvest_allowance.to_string()),
            log("provide_token_amount", swap_rate.return_amount.to_string()),
            log("provide_ust_amount", net_swap_amount.to_string()),
            log(
                "remaining_reinvest_allowance",
                pool_info.reinvest_allowance.to_string(),
            ),
        ],
        data: None,
    };
    Ok(response)
}

pub fn re_invest_mir<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    config: Config,
    mir_token: HumanAddr,
) -> StdResult<HandleResponse> {
    let terraswap_factory = deps.api.human_address(&config.terraswap_factory)?;

    let mir_token_raw = deps.api.canonical_address(&mir_token)?;

    let mut pool_info = pool_info_read(&deps.storage).load(mir_token_raw.as_slice())?;
    let reinvest_allowance = pool_info.reinvest_allowance;
    let swap_amount = reinvest_allowance.multiply_ratio(1u128, 2u128);

    let swap_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: mir_token.clone(),
        },
        amount: swap_amount,
    };

    let pair_info = query_pair_info(
        &deps,
        &terraswap_factory,
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: mir_token.clone(),
            },
        ],
    )?;

    let swap_rate = simulate(&deps, &pair_info.contract_addr, &swap_asset)?;

    let reinvest_ust = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: swap_rate.return_amount,
    };

    let return_reinvest_ust = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: reinvest_ust.deduct_tax(deps)?.amount,
    };

    let net_reinvest_ust = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: return_reinvest_ust.deduct_tax(deps)?.amount,
    };

    let swap_mir_rate = simulate(&deps, &pair_info.contract_addr, &net_reinvest_ust)?;

    let provide_mir = swap_mir_rate.return_amount;

    pool_info.reinvest_allowance = (swap_amount - provide_mir)?;
    pool_info_store(&mut deps.storage).save(&mir_token_raw.as_slice(), &pool_info)?;

    let swap_mir = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mir_token.clone(),
        msg: to_binary(&Cw20HandleMsg::Send {
            contract: pair_info.contract_addr.clone(),
            amount: swap_amount,
            msg: Some(to_binary(&TerraswapCw20HookMsg::Swap {
                max_spread: None,
                belief_price: None,
                to: None,
            })?),
        })?,
        send: vec![],
    });

    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mir_token.clone(),
        msg: to_binary(&Cw20HandleMsg::IncreaseAllowance {
            spender: pair_info.contract_addr.clone(),
            amount: provide_mir,
            expires: None,
        })?,
        send: vec![],
    });

    let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr,
        msg: to_binary(&TerraswapHandleMsg::ProvideLiquidity {
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: mir_token.clone(),
                    },
                    amount: provide_mir,
                },
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: config.base_denom.clone(),
                    },
                    amount: net_reinvest_ust.amount,
                },
            ],
            slippage_tolerance: None,
        })?,
        send: vec![Coin {
            denom: config.base_denom,
            amount: net_reinvest_ust.amount,
        }],
    });

    let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address,
        msg: to_binary(&HandleMsg::stake {
            asset_token: mir_token.clone(),
        })?,
        send: vec![],
    });

    let response = HandleResponse {
        messages: vec![swap_mir, increase_allowance, provide_liquidity, stake],
        log: vec![
            log("action", "re-invest"),
            log("asset_token", mir_token.as_str()),
            log("reinvest_allowance", reinvest_allowance.to_string()),
            log("provide_token_amount", provide_mir.to_string()),
            log("provide_ust_amount", net_reinvest_ust.amount.to_string()),
            log(
                "remaining_reinvest_allowance",
                pool_info.reinvest_allowance.to_string(),
            ),
        ],
        data: None,
    };
    Ok(response)
}

pub fn try_stake<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_token: HumanAddr,
) -> HandleResult {
    if &env.message.sender != &env.contract.address {
        return Err(StdError::unauthorized());
    }
    let config: Config = read_config(&deps.storage)?;
    let mirror_staking = deps.api.human_address(&config.mirror_staking)?;
    let asset_token_raw: CanonicalAddr = deps.api.canonical_address(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(&deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.human_address(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps, &staking_token, &env.contract.address)?;

    let response = HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.clone(),
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: mirror_staking,
                amount,
                msg: Some(to_binary(&MirrorCw20HookMsg::Bond {
                    asset_token: asset_token.clone(),
                })?),
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
