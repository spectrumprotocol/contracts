use crate::concat;
use cosmwasm_bignumber::Uint256;
use cosmwasm_std::{Addr, Binary, Deps, QueryRequest, StdResult, WasmQuery};
use cosmwasm_storage::to_length_prefixed;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BorrowerInfo {
    pub balance: Uint256,
    // we do not need those fields, removing it will save some space in
    // compiled wasm file
    //
    // pub borrower: String,
    // pub spendable: Uint256,
}

pub fn get_basset_in_custody(
    deps: Deps,
    custody_basset_addr: &Addr,
    account_addr: &Addr,
) -> StdResult<Uint256> {
    let borrower_info: StdResult<BorrowerInfo> =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: custody_basset_addr.to_string(),
            key: Binary::from(concat(
                //Anchor use cosmwasm_storage::bucket which add length prefix
                &to_length_prefixed(b"borrower").to_vec(),
                (deps.api.addr_canonicalize(account_addr.as_str())?).as_slice(),
            )),
        }));

    let balance = borrower_info.map(|bi| bi.balance).unwrap_or_default();
    Ok(balance)
}
