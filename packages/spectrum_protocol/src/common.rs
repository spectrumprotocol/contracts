use std::convert::TryFrom;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Order, CanonicalAddr, Uint128, StdResult, StdError};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderBy {
    Asc,
    Desc,
}

impl From<OrderBy> for Order {
    fn from(val: OrderBy) -> Order {
        if val == OrderBy::Asc {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}

// this will set the first key after the provided key, by appending a 1 byte
pub fn calc_range_start(start_after: Option<u64>) -> Option<Vec<u8>> {
    start_after.map(|id| {
        let mut v = id.to_be_bytes().to_vec();
        v.push(1);
        v
    })
}

// this will set the first key after the provided key, by appending a 1 byte
pub fn calc_range_end(start_after: Option<u64>) -> Option<Vec<u8>> {
    start_after.map(|id| id.to_be_bytes().to_vec())
}

// this will set the first key after the provided key, by appending a 1 byte
pub fn calc_range_start_addr(start_after: Option<CanonicalAddr>) -> Option<Vec<u8>> {
    start_after.map(|addr| {
        let mut v = addr.as_slice().to_vec();
        v.push(1);
        v
    })
}

// this will set the first key after the provided key, by appending a 1 byte
pub fn calc_range_end_addr(start_after: Option<CanonicalAddr>) -> Option<Vec<u8>> {
    start_after.map(|addr| addr.as_slice().to_vec())
}

pub fn compute_deposit_time(
    last_deposit_amount: Uint128,
    new_deposit_amount: Uint128,
    last_deposit_time: u64,
    new_deposit_time: u64,
) -> StdResult<u64> {
    let last_weight = last_deposit_amount.u128() * (last_deposit_time as u128);
    let new_weight = new_deposit_amount.u128() * (new_deposit_time as u128);
    let weight_avg = (last_weight + new_weight) / (last_deposit_amount.u128() + new_deposit_amount.u128());
    u64::try_from(weight_avg).map_err(|_| StdError::generic_err("Overflow in compute_deposit_time"))
}
