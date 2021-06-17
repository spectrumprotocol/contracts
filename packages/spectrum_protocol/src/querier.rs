use cosmwasm_std::{
    from_binary, log, to_binary, Api, BalanceResponse, BankQuery, Binary, CanonicalAddr, CosmosMsg,
    Extern, HandleResponse, HandleResult, HumanAddr, Querier, QueryRequest, StdResult, Storage,
    Uint128, WasmMsg, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use cw20::Cw20HandleMsg;

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}

pub fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account_addr: &HumanAddr,
    denom: String,
) -> StdResult<Uint128> {
    let balance: BalanceResponse = deps.querier.query(&QueryRequest::Bank(BankQuery::Balance {
        address: HumanAddr::from(account_addr),
        denom,
    }))?;
    Ok(balance.amount.amount)
}

pub fn load_token_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &CanonicalAddr,
    account_addr: &CanonicalAddr,
) -> StdResult<Uint128> {
    // load balance form the token contract
    let res = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: deps.api.human_address(contract_addr)?,
            key: Binary::from(concat(
                &to_length_prefixed(b"balance").to_vec(),
                account_addr.as_slice(),
            )),
        }))
        .unwrap_or_else(|_| to_binary(&Uint128::zero()).unwrap());

    from_binary(&res)
}

pub fn send_tokens<A: Api>(
    api: &A,
    asset_token: &CanonicalAddr,
    recipient: &CanonicalAddr,
    amount: u128,
    action: &str,
) -> HandleResult {
    let contract_human = api.human_address(asset_token)?;
    let recipient_human = api.human_address(recipient)?;

    let log = vec![
        log("action", action),
        log("recipient", recipient_human.as_str()),
        log("amount", &amount.to_string()),
    ];

    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_human,
            msg: to_binary(&Cw20HandleMsg::Transfer {
                recipient: recipient_human,
                amount: Uint128::from(amount),
            })?,
            send: vec![],
        })],
        log,
        data: None,
    })
}
