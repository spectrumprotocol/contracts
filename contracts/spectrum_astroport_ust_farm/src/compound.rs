use cosmwasm_std::{attr, to_binary, Attribute, CanonicalAddr, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg, QueryRequest, WasmQuery, Deps};

use crate::{
    bond::deposit_farm_share,
    state::{read_config, state_store}, querier::{query_astroport_pending_token, query_astroport_pool_balance},
};

use cw20::Cw20ExecuteMsg;

use crate::state::{pool_info_read, pool_info_store, read_state, Config, PoolInfo};
use spectrum_protocol::{gov_proxy::{
    StakerInfoGovResponse, ExecuteMsg as GovProxyExecuteMsg, Cw20HookMsg as GovProxyCw20HookMsg
}, farm_helper::compute_provide_after_swap_astroport};
use astroport::generator::{
    ExecuteMsg as AstroportExecuteMsg, PendingTokenResponse, Cw20HookMsg as AstroportCw20HookMsg
};
use spectrum_protocol::anchor_farm::ExecuteMsg;
use astroport::asset::{Asset, AssetInfo};
use astroport::pair::{Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg, QueryMsg as AstroportPairQueryMsg, PoolResponse};
use astroport::querier::{query_token_balance, simulate};
use spectrum_protocol::farm_helper::{compute_provide_after_swap, deduct_tax};
use moneymarket::market::{ExecuteMsg as MoneyMarketExecuteMsg};

pub fn compound(deps: DepsMut, env: Env, info: MessageInfo, threshold_compound_astro: Uint128) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    if config.controller != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let pair_contract = deps.api.addr_humanize(&config.pair_contract)?;
    let astroport_generator = deps.api.addr_humanize(&config.astroport_generator)?;
    let farm_token = deps.api.addr_humanize(&config.farm_token)?;
    let gov_proxy = if config.gov_proxy.is_some(){
        Some(deps.api.addr_humanize(&config.gov_proxy.unwrap())?)
    } else {
        None
    };

    let mut pool_info = pool_info_read(deps.storage).load(config.farm_token.as_slice())?;

    // This get pending (ASTRO), and pending proxy rewards
    // let reward_info = query_astroport_pending_token(
    //     deps.as_ref(),
    //     &pool_info.staking_token,
    //     &deps.api.addr_canonicalize(env.contract.address.as_str())?,
    //     &config.astroport_generator
    // )?;

    // TODO query env.contract farm token and ASTRO
    // TODO set reinvest allowance for both farm token and ASTRO (need new reinvest_allowance_astro?)
    // TODO query_astroport_pending_token and do manual claim
    // TODO logic to compound and stake ASTRO
    let farm_token_info = AssetInfo::cw20(&config.farm_token); //TODO how to cast to cw20?
    let farm_token_amount = farm_token_info.query_balance(&deps.querier, &env.contract.address)?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &deps.api.addr_canonicalize(env.contract.address.as_str())?,
        &config.astroport_generator
    )?;

    let mut total_farm_token_swap_amount = Uint128::zero();
    let mut total_farm_token_stake_amount = Uint128::zero();
    let mut total_farm_token_commission = Uint128::zero();
    let mut compound_amount = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];

    // calculate auto-compound, auto-Stake, and commission in farm token
    let reward = farm_token_amount;
    if !reward.is_zero() && !lp_balance.is_zero() {
        let total_fee = config.community_fee + config.platform_fee + config.controller_fee;
        let commission = reward * total_fee;
        let anchor_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_farm_token_commission += commission;
        total_farm_token_swap_amount += commission;

        let auto_bond_amount = lp_balance.checked_sub(pool_info.total_stake_bond_amount)?;
        compound_amount =
            anchor_amount.multiply_ratio(auto_bond_amount, lp_balance);
        let stake_amount = anchor_amount.checked_sub(compound_amount)?;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("stake_amount", stake_amount));

        total_farm_token_stake_amount += stake_amount;
    }
    let mut state = read_state(deps.storage)?;
    deposit_farm_share(
        deps.as_ref(),
        &mut state,
        &mut pool_info,
        &config,
        total_farm_token_stake_amount,
    )?;
    state_store(deps.storage).save(&state)?;
    pool_info_store(deps.storage).save(config.farm_token.as_slice(), &pool_info)?;

    // get reinvest amount

    // TODO call manual claim reward and add to reinvest_allowance
    // TODO ASTRO reinvest allowance 

    let reinvest_allowance = query_token_balance(&deps.querier, farm_token.clone(), env.contract.address.clone())?;
    let reinvest_amount = reinvest_allowance + compound_amount;
    // split reinvest amount
    let swap_amount = reinvest_amount.multiply_ratio(1u128, 2u128);
    // add commission to reinvest farm token to total swap amount
    total_farm_token_swap_amount += swap_amount;

    // find farm token swap rate
    let farm_token_asset = Asset {
        info: AssetInfo::Token {
            contract_addr: farm_token, //TODO will Astroport interface change again?
        },
        amount: total_farm_token_swap_amount,
    };
    let farm_token_swap_rate = simulate(
        &deps.querier,
        pair_contract.clone(),
        &farm_token_asset,
    )?;
    let total_ust_return_amount = deduct_tax(&deps.querier, farm_token_swap_rate.return_amount, config.base_denom.clone())?;
    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let total_ust_commission_amount = if total_farm_token_swap_amount != Uint128::zero() {
        total_ust_return_amount.multiply_ratio(total_farm_token_commission, total_farm_token_swap_amount)
    } else {
        Uint128::zero()
    };
    let total_ust_reinvest_amount =
        total_ust_return_amount.checked_sub(total_ust_commission_amount)?;

    // deduct tax for provided UST
    let net_reinvest_ust = deduct_tax(
        &deps.querier,
        total_ust_reinvest_amount,
        config.base_denom.clone(),
    )?;
    let pool: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&AstroportPairQueryMsg::Pool {})?,
    }))?;

    let provide_farm_token = compute_provide_after_swap_astroport(
        &pool,
        &farm_token_asset,
        farm_token_swap_rate.return_amount,
        net_reinvest_ust
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];
    // let withdraw_all_farm_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
    //     contract_addr: astroport_generator.to_string(),
    //     funds: vec![],
    //     msg: to_binary(&AnchorStakingExecuteMsg::Withdraw {})?,
    // });
    // messages.push(withdraw_all_farm_token);

    if !total_farm_token_swap_amount.is_zero() {
        let swap_farm_token: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: farm_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract.to_string(),
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

        // find SPEC swap rate
        let net_commission_amount = deduct_tax(&deps.querier, total_ust_commission_amount, config.base_denom.clone())?;

        let mut state = read_state(deps.storage)?;
        state.earning += net_commission_amount;
        state_store(deps.storage).save(&state)?;

        attributes.push(attr("net_commission", net_commission_amount));

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
            msg: to_binary(&MoneyMarketExecuteMsg::DepositStable {})?,
            funds: vec![Coin {
                denom: config.base_denom.clone(),
                amount: net_commission_amount,
            }],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::send_fee {})?,
            funds: vec![],
        }));
    }

    if !total_farm_token_stake_amount.is_zero() && gov_proxy.is_some() {
        let stake_farm_token = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: farm_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: gov_proxy.unwrap().to_string(),
                amount: total_farm_token_stake_amount,
                msg: to_binary(&GovProxyCw20HookMsg::Stake {})?,
            })?,
        });
        messages.push(stake_farm_token);
    }

    if !provide_farm_token.is_zero() {
        let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: farm_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_contract.to_string(),
                amount: provide_farm_token,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: farm_token,
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
                slippage_tolerance: None,
                receiver: None,
                auto_stake: Some(false),
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
                asset_token: farm_token.to_string(),
            })?,
            funds: vec![],
        });
        messages.push(stake);
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", farm_token));
    attributes.push(attr("reinvest_amount", reinvest_amount));
    attributes.push(attr("provide_token_amount", provide_farm_token));
    attributes.push(attr("provide_ust_amount", net_reinvest_ust));

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset_token: String,
) -> StdResult<Response> {
    // only anchor farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let astroport_generator = deps.api.addr_humanize(&config.astroport_generator)?;
    let asset_token_raw: CanonicalAddr = deps.api.addr_canonicalize(&asset_token)?;
    let pool_info: PoolInfo = pool_info_read(deps.storage).load(asset_token_raw.as_slice())?;
    let staking_token = deps.api.addr_humanize(&pool_info.staking_token)?;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_generator.to_string(),
                amount,
                msg: to_binary(&AstroportCw20HookMsg::Deposit {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("asset_token", asset_token),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}

pub fn send_fee(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> StdResult<Response> {

    // only farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config: Config = read_config(deps.storage)?;
    let aust_token = deps.api.addr_humanize(&config.aust_token)?;
    let spectrum_gov = deps.api.addr_humanize(&config.spectrum_gov)?;

    let aust_balance = query_token_balance(&deps.querier, aust_token.clone(), env.contract.address)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let thousand = Uint128::from(1000u64);
    let total_fee = config.community_fee + config.controller_fee + config.platform_fee;
    let community_amount = aust_balance.multiply_ratio(thousand * config.community_fee, thousand * total_fee);
    if !community_amount.is_zero() {
        let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: spectrum_gov.to_string(),
                amount: community_amount,
            })?,
            funds: vec![],
        });
        messages.push(transfer_community_fee);
    }

    let platform_amount = aust_balance.multiply_ratio(thousand * config.platform_fee, thousand * total_fee);
    if !platform_amount.is_zero() {
        let stake_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&config.platform)?.to_string(),
                amount: platform_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_platform_fee);
    }

    let controller_amount = aust_balance.checked_sub(community_amount + platform_amount)?;
    if !controller_amount.is_zero() {
        let stake_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: deps.api.addr_humanize(&config.controller)?.to_string(),
                amount: controller_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_controller_fee);
    }
    Ok(Response::new()
        .add_messages(messages))
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