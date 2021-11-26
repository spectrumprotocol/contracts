use cosmwasm_std::{
    attr, to_binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};

use crate::state::{read_config, Config, PoolInfo};

use cw20::Cw20ExecuteMsg;
use spectrum_protocol::astroport_farm::ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store};
use mirror_protocol::staking::Cw20HookMsg as MirrorCw20HookMsg;

use std::str::FromStr;
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::{Cw20HookMsg as TerraswapCw20HookMsg, ExecuteMsg as TerraswapExecuteMsg};
use terraswap::querier::{query_pair_info, query_token_balance, simulate};

const TERRASWAP_COMMISSION_RATE: &str = "0.003";

pub fn re_invest(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: String,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != CanonicalAddr::from(vec![])
        && config.controller != deps.api.addr_canonicalize(info.sender.as_str())?
    {
        return Err(StdError::generic_err("unauthorized"));
    }

    if asset_token == deps.api.addr_humanize(&config.mirror_token)? {
        re_invest_mir(deps, env, config, asset_token)
    } else {
        re_invest_asset(deps, env, config, asset_token)
    }
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

fn re_invest_asset(
    deps: DepsMut,
    env: Env,
    config: Config,
    asset_token: String,
) -> StdResult<Response> {
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;

    let asset_token_raw = deps.api.addr_canonicalize(&asset_token)?;

    let mut pool_info = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;

    let reinvest_allowance = pool_info.reinvest_allowance;
    let net_swap = reinvest_allowance.multiply_ratio(1u128, 2u128);
    let for_liquidity = reinvest_allowance.checked_sub(net_swap)?;
    let commission = for_liquidity * Decimal::from_str(TERRASWAP_COMMISSION_RATE)?;
    let net_liquidity = for_liquidity.checked_sub(commission)?;
    pool_info.reinvest_allowance = commission;
    pool_info_store(deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;

    let net_swap_after_tax = deduct_tax(deps.as_ref(), net_swap, config.base_denom.clone());
    let net_liquidity_after_tax =
        deduct_tax(deps.as_ref(), net_liquidity, config.base_denom.clone());

    let net_swap_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: net_swap_after_tax,
    };

    let pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory,
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: asset_token.clone(),
            },
        ],
    )?;

    let swap_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&pair_info.contract_addr)?,
        &net_swap_asset,
    )?;

    let swap_asset_token = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr.clone(),
        msg: to_binary(&TerraswapExecuteMsg::Swap {
            offer_asset: net_swap_asset,
            max_spread: None,
            belief_price: None,
            to: None,
        })?,
        funds: vec![Coin {
            denom: config.base_denom.clone(),
            amount: net_swap_after_tax,
        }],
    });

    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: asset_token.clone(),
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: pair_info.contract_addr.clone(),
            amount: swap_rate.return_amount,
            expires: None,
        })?,
        funds: vec![],
    });

    let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr,
        msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
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
                    amount: net_liquidity_after_tax,
                },
            ],
            slippage_tolerance: None,
            receiver: None,
        })?,
        funds: vec![Coin {
            denom: config.base_denom,
            amount: net_liquidity_after_tax,
        }],
    });

    let stake = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::stake {
            asset_token: asset_token.clone(),
        })?,
        funds: vec![],
    });
    Ok(Response::new()
        .add_messages(vec![
            swap_asset_token,
            increase_allowance,
            provide_liquidity,
            stake,
        ])
        .add_attributes(vec![
            attr("action", "re-invest"),
            attr("asset_token", asset_token),
            attr("reinvest_allowance", reinvest_allowance),
            attr("provide_token_amount", swap_rate.return_amount),
            attr("provide_ust_amount", net_liquidity_after_tax),
            attr("remaining_reinvest_allowance", pool_info.reinvest_allowance),
        ]))
}

fn re_invest_mir(
    deps: DepsMut,
    env: Env,
    config: Config,
    mir_token: String,
) -> StdResult<Response> {
    let terraswap_factory = deps.api.addr_humanize(&config.terraswap_factory)?;

    let mir_token_raw = deps.api.addr_canonicalize(&mir_token)?;

    let mut pool_info = pool_info_read(deps.storage).load(mir_token_raw.as_slice())?;
    let reinvest_allowance = pool_info.reinvest_allowance;
    let swap_amount = reinvest_allowance.multiply_ratio(1u128, 2u128);

    let swap_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: mir_token.clone(),
        },
        amount: swap_amount,
    };

    let pair_info = query_pair_info(
        &deps.querier,
        terraswap_factory,
        &[
            AssetInfo::NativeToken {
                denom: config.base_denom.clone(),
            },
            AssetInfo::Token {
                contract_addr: mir_token.clone(),
            },
        ],
    )?;

    let swap_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&pair_info.contract_addr)?,
        &swap_asset,
    )?;

    let net_reinvest_ust = deduct_tax(
        deps.as_ref(),
        deduct_tax(
            deps.as_ref(),
            swap_rate.return_amount,
            config.base_denom.clone(),
        ),
        config.base_denom.clone(),
    );
    let net_reinvest_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.base_denom.clone(),
        },
        amount: net_reinvest_ust,
    };
    let swap_mir_rate = simulate(
        &deps.querier,
        deps.api.addr_validate(&pair_info.contract_addr)?,
        &net_reinvest_asset,
    )?;

    let provide_mir = swap_mir_rate.return_amount + swap_mir_rate.commission_amount;

    pool_info.reinvest_allowance = swap_amount.checked_sub(provide_mir)?;
    pool_info_store(deps.storage).save(mir_token_raw.as_slice(), &pool_info)?;

    let swap_mir = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mir_token.clone(),
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: pair_info.contract_addr.clone(),
            amount: swap_amount,
            msg: to_binary(&TerraswapCw20HookMsg::Swap {
                max_spread: None,
                belief_price: None,
                to: None,
            })?,
        })?,
        funds: vec![],
    });

    let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mir_token.clone(),
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: pair_info.contract_addr.clone(),
            amount: provide_mir,
            expires: None,
        })?,
        funds: vec![],
    });

    let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr,
        msg: to_binary(&TerraswapExecuteMsg::ProvideLiquidity {
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
            asset_token: mir_token.clone(),
        })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_messages(vec![swap_mir, increase_allowance, provide_liquidity, stake])
        .add_attributes(vec![
            attr("action", "re-invest"),
            attr("asset_token", mir_token),
            attr("reinvest_allowance", reinvest_allowance),
            attr("provide_token_amount", provide_mir),
            attr("provide_ust_amount", net_reinvest_ust),
            attr("remaining_reinvest_allowance", pool_info.reinvest_allowance),
        ]))
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: String,
) -> StdResult<Response> {
    if info.sender.as_str() != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let mirror_staking = deps.api.addr_humanize(&config.mirror_staking)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: mirror_staking.to_string(),
                amount,
                msg: to_binary(&MirrorCw20HookMsg::Bond {
                    asset_token: asset_token.clone(),
                })?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("asset_token", asset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}
