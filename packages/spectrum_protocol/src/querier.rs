use cosmwasm_std::{
    from_binary, to_binary, Addr, BalanceResponse, BankQuery, Binary, CanonicalAddr,
    CosmosMsg, Deps, DepsMut, QueryRequest, Response, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use cw20::Cw20ExecuteMsg;

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}

pub fn query_balance(deps: Deps, account_addr: Addr, denom: String) -> StdResult<Uint128> {
    let balance: BalanceResponse = deps.querier.query(&QueryRequest::Bank(BankQuery::Balance {
        address: account_addr.to_string(),
        denom,
    }))?;
    Ok(balance.amount.amount)
}

pub fn load_token_balance(
    deps: Deps,
    contract_addr: &CanonicalAddr,
    account_addr: &CanonicalAddr,
) -> StdResult<Uint128> {
    // load balance form the token contract
    let res = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: deps.api.addr_humanize(contract_addr)?.to_string(),
            key: Binary::from(concat(
                &to_length_prefixed(b"balance").to_vec(),
                account_addr.as_slice(),
            )),
        }))
        .unwrap_or_else(|_| to_binary(&Uint128::zero()).unwrap());

    from_binary(&res)
}

pub fn send_tokens(
    deps: DepsMut,
    asset_token: &CanonicalAddr,
    recipient: &CanonicalAddr,
    amount: u128,
    action: &str,
) -> StdResult<Response> {
    let contract_human = deps.api.addr_humanize(asset_token)?.to_string();
    let recipient_human = deps.api.addr_humanize(recipient)?.to_string();

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_human,
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient_human.clone(),
                amount: Uint128::from(amount),
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            ("action", action),
            ("recipient", recipient_human.as_str()),
            ("amount", amount.to_string().as_str()),
        ]))
}
