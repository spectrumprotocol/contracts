use crate::state::{
    account_store, poll_voter_store, read_account, read_config, read_poll, read_state, read_vault,
    read_vaults, state_store, vault_store, Account, Config, State,
};
use cosmwasm_std::{
    log, to_binary, Api, CanonicalAddr, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20HandleMsg;
use spectrum_protocol::gov::{BalanceResponse, PollStatus, VaultInfo, VaultsResponse};
use spectrum_protocol::querier::{load_token_balance, send_tokens};

/// mint should be done before
/// - deposit_reward
/// - poll_end
/// - update_config
/// - upsert_vault
/// - withdraw (for warchest & vault)
pub fn mint<S: Storage, A: Api, Q: Querier>(deps: &mut Extern<S, A, Q>, env: Env) -> HandleResult {
    let mut state = state_store(&mut deps.storage).load()?;
    let config = read_config(&deps.storage)?;
    let mintable = calc_mintable(&state, &config, env.block.height);

    if mintable.is_zero() {
        if state.last_mint < config.mint_end {
            state.last_mint = env.block.height;
            state_store(&mut deps.storage).save(&state)?;
        }
        return Ok(HandleResponse::default());
    }

    // mint to warchest
    let total_balance = if state.total_share.is_zero() {
        Uint128::zero()
    } else {
        (load_token_balance(&deps, &config.spec_token, &state.contract_addr)? - state.poll_deposit)?
    };
    let mut mint_share = Uint128::zero();
    let mut to_warchest = Uint128::zero();
    if config.warchest_address != CanonicalAddr::default()
        && config.warchest_ratio != Decimal::zero()
    {
        to_warchest = mintable * config.warchest_ratio;
        let share = state.calc_share(to_warchest, total_balance);
        let key = config.warchest_address.as_slice();
        let mut account = account_store(&mut deps.storage)
            .may_load(key)?
            .unwrap_or_default();
        account.share += share;
        mint_share += share;
        account_store(&mut deps.storage).save(key, &account)?;
    }

    // mint to vault
    let vaults = read_vaults(&deps.storage)?;
    let share = state.calc_share((mintable - to_warchest)?, total_balance);
    for (addr, vault) in vaults.into_iter() {
        let key = addr.as_slice();
        let mut account = account_store(&mut deps.storage)
            .may_load(key)?
            .unwrap_or_default();
        let vault_share = share.multiply_ratio(vault.weight, state.total_weight as u128);
        account.share += vault_share;
        mint_share += vault_share;
        account_store(&mut deps.storage).save(key, &account)?;
    }
    state.total_share += mint_share;
    state.last_mint = env.block.height;
    state_store(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.spec_token)?,
            msg: to_binary(&Cw20HandleMsg::Mint {
                amount: mintable,
                recipient: deps.api.human_address(&state.contract_addr)?,
            })?,
            send: vec![],
        })],
        data: None,
        log: vec![log("action", "mint"), log("amount", mintable.to_string())],
    })
}

pub fn stake_tokens<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    sender: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    if amount.is_zero() {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    let sender_address_raw = deps.api.canonical_address(&sender)?;
    let key = sender_address_raw.as_slice();

    let mut account = account_store(&mut deps.storage)
        .may_load(key)?
        .unwrap_or_default();
    let config = read_config(&deps.storage)?;
    let mut state = state_store(&mut deps.storage).load()?;

    // balance already increased, so subtract deposit amount
    let current_balance = load_token_balance(&deps, &config.spec_token, &state.contract_addr)?;
    let total_balance = (current_balance - (state.poll_deposit + amount))?;

    let share = state.calc_share(amount, total_balance);
    account.share += share;
    state.total_share += share;

    state_store(&mut deps.storage).save(&state)?;
    account_store(&mut deps.storage).save(key, &account)?;

    Ok(HandleResponse {
        messages: vec![],
        data: None,
        log: vec![
            log("action", "staking"),
            log("sender", sender.as_str()),
            log("share", share.to_string()),
            log("amount", amount.to_string()),
        ],
    })
}

// Withdraw amount if not staked. By default all funds will be withdrawn.
pub fn withdraw<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Option<Uint128>,
) -> HandleResult {
    let sender_address_raw = deps.api.canonical_address(&env.message.sender)?;
    let key = sender_address_raw.as_slice();

    if let Some(mut account) = account_store(&mut deps.storage).may_load(key)? {
        let config = read_config(&deps.storage)?;
        let mut state = state_store(&mut deps.storage).load()?;

        // Load total share & total balance except proposal deposit amount
        let total_share = state.total_share.u128();
        let total_balance = (load_token_balance(&deps, &config.spec_token, &state.contract_addr)?
            - state.poll_deposit)?
            .u128();

        let locked_balance = compute_locked_balance(deps, &mut account, &sender_address_raw)?;
        let locked_share = locked_balance * total_share / total_balance;
        let user_share = account.share.u128();

        let withdraw_share = amount
            .map(|v| std::cmp::max(v.multiply_ratio(total_share, total_balance).u128(), 1u128))
            .unwrap_or_else(|| user_share - locked_share);
        let withdraw_amount = amount
            .map(|v| v.u128())
            .unwrap_or_else(|| withdraw_share * total_balance / total_share);

        if locked_share + withdraw_share > user_share {
            Err(StdError::generic_err(
                "User is trying to withdraw too many tokens.",
            ))
        } else {
            let share = user_share - withdraw_share;
            account.share = Uint128::from(share);

            account_store(&mut deps.storage).save(key, &account)?;

            state.total_share = Uint128::from(total_share - withdraw_share);
            state_store(&mut deps.storage).save(&state)?;

            send_tokens(
                &deps.api,
                &config.spec_token,
                &sender_address_raw,
                withdraw_amount,
                "withdraw",
            )
        }
    } else {
        Err(StdError::generic_err("Nothing staked"))
    }
}

// removes not in-progress poll voter info & unlock tokens
// and returns the largest locked amount in participated polls.
fn compute_locked_balance<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    account: &mut Account,
    voter: &CanonicalAddr,
) -> StdResult<u128> {
    // filter out not in-progress polls
    account.locked_balance.retain(|(poll_id, _)| {
        let poll = read_poll(&deps.storage, &poll_id.to_be_bytes())
            .unwrap()
            .unwrap();

        if poll.status != PollStatus::in_progress {
            // remove voter info from the poll
            poll_voter_store(&mut deps.storage, *poll_id).remove(voter.as_slice());
        }

        poll.status == PollStatus::in_progress
    });

    Ok(account
        .locked_balance
        .iter()
        .map(|(_, v)| v.balance.u128())
        .max()
        .unwrap_or_default())
}

pub fn upsert_vault<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    vault_address: HumanAddr,
    weight: u32,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    if config.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }
    let mut state = state_store(&mut deps.storage).load()?;
    validate_minted(&state, &config, env.block.height)?;

    let vault_address_raw = deps.api.canonical_address(&vault_address)?;
    let key = vault_address_raw.as_slice();
    let mut vault = vault_store(&mut deps.storage)
        .may_load(key)?
        .unwrap_or_default();

    state.total_weight = state.total_weight + weight - vault.weight;
    vault.weight = weight;

    state_store(&mut deps.storage).save(&state)?;

    if weight == 0 {
        vault_store(&mut deps.storage).remove(key);
    } else {
        vault_store(&mut deps.storage).save(key, &vault)?;
    }

    Ok(HandleResponse {
        messages: vec![],
        data: None,
        log: vec![log("new_total_weight", state.total_weight.to_string())],
    })
}

pub fn query_balances<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
    height: Option<u64>,
) -> StdResult<BalanceResponse> {
    let addr_raw = deps.api.canonical_address(&address).unwrap();
    let config = read_config(&deps.storage)?;
    let state = read_state(&deps.storage)?;
    let mut account = read_account(&deps.storage, addr_raw.as_slice())?.unwrap_or_default();

    // filter out not in-progress polls
    account.locked_balance.retain(|(poll_id, _)| {
        let poll = read_poll(&deps.storage, &poll_id.to_be_bytes())
            .unwrap()
            .unwrap();

        poll.status == PollStatus::in_progress
    });

    let total_balance = if state.total_share.is_zero() {
        Uint128::zero()
    } else {
        (load_token_balance(&deps, &config.spec_token, &state.contract_addr)? - state.poll_deposit)?
    };

    let mut balance = account.calc_balance(total_balance, state.total_share);
    let mut share = account.share;
    if let Some(height) = height {
        if addr_raw == config.warchest_address {
            let mintable = calc_mintable(&state, &config, height);
            let to_warchest = mintable * config.warchest_ratio;
            share += state.calc_share(to_warchest, total_balance);
            balance += to_warchest;
        } else if let Some(vault) = read_vault(&deps.storage, addr_raw.as_slice())? {
            let mintable = calc_mintable(&state, &config, height);
            let to_warchest = mintable * config.warchest_ratio;
            let mintable = (mintable - to_warchest)?;
            let vaults_share = state.calc_share(mintable, total_balance);
            share += vaults_share.multiply_ratio(vault.weight, state.total_weight);
            balance += mintable.multiply_ratio(vault.weight, state.total_weight);
        }
    }

    Ok(BalanceResponse {
        balance,
        share,
        locked_balance: account.locked_balance,
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

pub fn query_vaults<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<VaultsResponse> {
    let vaults = read_vaults(&deps.storage)?;
    Ok(VaultsResponse {
        vaults: vaults
            .into_iter()
            .map(|it| VaultInfo {
                address: deps.api.human_address(&it.0).unwrap(),
                weight: it.1.weight,
            })
            .collect(),
    })
}
