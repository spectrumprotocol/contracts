use crate::state::{account_store, poll_voter_store, read_account, read_config, read_poll, read_state, read_vault, read_vaults, state_store, vault_store, Account, Config, State, StatePool};
use cosmwasm_std::{
    attr, to_binary, CanonicalAddr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::gov::{BalanceResponse, PollStatus, VaultInfo, VaultsResponse, BalancePoolInfo};
use terraswap::querier::query_token_balance;
use std::convert::TryInto;

pub fn reconcile_balance(state: &mut State, balance: Uint128) -> StdResult<()> {
    let diff = if balance >= state.prev_balance {
        balance.checked_sub(state.prev_balance)?
    } else {
        state.prev_balance.checked_sub(balance)?
    };
    if diff.is_zero() {
        return Ok(());
    }

    let pools: Vec<StatePool> = vec![
        state.pools.into_iter().filter(|it| it.active).rev().collect(),
        vec![ state.get_pool(0u64) ],
    ].concat();

    let len: u128 = (pools.len() as u64).into();
    let count = 1u128 + len;
    let mut denom = 0u128;
    let mut changes = vec![Uint128::zero(); pools.len()];
    let mut total = Uint128::zero();
    for i in 0..pools.len() {
        let pool = pools.get(i).unwrap();
        denom += pool.total_balance.u128() * count;
        if denom == 0u128 {
            for j in 0..i {
                let inner_pool = pools.get(j).unwrap();
                let change = diff.multiply_ratio(inner_pool.total_balance, denom);
                total += change;
                changes[j] += change;
            }
        }
    }

    changes[0] += diff.checked_sub(total)?;
    if balance >= state.prev_balance {
        state.total_balance += changes.pop().unwrap();
        for pool in state.pools.iter_mut() {
            if !pool.active {
                continue;
            }
            pool.total_balance += changes.pop().unwrap();
        }
    } else {
        state.total_balance -= changes.pop().unwrap();
        for pool in state.pools.iter_mut() {
            if !pool.active {
                continue;
            }
            pool.total_balance -= changes.pop().unwrap();
        }
    }

    state.prev_balance = balance;

    Ok(())
}

/// mint should be done before
/// - deposit_reward
/// - poll_end
/// - update_config
/// - upsert_vault
/// - withdraw (for warchest & vault)
pub fn mint(deps: DepsMut, env: Env) -> StdResult<Response> {
    let mut state = state_store(deps.storage).load()?;
    let config = read_config(deps.storage)?;
    let mintable = calc_mintable(&state, &config, env.block.height);

    if mintable.is_zero() {
        if state.last_mint < config.mint_end {
            state.last_mint = env.block.height;
            state_store(deps.storage).save(&state)?;
        }
        return Ok(Response::default());
    }

    // mint to warchest
    let total_balance = query_token_balance(
        &deps.querier,
        deps.api.addr_humanize(&config.spec_token)?,
        deps.api.addr_humanize(&state.contract_addr)?,
    )?
    .checked_sub(state.poll_deposit)?;
    reconcile_balance(&mut state, total_balance)?;

    let mut mint_share = Uint128::zero();
    let mut to_warchest = Uint128::zero();
    let state_pool_0 = state.get_pool(0u64);
    if config.warchest_address != CanonicalAddr::from(vec![])
        && config.warchest_ratio != Decimal::zero()
    {
        to_warchest = mintable * config.warchest_ratio;
        let share = state_pool_0.calc_share(to_warchest);
        let key = config.warchest_address.as_slice();
        let mut account = account_store(deps.storage)
            .may_load(key)?
            .unwrap_or_default();
        account.share += share;
        mint_share += share;
        account_store(deps.storage).save(key, &account)?;
    }

    // mint to vault
    let vaults = read_vaults(deps.storage)?;
    let share = state_pool_0.calc_share(mintable.checked_sub(to_warchest)?);
    for (addr, vault) in vaults.into_iter() {
        let key = addr.as_slice();
        let mut account = account_store(deps.storage)
            .may_load(key)?
            .unwrap_or_default();
        let vault_share = share.multiply_ratio(vault.weight, state.total_weight as u128);
        account.share += vault_share;
        mint_share += vault_share;
        account_store(deps.storage).save(key, &account)?;
    }
    state.total_share += mint_share;
    state.total_balance += mintable;
    state.last_mint = env.block.height;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spec_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                amount: mintable,
                recipient: deps.api.addr_humanize(&state.contract_addr)?.to_string(),
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![attr("action", "mint"), attr("amount", mintable)]))
}

const SEC_IN_DAY: u64 = 24u64 * 60u64 * 60u64;

pub fn stake_tokens(
    deps: DepsMut,
    env: Env,
    sender: String,
    amount: Uint128,
    days: u64,
) -> StdResult<Response> {
    if amount.is_zero() {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    let sender_address_raw = deps.api.addr_canonicalize(&sender)?;
    let key = sender_address_raw.as_slice();

    let mut account = account_store(deps.storage)
        .may_load(key)?
        .unwrap_or_default();
    let config = read_config(deps.storage)?;
    let mut state = state_store(deps.storage).load()?;

    // balance already increased, so subtract deposit amount
    let current_balance = query_token_balance(
        &deps.querier,
        deps.api.addr_humanize(&config.spec_token)?,
        deps.api.addr_humanize(&state.contract_addr)?,
    )?;
    let total_balance = current_balance.checked_sub(state.poll_deposit + amount)?;
    reconcile_balance(&mut state, total_balance)?;

    let mut state_pool = state.get_pool(days);
    let share = state_pool.calc_share(amount);
    if days == 0u64 {
        account.share += share;
        state.total_share += share;
        state.total_balance += amount;
    } else {
        let mut acc_pool = account.get_pool(days);
        let time = env.block.time.seconds();
        let new_share = acc_pool.share + share;
        let remaining = if acc_pool.unlock < time {
            Uint128::zero()
        } else {
            Uint128::from(acc_pool.unlock - time).multiply_ratio(acc_pool.share, new_share)
        };
        let additional = Uint128::from(SEC_IN_DAY * acc_pool.days).multiply_ratio(share, new_share);
        let add_time: u64 = (remaining + additional).u128().try_into().unwrap();
        acc_pool.unlock = time + add_time;
        acc_pool.share = new_share;

        state_pool.total_share += share;
        state_pool.total_balance += amount;
    }

    state_store(deps.storage).save(&state)?;
    account_store(deps.storage).save(key, &account)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "staking"),
        attr("sender", sender),
        attr("share", share),
        attr("amount", amount),
        attr("days", days.to_string()),
    ]))
}

// Withdraw amount if not staked. By default all funds will be withdrawn.
pub fn withdraw(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
    days: u64,
) -> StdResult<Response> {
    let sender_address_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let key = sender_address_raw.as_slice();

    if let Some(mut account) = account_store(deps.storage).may_load(key)? {
        let config = read_config(deps.storage)?;
        let mut state = state_store(deps.storage).load()?;

        // Load total share & total balance except proposal deposit amount
        let total_balance = query_token_balance(
            &deps.querier,
            deps.api.addr_humanize(&config.spec_token)?,
            deps.api.addr_humanize(&state.contract_addr)?,
        )?
        .checked_sub(state.poll_deposit)?;
        reconcile_balance(&mut state, total_balance)?;

        let locked_balance = compute_locked_balance(deps.branch(), &mut account, &sender_address_raw)?;
        let user_balance = account.calc_total_balance(&state);
        let amount = amount.unwrap_or_else(|| user_balance.checked_sub(locked_balance).unwrap());
        if locked_balance + amount > user_balance {
            return Err(StdError::generic_err(
                "User is trying to withdraw too many tokens.",
            ));
        }

        let mut acc_pool = account.get_pool(days);
        if acc_pool.unlock > env.block.time.seconds() {
            return Err(StdError::generic_err("Pool is locked"));
        }

        let mut state_pool = state.get_pool(days);
        let mut withdraw_share = state_pool.calc_share(amount);
        if state_pool.calc_balance(withdraw_share) < amount {
            withdraw_share += Uint128::from(1u128);
        }

        if days == 0u64 {
            account.share -= withdraw_share;
            state.total_share -= withdraw_share;
            state.total_balance -= amount;
        } else {
            acc_pool.share -= withdraw_share;
            state_pool.total_share -= withdraw_share;
            state_pool.total_balance -= amount;
        }

        account_store(deps.storage).save(key, &account)?;
        state_store(deps.storage).save(&state)?;

        send_tokens(
            deps,
            &config.spec_token,
            &sender_address_raw,
            amount,
            "withdraw",
        )
    } else {
        Err(StdError::generic_err("Nothing staked"))
    }
}

fn send_tokens(
    deps: DepsMut,
    asset_token: &CanonicalAddr,
    recipient: &CanonicalAddr,
    amount: Uint128,
    action: &str,
) -> StdResult<Response> {
    let contract_human = deps.api.addr_humanize(asset_token)?.to_string();
    let recipient_human = deps.api.addr_humanize(recipient)?.to_string();

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_human,
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient_human.clone(),
                amount,
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            attr("action", action),
            attr("recipient", recipient_human),
            attr("amount", amount),
        ]))
}

// removes not in-progress poll voter info & unlock tokens
// and returns the largest locked amount in participated polls.
fn compute_locked_balance(
    deps: DepsMut,
    account: &mut Account,
    voter: &CanonicalAddr,
) -> StdResult<Uint128> {
    // filter out not in-progress polls
    account.locked_balance.retain(|(poll_id, _)| {
        let poll = read_poll(deps.storage, &poll_id.to_be_bytes())
            .unwrap()
            .unwrap();

        if poll.status != PollStatus::in_progress {
            // remove voter info from the poll
            poll_voter_store(deps.storage, *poll_id).remove(voter.as_slice());
        }

        poll.status == PollStatus::in_progress
    });

    Ok(account
        .locked_balance
        .iter()
        .map(|(_, v)| v.balance)
        .max()
        .unwrap_or_default())
}

pub fn upsert_vault(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vault_address: String,
    weight: u32,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    let mut state = state_store(deps.storage).load()?;
    validate_minted(&state, &config, env.block.height)?;

    let vault_address_raw = deps.api.addr_canonicalize(&vault_address)?;
    let key = vault_address_raw.as_slice();
    let mut vault = vault_store(deps.storage).may_load(key)?.unwrap_or_default();

    state.total_weight = state.total_weight + weight - vault.weight;
    vault.weight = weight;

    state_store(deps.storage).save(&state)?;

    if weight == 0 {
        vault_store(deps.storage).remove(key);
    } else {
        vault_store(deps.storage).save(key, &vault)?;
    }

    Ok(Response::new().add_attributes(vec![attr(
        "new_total_weight",
        state.total_weight.to_string(),
    )]))
}

pub fn query_balances(deps: Deps, address: String, height: u64) -> StdResult<BalanceResponse> {
    let addr_raw = deps.api.addr_canonicalize(&address).unwrap();
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;
    let mut account = read_account(deps.storage, addr_raw.as_slice())?.unwrap_or_default();

    // filter out not in-progress polls
    account.locked_balance.retain(|(poll_id, _)| {
        let poll = read_poll(deps.storage, &poll_id.to_be_bytes())
            .unwrap()
            .unwrap();

        poll.status == PollStatus::in_progress
    });

    let total_balance = query_token_balance(
        &deps.querier,
        deps.api.addr_humanize(&config.spec_token)?,
        deps.api.addr_humanize(&state.contract_addr)?,
    )?
    .checked_sub(state.poll_deposit)?;
    reconcile_balance(&mut state, total_balance)?;

    let state_pool_0 = state.get_pool(0u64);
    let mut balance = state_pool_0.calc_balance(account.share);
    let mut share = account.share;

    if addr_raw == config.warchest_address {
        let mintable = calc_mintable(&state, &config, height);
        let to_warchest = mintable * config.warchest_ratio;
        share += state_pool_0.calc_share(to_warchest);
        balance += to_warchest;
    } else if let Some(vault) = read_vault(deps.storage, addr_raw.as_slice())? {
        let mintable = calc_mintable(&state, &config, height);
        let to_warchest = mintable * config.warchest_ratio;
        let mintable = mintable.checked_sub(to_warchest)?;
        let vaults_share = state_pool_0.calc_share(mintable);
        share += vaults_share.multiply_ratio(vault.weight, state.total_weight);
        balance += mintable.multiply_ratio(vault.weight, state.total_weight);
    }

    Ok(BalanceResponse {
        locked_balance: account.locked_balance,
        pools: vec![
            vec![
                BalancePoolInfo {
                    days: 0u64,
                    share,
                    balance,
                    unlock: 0u64,
                },
            ],
            account.pools.into_iter().map(|it| BalancePoolInfo {
                days: it.days,
                share: it.share,
                unlock: it.unlock,
                balance: state.get_pool(it.days).calc_balance(it.share),
            }).collect()
        ].concat()
    })
}

pub fn calc_mintable(state: &State, config: &Config, height: u64) -> Uint128 {
    let last_mint = if config.mint_start > state.last_mint {
        config.mint_start
    } else {
        state.last_mint
    };
    let height = if height < config.mint_end {
        height
    } else {
        config.mint_end
    };
    if last_mint < height {
        let diff: u128 = (height - last_mint).into();
        let val = config.mint_per_block.u128() * diff;
        Uint128::from(val)
    } else {
        Uint128::zero()
    }
}

pub fn validate_minted(state: &State, config: &Config, height: u64) -> StdResult<()> {
    if state.last_mint < config.mint_end && state.last_mint != height {
        Err(StdError::generic_err(
            "required mint before using this function",
        ))
    } else {
        Ok(())
    }
}

pub fn query_vaults(deps: Deps) -> StdResult<VaultsResponse> {
    let vaults = read_vaults(deps.storage)?;
    Ok(VaultsResponse {
        vaults: vaults
            .into_iter()
            .map(|it| VaultInfo {
                address: deps.api.addr_humanize(&it.0).unwrap().to_string(),
                weight: it.1.weight,
            })
            .collect(),
    })
}
