use std::convert::TryFrom;
use cosmwasm_std::{QuerierWrapper, StdError, StdResult, Uint128};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::PoolResponse;
use astroport::asset::{Asset as AstroportAsset, AssetInfo as AstroportAssetInfo};
use astroport::pair::{PoolResponse as AstroportPoolResponse};

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

pub fn deduct_tax(querier: &QuerierWrapper, amount: Uint128, base_denom: String) -> StdResult<Uint128> {
    let asset = Asset {
        info: AssetInfo::NativeToken {
            denom: base_denom,
        },
        amount,
    };
    asset.deduct_tax(querier).map(|it| it.amount)
}

pub fn compute_provide_after_swap(
    pool: &PoolResponse,
    offer: &Asset,
    return_amt: Uint128,
    ask_reinvest_amt: Uint128,
) -> StdResult<Uint128> {
    let (offer_amount, ask_amount) = if pool.assets[0].info == offer.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };

    let offer_amount = offer_amount + offer.amount;
    let ask_amount = ask_amount.checked_sub(return_amt)?;

    Ok(ask_reinvest_amt.multiply_ratio(offer_amount, ask_amount))
}

pub fn compute_provide_after_swap_astroport(
    pool: &AstroportPoolResponse,
    offer: &AstroportAsset,
    return_amt: Uint128,
    ask_reinvest_amt: Uint128,
) -> StdResult<Uint128> {
    let (offer_amount, ask_amount) = if pool.assets[0].info == offer.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };

    let offer_amount = offer_amount + offer.amount;
    let ask_amount = ask_amount.checked_sub(return_amt)?;

    Ok(ask_reinvest_amt.multiply_ratio(offer_amount, ask_amount))
}

