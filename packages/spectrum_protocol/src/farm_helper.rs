use std::convert::TryFrom;
use cosmwasm_std::{CosmosMsg, Decimal, Fraction, QuerierWrapper, StdError, StdResult, Uint128};
use terraswap::asset::{Asset, AssetInfo};
use terraswap::pair::PoolResponse;
use uint::construct_uint;
use astroport::factory::PairType;
use astroport::pair::{PoolResponse as PoolResponseAstroport};
use astroport::asset::{Asset as AssetAstroport};

construct_uint! {
    pub struct U256(4);
}

pub(crate) fn compute_swap_amount(
    amount_a: Uint128,
    amount_b: Uint128,
    pool_a: Uint128,
    pool_b: Uint128,
) -> Uint128 {
    let amount_a = U256::from(amount_a.u128());
    let amount_b = U256::from(amount_b.u128());
    let pool_a = U256::from(pool_a.u128());
    let pool_b = U256::from(pool_b.u128());

    let pool_ax = amount_a + pool_a;
    let pool_bx = amount_b + pool_b;
    let area_ax = pool_ax * pool_b;
    let area_bx = pool_bx * pool_a;

    let a = U256::from(9) * area_ax + U256::from(3988000) * area_bx;
    let b = U256::from(3) * area_ax + area_ax.integer_sqrt() * a.integer_sqrt();
    let result = b / U256::from(2000) / pool_bx - pool_a;

    result.as_u128().into()
}

pub fn get_swap_amount_astroport(
    pool: &PoolResponseAstroport,
    asset: &AssetAstroport,
    pair_type: Option<PairType>,
) -> Uint128 {
    if let Some(PairType::Stable {}) = pair_type {
        asset.amount.multiply_ratio(10000u128, 19995u128)
    } else if pool.assets[0].info == asset.info {
        compute_swap_amount(asset.amount, Uint128::zero(), pool.assets[0].amount, pool.assets[1].amount)
    } else {
        compute_swap_amount(asset.amount, Uint128::zero(), pool.assets[1].amount, pool.assets[0].amount)
    }
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
