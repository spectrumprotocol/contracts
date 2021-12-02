use std::convert::TryFrom;
use cosmwasm_std::{Deps, StdError, StdResult, Uint128};
use terraswap::asset::{Asset, AssetInfo};

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

pub fn deduct_tax(deps: Deps, amount: Uint128, base_denom: String) -> Uint128 {
    let asset = Asset {
        info: AssetInfo::NativeToken {
            denom: base_denom.clone(),
        },
        amount,
    };
    let after_tax = Asset {
        info: AssetInfo::NativeToken { denom: base_denom },
        amount: asset.deduct_tax(&deps.querier).unwrap().amount,
    };
    after_tax.amount
}
