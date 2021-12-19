use cosmwasm_std::{CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, to_binary, Uint128, WasmMsg};
use cw20::{Cw20ExecuteMsg};
use spectrum_protocol::gov_proxy::StakerResponse;
use astroport::staking::{Cw20HookMsg as XAstroCw20HookMsg};
use crate::querier::query_xastro_gov;
use crate::state::{account_store, read_account, read_config};

pub fn query_staker_info_gov(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<StakerResponse> {
    let config = read_config(deps.storage)?;

    let gov_response = query_xastro_gov(deps, &config, &env.contract.address)?;
    let addr_raw = deps.api.addr_canonicalize(&address).unwrap();
    let account = read_account(deps.storage, addr_raw.as_slice())?
        .unwrap_or_default();
    let balance = gov_response.calc_balance(account.share);

    Ok(StakerResponse {
        balance,
    })
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: String,
    amount: Uint128,
) -> StdResult<Response> {

    if amount.is_zero() {
        return Err(StdError::generic_err("Insufficient funds sent"));
    }

    let config = read_config(deps.storage)?;
    if config.farm_token != deps.api.addr_canonicalize(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let gov_response = query_xastro_gov(deps.as_ref(), &config, &env.contract.address)?;
    let sender_address_raw = deps.api.addr_canonicalize(&sender)?;
    let key = sender_address_raw.as_slice();
    let mut account = account_store(deps.storage)
        .may_load(key)?
        .unwrap_or_default();

    let share = gov_response.calc_share(amount);
    account.share += share;

    account_store(deps.storage).save(key, &account)?;

    let xastro_token = deps.api.addr_humanize(&config.xastro_token)?;
    let xastro_gov = deps.api.addr_humanize(&config.farm_gov)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xastro_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: xastro_gov.to_string(),
                msg: to_binary(&XAstroCw20HookMsg::Enter {})?,
                amount,
            })?,
        })]))
}

pub fn unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>
) -> StdResult<Response> {

    let sender_address_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let key = sender_address_raw.as_slice();

    let config = read_config(deps.storage)?;

    let gov_response = query_xastro_gov(deps.as_ref(), &config, &env.contract.address)?;
    let mut account = account_store(deps.storage).load(key)?;
    let user_balance = gov_response.calc_balance(account.share);
    let amount = amount.unwrap_or(user_balance);
    if amount > user_balance {
        return Err(StdError::generic_err(
            "User is trying to withdraw too many tokens.",
        ));
    }

    let mut withdraw_share = gov_response.calc_share(amount);
    if gov_response.calc_balance(withdraw_share) < amount {
        withdraw_share += Uint128::from(1u128);
    }

    account.share -= withdraw_share;
    account_store(deps.storage).save(key, &account)?;

    let astro_token = deps.api.addr_humanize(&config.farm_token)?;
    let xastro_token = deps.api.addr_humanize(&config.xastro_token)?;
    let xastro_gov = deps.api.addr_humanize(&config.farm_gov)?;

    Ok(Response::new().add_messages(vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xastro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: xastro_gov.to_string(),
                msg: to_binary(&XAstroCw20HookMsg::Leave {})?,
                amount: withdraw_share,
            })?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: gov_response.calc_balance(withdraw_share),
            })?,
            funds: vec![],
        })
    ]))
}
