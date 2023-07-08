use cosmwasm_std::{Decimal, Uint128, Fraction};
use std::ops;
use std::fmt::{self, Write};
use std::str::FromStr;

const DECIMAL_FRACTIONAL: u128 = 1_000_000_000_000_000_000; // 1*10**18

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UDec128(u128);

impl UDec128 {
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn multiply_ratio<A: Into<u128>, B: Into<u128>>(&self, num: A, denom: B) -> UDec128 {
        let value = Uint128::from(self.0);
        let result = value.multiply_ratio(num, denom);
        UDec128(result.u128())
    }
}

impl From<Decimal> for UDec128 {
    fn from(val: Decimal) -> Self {
        UDec128(val.numerator().u128())
    }
}

impl From<UDec128> for Decimal {
    fn from(val: UDec128) -> Decimal {
        Decimal::from_str(&val.to_string()).unwrap()
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

impl ops::Mul<Uint128> for UDec128 {
    type Output = Self;

    fn mul(self, rhs: Uint128) -> Self::Output {
        UDec128(self.0 * rhs.u128())
    }
}

impl ops::Div<Uint128> for UDec128 {
    type Output = Self;

    fn div(self, rhs: Uint128) -> Self::Output {
        UDec128(self.0 / rhs.u128())
    }
}

impl fmt::Display for UDec128 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let whole = (self.0) / DECIMAL_FRACTIONAL;
        let fractional = (self.0) % DECIMAL_FRACTIONAL;

        if fractional == 0 {
            write!(f, "{}", whole)
        } else {
            let fractional_string = format!("{:018}", fractional);
            f.write_str(&whole.to_string())?;
            f.write_char('.')?;
            f.write_str(fractional_string.trim_end_matches('0'))?;
            Ok(())
        }
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
        let right = Uint128::new(1111111111); //1_111_111_111
        assert_eq!(
            Decimal::from_ratio(1_234_567_900_987_654_321u128, 1_000u128),
            (left * right).into(),
        );
    }

    #[test]
    fn test_convert() {
        let dec = Decimal::permille(123456789); //123_456_789
        let udec: UDec128 = dec.into();
        let dec2:Decimal = udec.into();
        assert_eq!(dec, dec2);
    }

    #[test]
    fn test_overflow() {
        let value: UDec128 = Decimal::percent(10000u64).into();
        assert_eq!(
            value.multiply_ratio(value.0, value.0),
            value
        );
    }
}
