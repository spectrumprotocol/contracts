use crate::state::{account_store, poll_voter_store, read_account, read_config, read_poll, read_state, read_vault, read_vaults, state_store, vault_store, Account, Config, State, StatePool, SEC_IN_DAY};
use cosmwasm_std::{
    attr, to_binary, CanonicalAddr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use spectrum_protocol::gov::{BalanceResponse, PollStatus, VaultInfo, VaultsResponse, BalancePoolInfo};
use terraswap::querier::query_token_balance;

pub fn reconcile_balance(deps: &Deps, state: &mut State, config: &Config, deposited_amount: Uint128) -> StdResult<Uint128> {
    reconcile_aust(deps, state, config)?;
    reconcile_spec(deps, state, config, deposited_amount)
}

fn reconcile_aust(deps: &Deps, state: &mut State, config: &Config) -> StdResult<()> {

    let balance = query_token_balance(
        &deps.querier,
        deps.api.addr_humanize(&config.aust_token)?,
        deps.api.addr_humanize(&state.contract_addr)?,
    )?;

    let diff = balance.checked_sub(state.prev_aust_balance)?;
    if diff.is_zero() {
        return Ok(());
    }
    let mut changes = distribute_reward(state, diff)?;
    state.aust_index = state.aust_index + Decimal::from_ratio(changes.pop().unwrap(), state.total_share);
    for pool in state.pools.iter_mut() {
        if !pool.active {
            continue;
        }
        pool.aust_index = pool.aust_index + Decimal::from_ratio(changes.pop().unwrap(), pool.total_share);
    }
    state.prev_aust_balance = balance;

    Ok(())
}

#[allow(clippy::needless_range_loop)]
fn distribute_reward(state: &State, diff: Uint128) -> StdResult<Vec<Uint128>> {
    // if amount grow, distribute reward to each pool equally
    // however higher lock pool also earn from lower lock pool
    let mut pools: Vec<&StatePool> = state.pools.iter().filter(|it| it.active).rev().collect();
    let pool_0 = StatePool {
        days: 0u64,
        total_balance: state.total_balance,
        total_share: state.total_share,
        aust_index: state.aust_index,
        active: true,
    };
    pools.push(&pool_0);

    let len: u128 = (pools.len() as u64).into();
    let mut denom = 0u128;
    let mut changes = vec![Uint128::zero(); pools.len()];
    let mut total = Uint128::zero();
    for i in 0..pools.len() {
        let pool = pools.get(i).unwrap();
        denom += pool.total_balance.u128() * len;
        if denom != 0u128 {
            for j in 0..=i {
                let inner_pool = pools.get(j).unwrap();
                let change = diff.multiply_ratio(inner_pool.total_balance, denom);
                total += change;
                changes[j] += change;
            }
        }
    }

    changes[0] += diff.checked_sub(total)?;

    Ok(changes)
}

fn reconcile_spec(deps: &Deps, state: &mut State, config: &Config, deposited_amount: Uint128) -> StdResult<Uint128> {

    let balance = query_token_balance(
        &deps.querier,
        deps.api.addr_humanize(&config.spec_token)?,
        deps.api.addr_humanize(&state.contract_addr)?,
    )?
        .checked_sub(state.poll_deposit)?
        .checked_sub(state.vault_balances)?
        .checked_sub(deposited_amount)?;

    if balance >= state.prev_balance {
        let diff = balance.checked_sub(state.prev_balance)?;
        if diff.is_zero() {
            return Ok(balance);
        }

        let mut changes = distribute_reward(state, diff)?;
        state.total_balance += changes.pop().unwrap();
        for pool in state.pools.iter_mut() {
            if !pool.active {
                continue;
            }
            pool.total_balance += changes.pop().unwrap();
        }
    } else {
        // if amount is reduced, decrease balance proportionally

        let diff = state.prev_balance.checked_sub(balance)?;
        let total_balance = state.total_balance + state.pools.iter().fold(Uint128::zero(), |a, b| a + b.total_balance);
        let mut remain = diff;
        let deduct_0 = diff.multiply_ratio(state.total_balance, total_balance);
        state.total_balance -= deduct_0;
        remain -= deduct_0;
        for pool in state.pools.iter_mut() {
            let deduct = diff.multiply_ratio(pool.total_balance, total_balance);
            pool.total_balance -= deduct;
            remain -= deduct;
        }
        if !remain.is_zero() {
            let deduct_0 = if remain > state.total_balance { state.total_balance } else { remain };
            state.total_balance -= deduct_0;
            remain -= deduct_0;
            for pool in state.pools.iter_mut() {
                if remain.is_zero() {
                    break;
                }
                let deduct = if remain > pool.total_balance { pool.total_balance } else { remain };
                pool.total_balance -= deduct;
                remain -= deduct;
            }
        }
    }

    state.prev_balance = balance;

    Ok(balance)
}

fn reconcile_account(account: &mut Account, state: &State) {
    account.pending_aust += (state.aust_index - account.aust_index) * account.share;
    account.aust_index = state.aust_index;

    for state_pool in state.pools.iter() {
        let account_pool = account.pools.iter_mut().find(|it| it.days == state_pool.days);
        if let Some(account_pool) = account_pool {
            account_pool.pending_aust += (state_pool.aust_index - account_pool.aust_index) * account_pool.share;
            account_pool.aust_index = state_pool.aust_index;
        }
    }
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
    let mut mintable = calc_mintable(&state, &config, env.block.height);

    if mintable.is_zero() {
        if state.last_mint < config.mint_end {
            state.last_mint = env.block.height;
            state_store(deps.storage).save(&state)?;
        }
        return Ok(Response::default());
    }

    reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero())?;

    // distribute to burnvault
    let mut total_mint = Uint128::zero();
    if !config.burnvault_ratio.is_zero() {
        let to_burnvault = mintable * config.burnvault_ratio;
        mintable = mintable.checked_sub(to_burnvault)?;
        total_mint += to_burnvault;
        let share = state.calc_share(0u64, to_burnvault)?;
        let key = config.burnvault_address.as_slice();
        let mut account = account_store(deps.storage)
            .may_load(key)?
            .unwrap_or_else(|| Account::create(&state));
        account.share += share;
        state.add_share(0u64, share, to_burnvault)?;
        account_store(deps.storage).save(key, &account)?;
    }

    // distribute to warchest
    if !config.warchest_ratio.is_zero() {
        let to_warchest = mintable * config.warchest_ratio;
        mintable = mintable.checked_sub(to_warchest)?;
        total_mint += to_warchest;
        let share = state.calc_share(0u64, to_warchest)?;
        let key = config.warchest_address.as_slice();
        let mut account = account_store(deps.storage)
            .may_load(key)?
            .unwrap_or_else(|| Account::create(&state));
        account.share += share;
        state.add_share(0u64, share, to_warchest)?;
        account_store(deps.storage).save(key, &account)?;
    }

    // mint to vault
    let vaults = read_vaults(deps.storage)?;
    for (key, mut vault) in vaults.into_iter() {
        let to_vault = mintable.multiply_ratio(vault.weight, state.total_weight as u128);
        total_mint += to_vault;
        vault.balance += to_vault;
        state.vault_balances += to_vault;
        vault_store(deps.storage).save(key.as_slice(), &vault)?;
    }

    state.last_mint = env.block.height;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.spec_token)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                amount: total_mint,
                recipient: deps.api.addr_humanize(&state.contract_addr)?.to_string(),
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![attr("action", "mint"), attr("amount", total_mint)]))
}

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

    let vault = read_vault(deps.storage, key)?;
    if vault.is_some() {
        return Err(StdError::generic_err("Not allow vault to stake directly"));
    }

    let config = read_config(deps.storage)?;
    let mut state = state_store(deps.storage).load()?;

    reconcile_balance(&deps.as_ref(), &mut state, &config, amount)?;

    let mut account = account_store(deps.storage)
        .may_load(key)?
        .unwrap_or_else(|| Account::create(&state));
    reconcile_account(&mut account, &state);

    let share = state.calc_share(days, amount)?;
    account.add_share(days, env.block.time.seconds(), share, 0u64, &state)?;
    state.add_share(days, share, amount)?;

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

pub fn update_stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    from_days: u64,
    to_days: u64
) -> StdResult<Response> {
    if from_days >= to_days {
        return Err(StdError::generic_err("must move to higher lock"));
    }

    let sender_address_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let key = sender_address_raw.as_slice();

    let mut account = account_store(deps.storage).load(key)?;
    let config = read_config(deps.storage)?;
    let mut state = state_store(deps.storage).load()?;

    reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero())?;
    reconcile_account(&mut account, &state);

    let mut from_share = state.calc_share(from_days, amount)?;
    if state.calc_balance(from_days, from_share)? < amount {
        from_share += Uint128::from(1u128);
    }

    let unlock = account.deduct_share(from_days, from_share, None)?;
    state.deduct_share(from_days, from_share, amount)?;

    let to_share = state.calc_share(to_days, amount)?;
    let time = env.block.time.seconds();
    let time_remain = if unlock < time { 0u64 } else { unlock - time };
    account.add_share(to_days, time, to_share, from_days * SEC_IN_DAY - time_remain, &state)?;
    state.add_share(to_days, to_share, amount)?;

    account_store(deps.storage).save(key, &account)?;
    state_store(deps.storage).save(&state)?;

    Ok(Response::new()
        .add_attributes(vec![
            attr("action", "update_stake"),
            attr("amount", amount),
            attr("from_days", from_days.to_string()),
            attr("to_days", to_days.to_string()),
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

    let mut state = state_store(deps.storage).load()?;
    let config = read_config(deps.storage)?;
    let vault = read_vault(deps.storage, key)?;
    let amount = if let Some(mut vault) = vault {
        let amount = amount.unwrap_or(vault.balance);
        if amount > vault.balance {
            return Err(StdError::generic_err(
                "User is trying to withdraw too many tokens.",
            ));
        }
        vault.balance = vault.balance.checked_sub(amount)?;
        state.vault_balances = state.vault_balances.checked_sub(amount)?;

        vault_store(deps.storage).save(key, &vault)?;
        state_store(deps.storage).save(&state)?;

        amount
    } else {
        reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero())?;

        let mut account = account_store(deps.storage).load(key)?;
        reconcile_account(&mut account, &state);

        let locked_balance = compute_locked_balance(deps.branch(), &mut account, &sender_address_raw)?;
        let user_balance = account.calc_total_balance(&state)?;
        let amount = amount.unwrap_or_else(|| {
            let max_withdraw = user_balance.checked_sub(locked_balance).unwrap();
            let pool_balance = account.calc_balance(days, &state).unwrap();
            if max_withdraw < pool_balance { max_withdraw } else { pool_balance }
        });
        if locked_balance + amount > user_balance {
            return Err(StdError::generic_err(
                "User is trying to withdraw too many tokens.",
            ));
        }

        let mut withdraw_share = state.calc_share(days, amount)?;
        if state.calc_balance(days, withdraw_share)? < amount {
            withdraw_share += Uint128::from(1u128);
        }

        account.deduct_share(days, withdraw_share, Some(env.block.time.seconds()))?;
        state.deduct_share(days, withdraw_share, amount)?;

        account_store(deps.storage).save(key, &account)?;
        state_store(deps.storage).save(&state)?;

        amount
    };

    send_tokens(
        deps,
        &config.spec_token,
        &sender_address_raw,
        amount,
        "withdraw",
    )
}

pub fn harvest(
    deps: DepsMut,
    info: MessageInfo,
    aust_amount: Option<Uint128>,
    days: u64,
) -> StdResult<Response> {
    let sender_address_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let key = sender_address_raw.as_slice();

    let mut state = state_store(deps.storage).load()?;
    let config = read_config(deps.storage)?;
    reconcile_balance(&deps.as_ref(), &mut state, &config, Uint128::zero())?;

    let mut account = account_store(deps.storage).load(key)?;
    reconcile_account(&mut account, &state);

    let aust_balance = account.get_aust(days);
    let amount = aust_amount.unwrap_or(aust_balance);
    if amount > aust_balance {
        return Err(StdError::generic_err(
            "User is trying to withdraw too many tokens.",
        ));
    }

    account.deduct_aust(days, amount)?;
    state.prev_aust_balance = state.prev_aust_balance.checked_sub(amount)?;

    account_store(deps.storage).save(key, &account)?;
    state_store(deps.storage).save(&state)?;

    send_tokens(
        deps,
        &config.aust_token,
        &sender_address_raw,
        amount,
        "harvest",
    )
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

pub fn upsert_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    days: u64,
    active: bool,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    if config.owner != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }
    let mut state = state_store(deps.storage).load()?;
    validate_minted(&state, &config, env.block.height)?;

    let pool = state.pools.iter_mut().find(|it| it.days == days);
    if let Some(pool) = pool {
        pool.active = active;
    } else if active {
        state.pools.push(StatePool {
            days,
            active,
            total_balance: Uint128::zero(),
            total_share: Uint128::zero(),
            aust_index: Decimal::zero(),
        });
        state.pools.sort_by(|a, b| a.days.partial_cmp(&b.days).unwrap());
    }

    state_store(deps.storage).save(&state)?;

    Ok(Response::new().add_attributes(vec![
        attr("days", days.to_string()),
        attr("active", active.to_string()),
    ]))
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
    vault_store(deps.storage).save(key, &vault)?;

    Ok(Response::new().add_attributes(vec![attr(
        "new_total_weight",
        state.total_weight.to_string(),
    )]))
}

pub fn query_balances(deps: Deps, address: String, height: u64) -> StdResult<BalanceResponse> {
    let addr_raw = deps.api.addr_canonicalize(&address).unwrap();

    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;
    if let Some(vault) = read_vault(deps.storage, addr_raw.as_slice())? {
        let mintable = calc_mintable(&state, &config, height);
        let to_vault = mintable
            * (Decimal::one() - config.burnvault_ratio)
            * (Decimal::one() - config.warchest_ratio);

        let add_balance = to_vault.multiply_ratio(vault.weight, state.total_weight);

        Ok(BalanceResponse {
            balance: vault.balance + add_balance,
            share: Uint128::zero(),
            locked_balance: vec![],
            pools: vec![],
        })
    } else {
        reconcile_balance(&deps, &mut state, &config, Uint128::zero())?;

        let mut account = read_account(deps.storage, addr_raw.as_slice())?
            .unwrap_or_else(|| Account::create(&state));
        reconcile_account(&mut account, &state);

        // filter out not in-progress polls
        account.locked_balance.retain(|(poll_id, _)| {
            let poll = read_poll(deps.storage, &poll_id.to_be_bytes())
                .unwrap()
                .unwrap();

            poll.status == PollStatus::in_progress
        });

        if addr_raw == config.burnvault_address {
            let mintable = calc_mintable(&state, &config, height);
            let to_burnvault = mintable * config.burnvault_ratio;
            let share = state.calc_share(0u64, to_burnvault)?;
            account.share += share;

            // add_share is optional, this will result to the same amount
            // state.add_share(0u64, share, to_burnvault)?;
        } else if addr_raw == config.warchest_address {
            let mintable = calc_mintable(&state, &config, height);
            let to_warchest = mintable
                * (Decimal::one() - config.burnvault_ratio)
                * config.warchest_ratio;
            let share = state.calc_share(0u64, to_warchest)?;
            account.share += share;

            // add_share is optional, this will result to the same amount
            // state.add_share(0u64, share, to_warchest)?;
        }

        Ok(BalanceResponse {
            balance: account.calc_total_balance(&state)?,
            share: account.share,
            locked_balance: account.locked_balance,
            pools: vec![
                vec![
                    BalancePoolInfo {
                        days: 0u64,
                        share: account.share,
                        balance: state.calc_balance(0u64, account.share)?,
                        unlock: 0u64,
                    },
                ],
                account.pools.into_iter().map(|it| BalancePoolInfo {
                    days: it.days,
                    share: it.share,
                    unlock: it.unlock,
                    balance: state.calc_balance(it.days, it.share).unwrap(),
                }).collect()
            ].concat()
        })
    }
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
                balance: it.1.balance,
            })
            .collect(),
    })
}
