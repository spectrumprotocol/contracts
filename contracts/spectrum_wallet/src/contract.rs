use cosmwasm_std::{
    log, to_binary, Api, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse, HandleResult,
    HumanAddr, InitResponse, MigrateResponse, MigrateResult, Querier, QueryRequest, StdError,
    StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use crate::state::{
    config_store, read_config, read_reward, read_rewards, read_state, reward_store, state_store,
    Config, RewardInfo, State,
};
use cw20::Cw20HandleMsg;
use spectrum_protocol::gov::{
    BalanceResponse as GovBalanceResponse, QueryMsg as GovQueryMsg, VoteOption,
};
use spectrum_protocol::wallet::{
    BalanceResponse, ConfigInfo, HandleMsg, MigrateMsg, QueryMsg, ShareInfo, SharesResponse,
    StateInfo,
};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: ConfigInfo,
) -> StdResult<InitResponse> {
    config_store(&mut deps.storage).save(&Config {
        owner: deps.api.canonical_address(&msg.owner)?,
        spectrum_token: deps.api.canonical_address(&msg.spectrum_token)?,
        spectrum_gov: deps.api.canonical_address(&msg.spectrum_gov)?,
    })?;

    state_store(&mut deps.storage).save(&State {
        contract_addr: deps.api.canonical_address(&env.contract.address)?,
        previous_share: Uint128::zero(),
        share_index: Decimal::zero(),
        total_weight: 0u32,
    })?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::poll_votes {
            poll_id,
            vote,
            amount,
        } => poll_votes(deps, env, poll_id, vote, amount),
        HandleMsg::withdraw { amount } => withdraw(deps, env, amount),
        HandleMsg::upsert_share { address, weight } => upsert_share(deps, env, address, weight),
        HandleMsg::update_config { owner } => update_config(deps, env, owner),
    }
}

fn poll_votes<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    poll_id: u64,
    vote: VoteOption,
    amount: Uint128,
) -> HandleResult {
    let shares = read_rewards(&deps.storage)?;
    if shares.len() != 1 || shares[0].0 != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    let config = read_config(&deps.storage)?;
    Ok(HandleResponse {
        log: vec![],
        data: None,
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.human_address(&config.spectrum_gov)?,
            msg: to_binary(&HandleMsg::poll_votes {
                poll_id,
                vote,
                amount,
            })?,
            send: vec![],
        })],
    })
}

fn withdraw<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Option<Uint128>,
) -> HandleResult {
    let staker_addr = deps.api.canonical_address(&env.message.sender)?;
    let mut state = read_state(&deps.storage)?;

    let config = read_config(&deps.storage)?;
    let staked = deposit_reward(deps, &mut state, &config, env.block.height, false)?;

    let mut reward_info = read_reward(&deps.storage, &staker_addr)?;
    before_share_change(&state, &mut reward_info)?;

    let (amount, share) = if let Some(amount) = amount {
        (amount, amount.multiply_ratio(staked.share, staked.balance))
    } else {
        (calc_balance(reward_info.share, &staked), reward_info.share)
    };
    reward_info.share = (reward_info.share - share)?;
    reward_store(&mut deps.storage).save(staker_addr.as_slice(), &reward_info)?;

    state.previous_share = (state.previous_share - share)?;
    state_store(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.human_address(&config.spectrum_gov)?,
                msg: to_binary(&HandleMsg::withdraw {
                    amount: Some(amount),
                })?,
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.human_address(&config.spectrum_token)?,
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: env.message.sender,
                    amount,
                })?,
                send: vec![],
            }),
        ],
        log: vec![log("action", "withdraw"), log("amount", amount.to_string())],
        data: None,
    })
}

fn deposit_reward<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    state: &mut State,
    config: &Config,
    height: u64,
    query: bool,
) -> StdResult<GovBalanceResponse> {
    if state.total_weight == 0u32 {
        return Ok(GovBalanceResponse {
            share: Uint128::zero(),
            balance: Uint128::zero(),
            locked_balance: vec![],
        });
    }

    let staked: GovBalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps.api.human_address(&config.spectrum_gov)?,
        msg: to_binary(&GovQueryMsg::balance {
            address: deps.api.human_address(&state.contract_addr)?,
            height: Some(height),
        })?,
    }))?;
    let diff = staked.share - state.previous_share;
    let deposit_share = if query {
        diff.unwrap_or(Uint128::zero())
    } else {
        diff?
    };
    let share_per_weight = Decimal::from_ratio(deposit_share, state.total_weight);
    state.share_index = state.share_index + share_per_weight;
    state.previous_share = staked.share;

    Ok(staked)
}

fn before_share_change(state: &State, reward_info: &mut RewardInfo) -> StdResult<()> {
    let share = Uint128::from(reward_info.weight as u128)
        * (state.share_index - reward_info.share_index.into());
    reward_info.share += share;
    reward_info.share_index = state.share_index;

    Ok(())
}

fn upsert_share<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    address: HumanAddr,
    weight: u32,
) -> HandleResult {
    let config = read_config(&deps.storage)?;
    if config.owner != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }
    let mut state = state_store(&mut deps.storage).load()?;
    deposit_reward(deps, &mut state, &config, env.block.height, false)?;

    let address_raw = deps.api.canonical_address(&address)?;
    let key = address_raw.as_slice();
    let mut reward_info = reward_store(&mut deps.storage)
        .may_load(key)?
        .unwrap_or_default();

    state.total_weight = state.total_weight + weight - reward_info.weight;
    reward_info.weight = weight;

    state_store(&mut deps.storage).save(&state)?;

    if weight == 0 {
        reward_store(&mut deps.storage).remove(key);
    } else {
        reward_store(&mut deps.storage).save(key, &reward_info)?;
    }

    Ok(HandleResponse {
        messages: vec![],
        data: None,
        log: vec![log("new_total_weight", state.total_weight.to_string())],
    })
}

fn update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    let mut config = read_config(&deps.storage)?;

    if deps.api.canonical_address(&env.message.sender)? != config.owner {
        return Err(StdError::unauthorized());
    }

    if let Some(owner) = owner {
        config.owner = deps.api.canonical_address(&owner)?;
    }

    config_store(&mut deps.storage).save(&config)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_config")],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::config {} => to_binary(&query_config(deps)?),
        QueryMsg::balance { address, height } => to_binary(&query_balance(deps, address, height)?),
        QueryMsg::shares {} => to_binary(&query_shares(deps)?),
        QueryMsg::state {} => to_binary(&query_state(deps)?),
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<ConfigInfo> {
    let config = read_config(&deps.storage)?;
    let resp = ConfigInfo {
        owner: deps.api.human_address(&config.owner)?,
        spectrum_token: deps.api.human_address(&config.spectrum_token)?,
        spectrum_gov: deps.api.human_address(&config.spectrum_gov)?,
    };

    Ok(resp)
}

pub fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    staker_addr: HumanAddr,
    height: u64,
) -> StdResult<BalanceResponse> {
    let staker_addr_raw = deps.api.canonical_address(&staker_addr)?;
    let mut state = read_state(&deps.storage)?;

    let config = read_config(&deps.storage)?;
    let staked = deposit_reward(deps, &mut state, &config, height, true)?;
    let mut reward_info = read_reward(&deps.storage, &staker_addr_raw)?;
    before_share_change(&state, &mut reward_info)?;

    Ok(BalanceResponse {
        share: reward_info.share,
        balance: calc_balance(reward_info.share, &staked),
    })
}

fn calc_balance(share: Uint128, staked: &GovBalanceResponse) -> Uint128 {
    if staked.share.is_zero() {
        Uint128::zero()
    } else {
        share.multiply_ratio(staked.balance, staked.share)
    }
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateInfo> {
    let state = read_state(&deps.storage)?;
    Ok(StateInfo {
        previous_share: state.previous_share,
        share_index: state.share_index,
        total_weight: state.total_weight,
    })
}

fn query_shares<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<SharesResponse> {
    let shares = read_rewards(&deps.storage)?;
    Ok(SharesResponse {
        shares: shares
            .into_iter()
            .map(|it| ShareInfo {
                address: deps.api.human_address(&it.0).unwrap(),
                weight: it.1.weight,
                share_index: it.1.share_index,
                share: it.1.share,
            })
            .collect(),
    })
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    Ok(MigrateResponse::default())
}
