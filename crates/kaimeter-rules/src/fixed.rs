//! Fixed-point decimal arithmetic at the bundle's scale.
//!
//! Every quantity is the integer ⌊x · 10⁶⌉ held in an `i128`. Multiplication
//! and division round half to even; the scale and the rounding rule are part
//! of the bundle identity, so an independent evaluator can reproduce every
//! intermediate value exactly.

use core::fmt;
use core::str::FromStr;

use crate::error::RuleError;

/// Number of decimal places represented by one [`Fixed`] unit.
pub const SCALE_DIGITS: u32 = 6;

/// Multiplier between a [`Fixed`] value and its scaled integer.
pub const SCALE: i128 = 1_000_000;

/// A decimal quantity stored as an integer multiple of 10⁻⁶.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(i128);

impl Fixed {
    /// Zero.
    pub const ZERO: Self = Self(0);

    /// One.
    pub const ONE: Self = Self(SCALE);

    /// Wraps an integer already scaled by [`SCALE`].
    #[must_use]
    pub const fn from_scaled(scaled: i128) -> Self {
        Self(scaled)
    }

    /// Returns the scaled integer.
    #[must_use]
    pub const fn scaled(self) -> i128 {
        self.0
    }

    /// Returns `true` when the value is exactly zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Adds two values.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Overflow`] when the sum leaves the 128-bit
    /// scaled range.
    pub fn try_add(self, rhs: Self) -> Result<Self, RuleError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(RuleError::Overflow)
    }

    /// Subtracts `rhs` from `self`.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Overflow`] when the difference leaves the
    /// 128-bit scaled range.
    pub fn try_sub(self, rhs: Self) -> Result<Self, RuleError> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(RuleError::Overflow)
    }

    /// Multiplies two values, rounding half to even at the bundle scale.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Overflow`] when the product leaves the 128-bit
    /// scaled range.
    pub fn try_mul(self, rhs: Self) -> Result<Self, RuleError> {
        let product = self.0.checked_mul(rhs.0).ok_or(RuleError::Overflow)?;
        div_round_half_even(product, SCALE).map(Self)
    }

    /// Divides by `rhs`, rounding half to even at the bundle scale.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::DivisionByZero`] when `rhs` is zero and
    /// [`RuleError::Overflow`] when the scaled quotient leaves the 128-bit
    /// range.
    pub fn try_div(self, rhs: Self) -> Result<Self, RuleError> {
        if rhs.0 == 0 {
            return Err(RuleError::DivisionByZero);
        }
        let numerator = self.0.checked_mul(SCALE).ok_or(RuleError::Overflow)?;
        div_round_half_even(numerator, rhs.0).map(Self)
    }

    /// Negates the value.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Overflow`] when the value has no negation in the
    /// 128-bit scaled range.
    pub fn try_neg(self) -> Result<Self, RuleError> {
        self.0.checked_neg().map(Self).ok_or(RuleError::Overflow)
    }
}

/// Divides `numerator` by `denominator`, rounding half to even.
fn div_round_half_even(numerator: i128, denominator: i128) -> Result<i128, RuleError> {
    if denominator == 0 {
        return Err(RuleError::DivisionByZero);
    }
    let negative = (numerator < 0) != (denominator < 0);
    let magnitude = numerator.unsigned_abs();
    let divisor = denominator.unsigned_abs();
    let mut quotient = magnitude / divisor;
    let remainder = magnitude % divisor;
    let twice = remainder * 2;
    if twice > divisor || (twice == divisor && quotient % 2 == 1) {
        quotient += 1;
    }
    let signed = i128::try_from(quotient).map_err(|_error| RuleError::Overflow)?;
    if negative { Ok(-signed) } else { Ok(signed) }
}

/// Streaming parser state for one decimal string.
#[derive(Debug, Default)]
struct DecimalParser {
    integer: i128,
    fraction: i128,
    fraction_digits: u32,
    extra_first: Option<u8>,
    extra_nonzero: bool,
    seen_dot: bool,
    seen_digit: bool,
}

impl DecimalParser {
    fn consume(&mut self, byte: u8) -> Result<(), RuleError> {
        match byte {
            b'.' if !self.seen_dot => {
                self.seen_dot = true;
                return Ok(());
            }
            b'0'..=b'9' => {}
            _ => return Err(RuleError::InvalidDecimal),
        }
        self.seen_digit = true;
        let digit = i128::from(byte - b'0');
        if self.seen_dot {
            self.consume_fraction(byte, digit);
            Ok(())
        } else {
            self.consume_integer(digit)
        }
    }

    fn consume_integer(&mut self, digit: i128) -> Result<(), RuleError> {
        self.integer = self
            .integer
            .checked_mul(10)
            .and_then(|value| value.checked_add(digit))
            .ok_or(RuleError::Overflow)?;
        Ok(())
    }

    fn consume_fraction(&mut self, byte: u8, digit: i128) {
        if self.fraction_digits < SCALE_DIGITS {
            self.fraction = self.fraction * 10 + digit;
            self.fraction_digits += 1;
        } else if self.extra_first.is_none() {
            self.extra_first = Some(byte);
        } else if byte != b'0' {
            self.extra_nonzero = true;
        }
    }

    fn finish(self, negative: bool) -> Result<Fixed, RuleError> {
        if !self.seen_digit {
            return Err(RuleError::InvalidDecimal);
        }
        let scaled_fraction = self.fraction * 10_i128.pow(SCALE_DIGITS - self.fraction_digits);
        let round_up = match self.extra_first {
            Some(first) => {
                first > b'5' || (first == b'5' && (self.extra_nonzero || scaled_fraction % 2 == 1))
            }
            None => false,
        };
        let magnitude = self
            .integer
            .checked_mul(SCALE)
            .and_then(|value| value.checked_add(scaled_fraction))
            .and_then(|value| value.checked_add(i128::from(u8::from(round_up))))
            .ok_or(RuleError::Overflow)?;
        if negative {
            magnitude
                .checked_neg()
                .ok_or(RuleError::Overflow)
                .map(Fixed)
        } else {
            Ok(Fixed(magnitude))
        }
    }
}

impl FromStr for Fixed {
    type Err = RuleError;

    /// Parses a plain decimal string, rounding digits beyond the bundle
    /// scale half to even.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::InvalidDecimal`] for malformed input and
    /// [`RuleError::Overflow`] when the integer part leaves the scaled range.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes = text.as_bytes();
        let (negative, digits) = match bytes.split_first() {
            Some((b'-', rest)) => (true, rest),
            Some((b'+', rest)) => (false, rest),
            _ => (false, bytes),
        };
        if digits.is_empty() {
            return Err(RuleError::InvalidDecimal);
        }
        let mut parser = DecimalParser::default();
        for &byte in digits {
            parser.consume(byte)?;
        }
        parser.finish(negative)
    }
}

impl fmt::Display for Fixed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let magnitude = self.0.unsigned_abs();
        if self.0 < 0 {
            formatter.write_str("-")?;
        }
        write!(
            formatter,
            "{}.{:06}",
            magnitude / 1_000_000,
            magnitude % 1_000_000
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(text: &str) -> Fixed {
        text.parse().unwrap()
    }

    #[test]
    fn parses_integer_decimals() {
        assert_eq!(fixed("0"), Fixed::ZERO);
        assert_eq!(fixed("1"), Fixed::ONE);
        assert_eq!(fixed("-42").scaled(), -42_000_000);
        assert_eq!(fixed("+7").scaled(), 7_000_000);
        assert_eq!(fixed("100000").scaled(), 100_000_000_000);
    }

    #[test]
    fn parses_fractional_decimals() {
        assert_eq!(fixed("0.143").scaled(), 143_000);
        assert_eq!(fixed("3.575").scaled(), 3_575_000);
        assert_eq!(fixed("-2.5").scaled(), -2_500_000);
        assert_eq!(fixed(".5").scaled(), 500_000);
        assert_eq!(fixed("2.").scaled(), 2_000_000);
        assert_eq!(fixed("0.000001").scaled(), 1);
    }

    #[test]
    fn rejects_malformed_decimals() {
        for text in ["", "-", "+", ".", "1.2.3", "1e5", "abc", "1,5", " 1", "1 "] {
            assert_eq!(
                text.parse::<Fixed>(),
                Err(RuleError::InvalidDecimal),
                "{text}"
            );
        }
    }

    #[test]
    fn rounds_parsed_digits_beyond_scale_half_even() {
        assert_eq!(fixed("0.0000004"), Fixed::ZERO);
        assert_eq!(fixed("0.0000005"), Fixed::ZERO);
        assert_eq!(fixed("0.0000015").scaled(), 2);
        assert_eq!(fixed("0.0000025").scaled(), 2);
        assert_eq!(fixed("0.00000150001").scaled(), 2);
        assert_eq!(fixed("0.00000050001").scaled(), 1);
        assert_eq!(fixed("-0.0000005"), Fixed::ZERO);
    }

    #[test]
    fn reports_parse_overflow() {
        assert_eq!(
            "170141183460469231731687303715884105728".parse::<Fixed>(),
            Err(RuleError::Overflow)
        );
    }

    #[test]
    fn adds_and_subtracts_with_error_reporting() {
        assert_eq!(fixed("0.1").try_add(fixed("0.2")).unwrap(), fixed("0.3"));
        assert_eq!(fixed("0.3").try_sub(fixed("0.1")).unwrap(), fixed("0.2"));
        assert_eq!(
            Fixed::from_scaled(i128::MAX).try_add(Fixed::ONE),
            Err(RuleError::Overflow)
        );
        assert_eq!(
            Fixed::from_scaled(i128::MIN).try_sub(Fixed::ONE),
            Err(RuleError::Overflow)
        );
    }

    #[test]
    fn multiplies_with_half_even_rounding() {
        assert_eq!(
            fixed("0.25").try_mul(fixed("0.143")).unwrap(),
            fixed("0.035750")
        );
        assert_eq!(fixed("2").try_mul(fixed("0.5")).unwrap(), Fixed::ONE);
        assert_eq!(fixed("1.5").try_mul(fixed("0.000001")).unwrap().scaled(), 2);
        assert_eq!(fixed("2.5").try_mul(fixed("0.000001")).unwrap().scaled(), 2);
        assert_eq!(
            Fixed::from_scaled(i128::MAX).try_mul(Fixed::from_scaled(2)),
            Err(RuleError::Overflow)
        );
    }

    #[test]
    fn divides_with_half_even_rounding() {
        assert_eq!(fixed("1").try_div(fixed("8")).unwrap(), fixed("0.125"));
        assert_eq!(fixed("1").try_div(fixed("3")).unwrap().scaled(), 333_333);
        assert_eq!(fixed("2").try_div(fixed("3")).unwrap().scaled(), 666_667);
        assert_eq!(fixed("0.000001").try_div(fixed("2")).unwrap(), Fixed::ZERO);
        assert_eq!(fixed("0.000003").try_div(fixed("2")).unwrap().scaled(), 2);
        assert_eq!(
            fixed("1").try_div(Fixed::ZERO),
            Err(RuleError::DivisionByZero)
        );
        assert_eq!(
            Fixed::from_scaled(i128::MAX).try_div(Fixed::ONE),
            Err(RuleError::Overflow)
        );
    }

    #[test]
    fn negates_with_error_reporting() {
        assert_eq!(fixed("0.5").try_neg().unwrap(), fixed("-0.5"));
        assert_eq!(Fixed::ZERO.try_neg().unwrap(), Fixed::ZERO);
        assert_eq!(
            Fixed::from_scaled(i128::MIN).try_neg(),
            Err(RuleError::Overflow)
        );
    }

    #[test]
    fn orders_by_value() {
        assert!(fixed("0.143") < fixed("0.143001"));
        assert!(fixed("-1") < Fixed::ZERO);
        assert_eq!(fixed("1.5"), Fixed::from_scaled(1_500_000));
    }

    #[test]
    fn displays_at_the_bundle_scale() {
        assert_eq!(fixed("1.835038").to_string(), "1.835038");
        assert_eq!(fixed("2").to_string(), "2.000000");
        assert_eq!(fixed("-0.5").to_string(), "-0.500000");
        assert_eq!(Fixed::ZERO.to_string(), "0.000000");
    }
}
