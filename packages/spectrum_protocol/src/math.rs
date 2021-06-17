use cosmwasm_std::{Decimal, Uint128};
use std::mem::transmute;
use std::ops;

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UDec128(u128);

impl UDec128 {
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl From<Decimal> for UDec128 {
    fn from(val: Decimal) -> Self {
        unsafe { transmute(val) }
    }
}

impl Into<Decimal> for UDec128 {
    fn into(self) -> Decimal {
        unsafe { transmute(self) }
    }
}

impl ops::Add for UDec128 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        UDec128(self.0 + other.0)
    }
}

impl ops::Sub for UDec128 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        UDec128(self.0 - other.0)
    }
}

// NOTE: this is already supported in cosmwasm 0.14.x
impl ops::Sub<UDec128> for Decimal {
    type Output = Self;

    fn sub(self, other: UDec128) -> Self {
        let me: UDec128 = self.into();
        (me - other).into()
    }
}

impl ops::Mul<Uint128> for UDec128 {
    type Output = Self;

    fn mul(self, rhs: Uint128) -> Self::Output {
        UDec128(self.0 * rhs.0)
    }
}

impl ops::Div<Uint128> for UDec128 {
    type Output = Self;

    fn div(self, rhs: Uint128) -> Self::Output {
        UDec128(self.0 / rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::math::UDec128;
    use cosmwasm_std::{Decimal, Uint128};

    #[test]
    fn test_subtract() {
        let left: UDec128 =
            Decimal::from_ratio(3179855158925365484u128, 1000000000000000000u128).into();
        let right: UDec128 =
            Decimal::from_ratio(3179855154879542739u128, 1000000000000000000u128).into();
        assert_eq!(
            Decimal::from_ratio(4045822745u128, 1000000000000000000u128),
            (left - right).into(),
        );
    }

    #[test]
    fn test_mult() {
        let left: UDec128 = Decimal::permille(1111111111).into(); //1_111_111.111
        let right = Uint128(1111111111); //1_111_111_111
        assert_eq!(
            Decimal::from_ratio(1_234_567_900_987_654_321u128, 1_000u128),
            (left * right).into(),
        );
    }
}
