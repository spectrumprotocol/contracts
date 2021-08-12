use cosmwasm_std::{
    log, to_binary, Api, CanonicalAddr, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HandleResult, String, Order, Querier, QueryRequest, StdError, StdResult, Storage, Uint128,
    WasmMsg, WasmQuery,
};

use crate::state::{
    pool_info_read, pool_info_store, read_config, read_state, rewards_read, rewards_store,
    state_store, Config, PoolInfo, RewardInfo, State,
};

use cw20::Cw20HandleMsg;

use crate::querier::query_pylon_reward_info;
use pylon_token::gov::{
    HandleMsg as PylonGovHandleMsg, QueryMsg as PylonGovQueryMsg,
    StakerResponse as PylonStakerResponse,
};
use pylon_token::staking::{Cw20HookMsg as PylonCw20HookMsg, HandleMsg as PylonStakingHandleMsg};
use spectrum_protocol::gov::{
    BalanceResponse as SpecBalanceResponse, HandleMsg as SpecHandleMsg, QueryMsg as SpecQueryMsg,
};
use spectrum_protocol::math::UDec128;
use spectrum_protocol::pylon_farm::{RewardInfoResponse, RewardInfoResponseItem};

pub fn bond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    sender_addr: String,
    asset_token: String,
    amount: Uint128,
    compound_rate: Option<Decimal>,
) -> HandleResult {
    let asset_token_raw = deps.api.canonical_address(&asset_token)?;
    let sender_addr_raw = deps.api.canonical_address(&sender_addr)?;

    let mut pool_info = pool_info_read(&deps.storage).load(asset_token_raw.as_slice())?;

    // only staking token contract can execute this message
    if pool_info.staking_token != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    let mut state = read_state(&deps.storage)?;

    let config = read_config(&deps.storage)?;
    let lp_balance = query_pylon_reward_info(
        &deps,
        &config.pylon_staking,
        &state.contract_addr,
        env.block.height,
    )?
    .bond_amount;

    // update reward index; before changing share
    if !pool_info.total_auto_bond_share.is_zero() || !pool_info.total_stake_bond_share.is_zero() {
        deposit_spec_reward(deps, &mut state, &config, env.block.height, false)?;
        spec_reward_to_pool(&state, &mut pool_info, lp_balance)?;
    }

    // withdraw reward to pending reward; before changing share
    let mut reward_info = rewards_read(&deps.storage, &sender_addr_raw)
        .may_load(asset_token_raw.as_slice())?
        .unwrap_or_else(|| RewardInfo {
            farm_share_index: pool_info.farm_share_index,
            auto_spec_share_index: pool_info.auto_spec_share_index,
            stake_spec_share_index: pool_info.stake_spec_share_index,
            auto_bond_share: Uint128::zero(),
            stake_bond_share: Uint128::zero(),
            spec_share: Uint128::zero(),
            accum_spec_share: Uint128::zero(),
            farm_share: Uint128::zero(),
        });
    before_share_change(&pool_info, &mut reward_info)?;

    // increase bond_amount
    increase_bond_amount(
        &mut pool_info,
        &mut reward_info,
        &config,
        amount,
        compound_rate,
        lp_balance,
    )?;

    rewards_store(&mut deps.storage, &sender_addr_raw)
        .save(&asset_token_raw.as_slice(), &reward_info)?;
    pool_info_store(&mut deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    state_store(&mut deps.storage).save(&state)?;

    stake_token(
        &deps.api,
        &config.pylon_staking,
        &pool_info.staking_token,
        &asset_token_raw,
        amount,
    )
}

pub fn deposit_farm_share<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    pool_info: &mut PoolInfo,
    config: &Config,
    amount: Uint128,
) -> StdResult<()> {
    let mut state = read_state(&deps.storage)?;
    let staked: PylonStakerResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&config.pylon_gov)?,
            msg: to_binary(&PylonGovQueryMsg::Staker {
                address: deps.api.human_address(&state.contract_addr)?,
            })?,
        }))?;

    let mut new_total_share = Uint128::zero();
    if !pool_info.total_stake_bond_share.is_zero() {
        let new_share = state.calc_farm_share(amount, staked.balance);
        let share_per_bond = Decimal::from_ratio(new_share, pool_info.total_stake_bond_share);
        pool_info.farm_share_index = pool_info.farm_share_index + share_per_bond;
        pool_info.farm_share += new_share;
        new_total_share += new_share;
    }

    state.total_farm_share += new_total_share;
    state_store(&mut deps.storage).save(&state)?;

    Ok(())
}

pub fn deposit_spec_reward<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    state: &mut State,
    config: &Config,
    height: u64,
    query: bool,
) -> StdResult<SpecBalanceResponse> {
    if state.total_weight == 0 {
        return Ok(SpecBalanceResponse {
            share: Uint128::zero(),
            balance: Uint128::zero(),
            locked_balance: vec![],
        });
    }

    let staked: SpecBalanceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&config.spectrum_gov)?,
            msg: to_binary(&SpecQueryMsg::balance {
                address: deps.api.human_address(&state.contract_addr)?,
                height: Some(height),
            })?,
        }))?;

    let diff = staked.share - state.previous_spec_share;
    let deposit_share = if query {
        diff.unwrap_or(Uint128::zero())
    } else {
        diff?
    };
    let share_per_weight = Decimal::from_ratio(deposit_share, state.total_weight);
    state.spec_share_index = state.spec_share_index + share_per_weight;
    state.previous_spec_share = staked.share;

    Ok(staked)
}

fn spec_reward_to_pool(
    state: &State,
    pool_info: &mut PoolInfo,
    lp_balance: Uint128,
) -> StdResult<()> {
    if lp_balance.is_zero() {
        return Ok(());
    }

    let share = (UDec128::from(state.spec_share_index) - pool_info.state_spec_share_index.into())
        * Uint128::from(pool_info.weight as u128);
    let stake_share = share * pool_info.total_stake_bond_amount / lp_balance;

    // spec reward to staker is per stake bond share & auto bond share
    if !stake_share.is_zero() {
        let stake_share_per_bond = stake_share / pool_info.total_stake_bond_share;
        pool_info.stake_spec_share_index =
            pool_info.stake_spec_share_index + stake_share_per_bond.into();
    }
    let auto_share = share - stake_share;
    if !auto_share.is_zero() {
        let auto_share_per_bond = auto_share / pool_info.total_auto_bond_share;
        pool_info.auto_spec_share_index =
            pool_info.auto_spec_share_index + auto_share_per_bond.into();
    }
    pool_info.state_spec_share_index = state.spec_share_index;
    Ok(())
}

// withdraw reward to pending reward
fn before_share_change(pool_info: &PoolInfo, reward_info: &mut RewardInfo) -> StdResult<()> {
    let farm_share = (pool_info.farm_share_index - reward_info.farm_share_index.into())
        * reward_info.stake_bond_share;
    reward_info.farm_share += farm_share;
    reward_info.farm_share_index = pool_info.farm_share_index;

    let stake_spec_share = reward_info.stake_bond_share
        * (pool_info.stake_spec_share_index - reward_info.stake_spec_share_index.into());
    let auto_spec_share = reward_info.auto_bond_share
        * (pool_info.auto_spec_share_index - reward_info.auto_spec_share_index.into());
    let spec_share = stake_spec_share + auto_spec_share;
    reward_info.spec_share += spec_share;
    reward_info.accum_spec_share += spec_share;
    reward_info.stake_spec_share_index = pool_info.stake_spec_share_index;
    reward_info.auto_spec_share_index = pool_info.auto_spec_share_index;

    Ok(())
}

// increase share amount in pool and reward info
fn increase_bond_amount(
    pool_info: &mut PoolInfo,
    reward_info: &mut RewardInfo,
    config: &Config,
    amount: Uint128,
    compound_rate: Option<Decimal>,
    lp_balance: Uint128,
) -> StdResult<()> {
    // calculate target state
    let compound_rate = compound_rate.unwrap_or(Decimal::zero());
    let amount_to_auto = amount * compound_rate;
    let amount_to_stake = (amount - amount_to_auto)?;
    let new_balance = lp_balance + amount;
    let new_auto_bond_amount =
        (new_balance - (pool_info.total_stake_bond_amount + amount_to_stake))?;

    // calculate deposit fee; split based on auto balance & stake balance
    let deposit_fee = amount * config.deposit_fee;
    let auto_bond_fee = deposit_fee.multiply_ratio(new_auto_bond_amount, new_balance);
    let stake_bond_fee = (deposit_fee - auto_bond_fee)?;

    // calculate amount after fee
    let remaining_amount = (amount - deposit_fee)?;
    let auto_bond_amount = remaining_amount * compound_rate;
    let stake_bond_amount = (remaining_amount - auto_bond_amount)?;

    // convert amount to share & update
    let auto_bond_share = pool_info.calc_auto_bond_share(auto_bond_amount, lp_balance);
    let stake_bond_share = pool_info.calc_stake_bond_share(stake_bond_amount);
    pool_info.total_auto_bond_share += auto_bond_share;
    pool_info.total_stake_bond_amount += stake_bond_amount + stake_bond_fee;
    pool_info.total_stake_bond_share += stake_bond_share;
    reward_info.auto_bond_share += auto_bond_share;
    reward_info.stake_bond_share += stake_bond_share;

    Ok(())
}

// stake LP token to Pylon Staking
fn stake_token<A: Api>(
    api: &A,
    pylon_staking: &CanonicalAddr,
    staking_token: &CanonicalAddr,
    asset_token: &CanonicalAddr,
    amount: Uint128,
) -> HandleResult {
    let asset_token = api.human_address(asset_token)?;
    let pylon_staking = api.human_address(pylon_staking)?;
    let staking_token = api.human_address(staking_token)?;
    let response = HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.clone(),
            send: vec![],
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: pylon_staking,
                amount,
                msg: Some(to_binary(&PylonCw20HookMsg::Bond {})?),
            })?,
        })],
        log: vec![
            log("action", "bond"),
            log("staking_token", staking_token.as_str()),
            log("asset_token", asset_token.as_str()),
            log("amount", amount.to_string()),
        ],
        data: None,
    };
    Ok(response)
}

pub fn unbond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_token: String,
    amount: Uint128,
) -> HandleResult {
    let staker_addr_raw = deps.api.canonical_address(&env.message.sender)?;
    let asset_token_raw = deps.api.canonical_address(&asset_token)?;

    let config = read_config(&deps.storage)?;
    let mut state = read_state(&deps.storage)?;
    let mut pool_info = pool_info_read(&deps.storage).load(asset_token_raw.as_slice())?;
    let mut reward_info =
        rewards_read(&deps.storage, &staker_addr_raw).load(asset_token_raw.as_slice())?;

    let lp_balance = query_pylon_reward_info(
        &deps,
        &config.pylon_staking,
        &state.contract_addr,
        env.block.height,
    )?
    .bond_amount;
    let user_auto_balance =
        pool_info.calc_user_auto_balance(lp_balance, reward_info.auto_bond_share);
    let user_stake_balance = pool_info.calc_user_stake_balance(reward_info.stake_bond_share);
    let user_balance = user_auto_balance + user_stake_balance;

    if user_balance < amount {
        return Err(StdError::generic_err("Cannot unbond more than bond amount"));
    }

    // distribute reward to pending reward; before changing share
    let config = read_config(&deps.storage)?;
    deposit_spec_reward(deps, &mut state, &config, env.block.height, false)?;
    spec_reward_to_pool(&state, &mut pool_info, lp_balance)?;
    before_share_change(&pool_info, &mut reward_info)?;

    // decrease bond amount
    let auto_bond_amount = if reward_info.stake_bond_share.is_zero() {
        amount
    } else {
        amount.multiply_ratio(user_auto_balance, user_balance)
    };
    let stake_bond_amount = (amount - auto_bond_amount)?;
    let auto_bond_share = pool_info.calc_auto_bond_share(auto_bond_amount, lp_balance);
    let stake_bond_share = pool_info.calc_stake_bond_share(stake_bond_amount);

    pool_info.total_auto_bond_share = (pool_info.total_auto_bond_share - auto_bond_share)?;
    pool_info.total_stake_bond_amount = (pool_info.total_stake_bond_amount - stake_bond_amount)?;
    pool_info.total_stake_bond_share = (pool_info.total_stake_bond_share - stake_bond_share)?;
    reward_info.auto_bond_share = (reward_info.auto_bond_share - auto_bond_share)?;
    reward_info.stake_bond_share = (reward_info.stake_bond_share - stake_bond_share)?;

    // update rewards info
    if reward_info.spec_share.is_zero()
        && reward_info.farm_share.is_zero()
        && reward_info.auto_bond_share.is_zero()
        && reward_info.stake_bond_share.is_zero()
    {
        rewards_store(&mut deps.storage, &staker_addr_raw).remove(asset_token_raw.as_slice());
    } else {
        rewards_store(&mut deps.storage, &staker_addr_raw)
            .save(asset_token_raw.as_slice(), &reward_info)?;
    }

    // update pool info
    pool_info_store(&mut deps.storage).save(asset_token_raw.as_slice(), &pool_info)?;
    state_store(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.human_address(&config.pylon_staking)?,
                send: vec![],
                msg: to_binary(&PylonStakingHandleMsg::Unbond { amount })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.human_address(&pool_info.staking_token)?,
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: env.message.sender.clone(),
                    amount,
                })?,
                send: vec![],
            }),
        ],
        log: vec![
            log("action", "unbond"),
            log("staker_addr", env.message.sender.as_str()),
            log("asset_token", asset_token.as_str()),
            log("amount", amount.to_string()),
        ],
        data: None,
    })
}

pub fn withdraw<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    asset_token: Option<String>,
) -> HandleResult {
    let staker_addr = deps.api.canonical_address(&env.message.sender)?;
    let asset_token = asset_token.map(|a| deps.api.canonical_address(&a).unwrap());
    let mut state = read_state(&deps.storage)?;

    // update pending reward; before withdraw
    let config = read_config(&deps.storage)?;
    let spec_staked = deposit_spec_reward(deps, &mut state, &config, env.block.height, false)?;

    let (spec_amount, spec_share, farm_amount, farm_share) = withdraw_reward(
        deps,
        &config,
        env.block.height,
        &state,
        &staker_addr,
        &asset_token,
        &spec_staked,
    )?;

    state.previous_spec_share = (state.previous_spec_share - spec_share)?;
    state.total_farm_share = (state.total_farm_share - farm_share)?;

    state_store(&mut deps.storage).save(&state)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    if !spec_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.spectrum_gov)?,
            msg: to_binary(&SpecHandleMsg::withdraw {
                amount: Some(spec_amount),
            })?,
            send: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.spectrum_token)?,
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: env.message.sender.clone(),
                amount: spec_amount,
            })?,
            send: vec![],
        }));
    }

    if !farm_amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.pylon_gov)?,
            msg: to_binary(&PylonGovHandleMsg::WithdrawVotingTokens {
                amount: Some(farm_amount),
            })?,
            send: vec![],
        }));
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.pylon_token)?,
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: env.message.sender.clone(),
                amount: farm_amount,
            })?,
            send: vec![],
        }));
    }

    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "withdraw"),
            log("farm_amount", farm_amount.to_string()),
            log("spec_amount", spec_amount.to_string()),
        ],
        data: None,
    })
}

fn withdraw_reward<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    config: &Config,
    height: u64,
    state: &State,
    staker_addr: &CanonicalAddr,
    asset_token: &Option<CanonicalAddr>,
    spec_staked: &SpecBalanceResponse,
) -> StdResult<(Uint128, Uint128, Uint128, Uint128)> {
    let rewards_bucket = rewards_read(&deps.storage, &staker_addr);

    // single reward withdraw; or all rewards
    let reward_pairs: Vec<(CanonicalAddr, RewardInfo)>;
    if let Some(asset_token) = asset_token {
        let key = asset_token.as_slice();
        let reward_info = rewards_bucket.may_load(key)?;
        reward_pairs = if let Some(reward_info) = reward_info {
            vec![(asset_token.clone(), reward_info)]
        } else {
            vec![]
        };
    } else {
        reward_pairs = rewards_bucket
            .range(None, None, Order::Ascending)
            .map(|item| {
                let (k, v) = item?;
                Ok((CanonicalAddr::from(k), v))
            })
            .collect::<StdResult<Vec<(CanonicalAddr, RewardInfo)>>>()?;
    }

    let farm_staked: PylonStakerResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&config.pylon_gov)?,
            msg: to_binary(&PylonGovQueryMsg::Staker {
                address: deps.api.human_address(&state.contract_addr)?,
            })?,
        }))?;

    let pylon_reward_info =
        query_pylon_reward_info(&deps, &config.pylon_staking, &state.contract_addr, height)?;

    let mut spec_amount = Uint128::zero();
    let mut spec_share = Uint128::zero();
    let mut farm_amount = Uint128::zero();
    let mut farm_share = Uint128::zero();
    for reward_pair in reward_pairs {
        let (asset_token_raw, mut reward_info) = reward_pair;

        // withdraw reward to pending reward
        let key = asset_token_raw.as_slice();
        let mut pool_info = pool_info_read(&deps.storage).load(key)?;
        let lp_balance = pylon_reward_info.bond_amount;
        spec_reward_to_pool(state, &mut pool_info, lp_balance)?;
        before_share_change(&pool_info, &mut reward_info)?;

        // update withdraw
        farm_share += reward_info.farm_share;
        farm_amount += calc_farm_balance(
            reward_info.farm_share,
            farm_staked.balance,
            state.total_farm_share,
        );

        let locked_share = config.calc_locked_reward(reward_info.accum_spec_share, height);
        let withdraw_share = if locked_share >= reward_info.spec_share {
            Uint128::zero()
        } else {
            (reward_info.spec_share - locked_share)?
        };
        spec_share += withdraw_share;
        spec_amount += calc_spec_balance(withdraw_share, spec_staked);
        pool_info.farm_share = (pool_info.farm_share - reward_info.farm_share)?;
        reward_info.farm_share = Uint128::zero();
        reward_info.spec_share = locked_share;

        // update rewards info
        pool_info_store(&mut deps.storage).save(key, &pool_info)?;
        if reward_info.spec_share.is_zero()
            && reward_info.farm_share.is_zero()
            && reward_info.auto_bond_share.is_zero()
            && reward_info.stake_bond_share.is_zero()
        {
            rewards_store(&mut deps.storage, &staker_addr).remove(key);
        } else {
            rewards_store(&mut deps.storage, &staker_addr).save(key, &reward_info)?;
        }
    }

    Ok((spec_amount, spec_share, farm_amount, farm_share))
}

fn calc_farm_balance(share: Uint128, total_balance: Uint128, total_farm_share: Uint128) -> Uint128 {
    if total_farm_share.is_zero() {
        Uint128::zero()
    } else {
        total_balance.multiply_ratio(share, total_farm_share)
    }
}

fn calc_spec_balance(share: Uint128, staked: &SpecBalanceResponse) -> Uint128 {
    if staked.share.is_zero() {
        Uint128::zero()
    } else {
        share.multiply_ratio(staked.balance, staked.share)
    }
}

pub fn query_reward_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    staker_addr: String,
    height: u64,
) -> StdResult<RewardInfoResponse> {
    let staker_addr_raw = deps.api.canonical_address(&staker_addr)?;
    let mut state = read_state(&deps.storage)?;

    let config = read_config(&deps.storage)?;
    let spec_staked = deposit_spec_reward(deps, &mut state, &config, height, true)?;
    let reward_infos = read_reward_infos(
        deps,
        &config,
        height,
        &state,
        &staker_addr_raw,
        &spec_staked,
    )?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_infos,
    })
}

fn read_reward_infos<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    config: &Config,
    height: u64,
    state: &State,
    staker_addr: &CanonicalAddr,
    spec_staked: &SpecBalanceResponse,
) -> StdResult<Vec<RewardInfoResponseItem>> {
    let rewards_bucket = rewards_read(&deps.storage, &staker_addr);

    let reward_pair = rewards_bucket
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, v) = item?;
            Ok((CanonicalAddr::from(k), v))
        })
        .collect::<StdResult<Vec<(CanonicalAddr, RewardInfo)>>>()?;

    let farm_staked: PylonStakerResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&config.pylon_gov)?,
            msg: to_binary(&PylonGovQueryMsg::Staker {
                address: deps.api.human_address(&state.contract_addr)?,
            })?,
        }))?;

    let pylon_reward_infos =
        query_pylon_reward_info(&deps, &config.pylon_staking, &state.contract_addr, height)?;

    let bucket = pool_info_read(&deps.storage);
    let reward_infos: Vec<RewardInfoResponseItem> = reward_pair
        .into_iter()
        .map(|(asset_token_raw, reward_info)| {
            let mut pool_info = bucket.load(asset_token_raw.as_slice())?;

            // update pending rewards
            let mut reward_info = reward_info;
            let lp_balance = pylon_reward_infos.bond_amount;
            let farm_share_index = reward_info.farm_share_index;
            let auto_spec_index = reward_info.auto_spec_share_index;
            let stake_spec_index = reward_info.stake_spec_share_index;

            spec_reward_to_pool(state, &mut pool_info, lp_balance)?;
            before_share_change(&pool_info, &mut reward_info)?;

            let auto_bond_amount =
                pool_info.calc_user_auto_balance(lp_balance, reward_info.auto_bond_share);
            let stake_bond_amount = pool_info.calc_user_stake_balance(reward_info.stake_bond_share);
            let locked_spec_share = config.calc_locked_reward(reward_info.accum_spec_share, height);
            Ok(RewardInfoResponseItem {
                asset_token: deps.api.human_address(&asset_token_raw)?,
                farm_share_index,
                auto_spec_share_index: auto_spec_index,
                stake_spec_share_index: stake_spec_index,
                bond_amount: auto_bond_amount + stake_bond_amount,
                auto_bond_amount,
                stake_bond_amount,
                farm_share: reward_info.farm_share,
                auto_bond_share: reward_info.auto_bond_share,
                stake_bond_share: reward_info.stake_bond_share,
                spec_share: reward_info.spec_share,
                pending_spec_reward: calc_spec_balance(reward_info.spec_share, spec_staked),
                pending_farm_reward: calc_farm_balance(
                    reward_info.farm_share,
                    farm_staked.balance,
                    state.total_farm_share,
                ),
                accum_spec_share: reward_info.accum_spec_share,
                locked_spec_share,
                locked_spec_reward: calc_spec_balance(locked_spec_share, spec_staked),
            })
        })
        .collect::<StdResult<Vec<RewardInfoResponseItem>>>()?;
    Ok(reward_infos)
}
