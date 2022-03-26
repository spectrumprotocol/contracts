use cosmwasm_std::{QuerierWrapper, QueryRequest, StdResult, to_binary, Uint128, WasmQuery};
use spectrum_protocol::gov::{BalanceResponse, QueryMsg};
use crate::state::StakeCredit;

pub(crate) fn compute_credit(balance: BalanceResponse, credits: &[StakeCredit]) -> Uint128 {
    let mut credit = Uint128::zero();
    for pool in balance.pools {
        let stake_credit = credits.iter()
            .find(|it| it.days <= pool.days);
        if let Some(stake_credit) = stake_credit {
            credit += pool.balance * stake_credit.credit;
        }
    }
    credit
}

pub fn query_credit(querier: &QuerierWrapper, contract_addr: String, address: String, credits: &[StakeCredit]) -> StdResult<Uint128> {
    let balance: BalanceResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr,
        msg: to_binary(&QueryMsg::balance {
            address
        })?,
    }))?;
    Ok(compute_credit(balance, credits))
}
