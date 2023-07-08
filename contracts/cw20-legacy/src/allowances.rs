use cosmwasm_std::{
    attr, Addr, Api, Binary, BlockInfo, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Storage, Uint128,
};
use cw20::{AllowanceResponse, Cw20ReceiveMsg, Expiration};

use crate::error::ContractError;
use crate::state::{ALLOWANCES, BALANCES, TOKEN_INFO};

pub fn execute_increase_allowance(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, ContractError> {
    let spender_addr = deps.api.addr_validate(&spender)?;
    if spender_addr == info.sender {
        return Err(ContractError::CannotSetOwnAccount {});
    }

    ALLOWANCES.update(
        deps.storage,
        (
            deps.api
                .addr_canonicalize(&info.sender.to_string())?
                .as_slice(),
            deps.api
                .addr_canonicalize(&spender_addr.to_string())?
                .as_slice(),
        ),
        |allow| -> StdResult<_> {
            let mut val = allow.unwrap_or_default();
            if let Some(exp) = expires {
                val.expires = exp;
            }
            val.allowance += amount;
            Ok(val)
        },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "increase_allowance"),
        attr("owner", info.sender),
        attr("spender", spender),
        attr("amount", amount),
    ]))
}

pub fn execute_decrease_allowance(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, ContractError> {
    let spender_addr = deps.api.addr_validate(&spender)?;
    if spender_addr == info.sender {
        return Err(ContractError::CannotSetOwnAccount {});
    }

    let sender_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let spender_raw = deps.api.addr_canonicalize(spender.as_str())?;

    let key = (sender_raw.as_slice(), spender_raw.as_slice());
    // load value and delete if it hits 0, or update otherwise
    let mut allowance = ALLOWANCES.load(deps.storage, key)?;
    if amount < allowance.allowance {
        // update the new amount
        allowance.allowance = allowance
            .allowance
            .checked_sub(amount)
            .map_err(StdError::overflow)?;
        if let Some(exp) = expires {
            allowance.expires = exp;
        }
        ALLOWANCES.save(deps.storage, key, &allowance)?;
    } else {
        ALLOWANCES.remove(deps.storage, key);
    }

    Ok(Response::new().add_attributes(vec![
        attr("action", "decrease_allowance"),
        attr("owner", info.sender),
        attr("spender", spender),
        attr("amount", amount),
    ]))
}

// this can be used to update a lower allowance - call bucket.update with proper keys
pub fn deduct_allowance(
    storage: &mut dyn Storage,
    api: &dyn Api,
    owner: &Addr,
    spender: &Addr,
    block: &BlockInfo,
    amount: Uint128,
) -> Result<AllowanceResponse, ContractError> {
    ALLOWANCES.update(
        storage,
        (
            api.addr_canonicalize(&owner.to_string())?.as_slice(),
            api.addr_canonicalize(&spender.to_string())?.as_slice(),
        ),
        |current| {
            match current {
                Some(mut a) => {
                    if a.expires.is_expired(block) {
                        Err(ContractError::Expired {})
                    } else {
                        // deduct the allowance if enough
                        a.allowance = a
                            .allowance
                            .checked_sub(amount)
                            .map_err(StdError::overflow)?;
                        Ok(a)
                    }
                }
                None => Err(ContractError::NoAllowance {}),
            }
        },
    )
}

pub fn execute_transfer_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    let owner_addr = deps.api.addr_validate(&owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(
        deps.storage,
        deps.api,
        &owner_addr,
        &info.sender,
        &env.block,
        amount,
    )?;

    BALANCES.update(
        deps.storage,
        deps.api
            .addr_canonicalize(&owner_addr.to_string())?
            .as_slice(),
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        deps.api
            .addr_canonicalize(&rcpt_addr.to_string())?
            .as_slice(),
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "transfer_from"),
        attr("from", owner),
        attr("to", recipient),
        attr("by", info.sender),
        attr("amount", amount),
    ]))
}

pub fn execute_burn_from(
    deps: DepsMut,

    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let owner_addr = deps.api.addr_validate(&owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(
        deps.storage,
        deps.api,
        &owner_addr,
        &info.sender,
        &env.block,
        amount,
    )?;

    // lower balance
    BALANCES.update(
        deps.storage,
        deps.api
            .addr_canonicalize(&owner_addr.to_string())?
            .as_slice(),
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    // reduce total_supply
    TOKEN_INFO.update(deps.storage, |mut meta| -> StdResult<_> {
        meta.total_supply = meta.total_supply.checked_sub(amount)?;
        Ok(meta)
    })?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "burn_from"),
        attr("from", owner),
        attr("by", info.sender),
        attr("amount", amount),
    ]))
}

pub fn execute_send_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    let rcpt_addr = deps.api.addr_validate(&contract)?;
    let owner_addr = deps.api.addr_validate(&owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(
        deps.storage,
        deps.api,
        &owner_addr,
        &info.sender,
        &env.block,
        amount,
    )?;

    // move the tokens to the contract
    BALANCES.update(
        deps.storage,
        deps.api
            .addr_canonicalize(&owner_addr.to_string())?
            .as_slice(),
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        deps.api
            .addr_canonicalize(&rcpt_addr.to_string())?
            .as_slice(),
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let attrs = vec![
        attr("action", "send_from"),
        attr("from", &owner),
        attr("to", &contract),
        attr("by", &info.sender),
        attr("amount", amount),
    ];

    Ok(Response::new().add_attributes(attrs).add_message(
        // create a send message
        Cw20ReceiveMsg {
            sender: info.sender.into(),
            amount,
            msg,
        }
        .into_cosmos_msg(contract)?,
    ))
}

pub fn query_allowance(deps: Deps, owner: String, spender: String) -> StdResult<AllowanceResponse> {
    let owner_addr = deps.api.addr_validate(&owner)?;
    let spender_addr = deps.api.addr_validate(&spender)?;
    let allowance = ALLOWANCES
        .may_load(
            deps.storage,
            (
                deps.api.addr_canonicalize(owner_addr.as_str())?.as_slice(),
                deps.api
                    .addr_canonicalize(spender_addr.as_str())?
                    .as_slice(),
            ),
        )?
        .unwrap_or_default();
    Ok(allowance)
}
