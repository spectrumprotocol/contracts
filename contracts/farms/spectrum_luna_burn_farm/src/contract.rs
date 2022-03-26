#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{attr, to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, Order};

use crate::{
    bond::bond,
    state::{read_config, state_store, store_config, Config, State},
};

use crate::bond::{claim_unbond, query_reward_info, query_unbond, unbond, withdraw};
use crate::burn::{burn, collect, collect_fee, collect_hook, query_burns, send_fee, simulate_collect};
use crate::state::{Hub, hub_store, hubs_read, HubType, read_state};
use crate::model::{ConfigInfo, ExecuteMsg, QueryMsg};

/// (we require 0-1)
fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " must be 0 to 1"))
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ConfigInfo,
) -> StdResult<Response> {
    validate_percentage(msg.community_fee, "community_fee")?;
    validate_percentage(msg.platform_fee, "platform_fee")?;
    validate_percentage(msg.controller_fee, "controller_fee")?;
    validate_percentage(msg.deposit_fee, "deposit_fee")?;

    store_config(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(&msg.owner)?,
            spectrum_token: deps.api.addr_canonicalize(&msg.spectrum_token)?,
            spectrum_gov: deps.api.addr_canonicalize(&msg.spectrum_gov)?,
            platform: deps.api.addr_canonicalize(&msg.platform)?,
            controller: deps.api.addr_canonicalize(&msg.controller)?,
            community_fee: msg.community_fee,
            platform_fee: msg.platform_fee,
            controller_fee: msg.controller_fee,
            deposit_fee: msg.deposit_fee,
            anchor_market: deps.api.addr_canonicalize(&msg.anchor_market)?,
            aust_token: deps.api.addr_canonicalize(&msg.aust_token)?,
            max_unbond_count: msg.max_unbond_count,
            burn_period: msg.burn_period,
            ust_pair_contract: deps.api.addr_canonicalize(&msg.ust_pair_contract)?,
            oracle: deps.api.addr_canonicalize(&msg.oracle)?,
            credits: msg.credits,
        },
    )?;

    state_store(deps.storage).save(&State {
        previous_spec_share: Uint128::zero(),
        spec_share_index: Decimal::zero(),
        total_bond_amount: Uint128::zero(),
        total_bond_share: Uint128::zero(),
        unbonding_amount: Uint128::zero(),
        unbond_counter: 0u64,
        unbonded_index: Uint128::zero(),
        unbonding_index: Uint128::zero(),
        claimable_amount: Uint128::zero(),
        deposit_fee: Uint128::zero(),
        perf_fee: Uint128::zero(),
        deposit_earning: Uint128::zero(),
        perf_earning: Uint128::zero(),
        burn_counter: 0u64,
    })?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::bond {
            staker_addr,
        } => {
            if info.funds.len() != 1 || info.funds[0].denom != "uluna" {
                return Err(StdError::generic_err("fund mismatch"));
            }
            bond(
                deps,
                env,
                staker_addr.unwrap_or_else(|| info.sender.to_string()),
                info.funds[0].amount,
            )
        },
        ExecuteMsg::update_config {
            owner,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
            deposit_fee,
            max_unbond_count,
            burn_period,
        } => update_config(
            deps,
            info,
            owner,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
            deposit_fee,
            max_unbond_count,
            burn_period,
        ),
        ExecuteMsg::unbond {
            amount,
        } => unbond(deps, env, info, amount),
        ExecuteMsg::claim_unbond {} =>
            claim_unbond(deps, env, info),
        ExecuteMsg::withdraw { spec_amount } =>
            withdraw(deps, env, info, spec_amount),
        // ExecuteMsg::compound {} => compound(deps, env, info),
        // ExecuteMsg::send_fee {} => send_fee(deps, env, info),
        ExecuteMsg::register_hub { token, hub_address, hub_type } =>
            register_hub(deps, info, token, hub_address, hub_type),
        ExecuteMsg::burn { amount, swap_operations, min_profit } =>
            burn(deps, env, info, amount, swap_operations, min_profit),
        ExecuteMsg::collect {} =>
            collect(deps, env),
        ExecuteMsg::collect_hook { prev_balance, total_input_amount } =>
            collect_hook(deps, env, info, prev_balance, total_input_amount),
        ExecuteMsg::collect_fee {} =>
            collect_fee(deps, env, info),
        ExecuteMsg::send_fee { deposit_fee_ratio } =>
            send_fee(deps, env, info, deposit_fee_ratio),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    controller: Option<String>,
    community_fee: Option<Decimal>,
    platform_fee: Option<Decimal>,
    controller_fee: Option<Decimal>,
    deposit_fee: Option<Decimal>,
    max_unbond_count: Option<u32>,
    burn_period: Option<u64>,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(owner) = owner {
        if config.owner == config.spectrum_gov {
            return Err(StdError::generic_err("cannot update owner"));
        }
        config.owner = deps.api.addr_canonicalize(&owner)?;
    }

    if let Some(controller) = controller {
        config.controller = deps.api.addr_canonicalize(&controller)?;
    }

    if let Some(community_fee) = community_fee {
        validate_percentage(community_fee, "community_fee")?;
        config.community_fee = community_fee;
    }

    if let Some(platform_fee) = platform_fee {
        validate_percentage(platform_fee, "platform_fee")?;
        config.platform_fee = platform_fee;
    }

    if let Some(controller_fee) = controller_fee {
        validate_percentage(controller_fee, "controller_fee")?;
        config.controller_fee = controller_fee;
    }

    if let Some(deposit_fee) = deposit_fee {
        validate_percentage(deposit_fee, "deposit_fee")?;
        config.deposit_fee = deposit_fee;
    }

    if let Some(max_unbond_count) = max_unbond_count {
        config.max_unbond_count = max_unbond_count;
    }

    if let Some(burn_period) = burn_period {
        config.burn_period = burn_period;
    }

    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

pub fn register_hub(
    deps: DepsMut,
    info: MessageInfo,
    token: String,
    hub_address: String,
    hub_type: HubType,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    let token_raw = deps.api.addr_canonicalize(&token)?;
    let hub = Hub {
        token: deps.api.addr_validate(&token)?,
        hub_address: deps.api.addr_validate(&hub_address)?,
        hub_type,
    };
    hub_store(deps.storage).save(token_raw.as_slice(), &hub)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::reward_info { staker_addr, } =>
            to_binary(&query_reward_info(deps, env, staker_addr)?),
        QueryMsg::unbond { staker_addr } =>
            to_binary(&query_unbond(deps, staker_addr)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
        QueryMsg::hubs {} => to_binary(&query_hubs(deps)?),
        QueryMsg::burns {} => to_binary(&query_burns(deps)?),
        QueryMsg::simulate_collect {} => to_binary(&simulate_collect(deps, env)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = read_config(deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        spectrum_token: deps.api.addr_humanize(&config.spectrum_token)?.to_string(),
        spectrum_gov: deps.api.addr_humanize(&config.spectrum_gov)?.to_string(),
        platform: deps.api.addr_humanize(&config.platform)?.to_string(),
        controller: deps.api.addr_humanize(&config.controller)?.to_string(),
        community_fee: config.community_fee,
        platform_fee: config.platform_fee,
        controller_fee: config.controller_fee,
        deposit_fee: config.deposit_fee,
        anchor_market: deps.api.addr_humanize(&config.anchor_market)?.to_string(),
        aust_token: deps.api.addr_humanize(&config.aust_token)?.to_string(),
        max_unbond_count: config.max_unbond_count,
        burn_period: config.burn_period,
        ust_pair_contract: deps.api.addr_humanize(&config.ust_pair_contract)?.to_string(),
        oracle: deps.api.addr_humanize(&config.oracle)?.to_string(),
        credits: config.credits,
    };

    Ok(resp)
}

fn query_state(deps: Deps) -> StdResult<State> {
    read_state(deps.storage)
}

fn query_hubs(deps: Deps) -> StdResult<Vec<Hub>> {
    hubs_read(deps.storage).range(None, None, Order::Ascending)
        .map(|item| {
            let (_, v) = item?;
            Ok(v)
        })
        .collect()

}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: ConfigInfo) -> StdResult<Response> {
    store_config(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(&msg.owner)?,
            spectrum_token: deps.api.addr_canonicalize(&msg.spectrum_token)?,
            spectrum_gov: deps.api.addr_canonicalize(&msg.spectrum_gov)?,
            platform: deps.api.addr_canonicalize(&msg.platform)?,
            controller: deps.api.addr_canonicalize(&msg.controller)?,
            community_fee: msg.community_fee,
            platform_fee: msg.platform_fee,
            controller_fee: msg.controller_fee,
            deposit_fee: msg.deposit_fee,
            anchor_market: deps.api.addr_canonicalize(&msg.anchor_market)?,
            aust_token: deps.api.addr_canonicalize(&msg.aust_token)?,
            max_unbond_count: msg.max_unbond_count,
            burn_period: msg.burn_period,
            ust_pair_contract: deps.api.addr_canonicalize(&msg.ust_pair_contract)?,
            oracle: deps.api.addr_canonicalize(&msg.oracle)?,
            credits: msg.credits,
        },
    )?;

    Ok(Response::default())
}
