//! Errors produced while applying a rule bundle.

use core::fmt;

/// Errors produced while applying a rule bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RuleError {
    /// A fixed-point operation left the 128-bit scaled range.
    Overflow,
    /// A fixed-point division had a zero divisor.
    DivisionByZero,
    /// A decimal string was not a plain decimal number.
    InvalidDecimal,
    /// A date was not a real calendar date or not `YYYY-MM-DD`.
    InvalidDate,
    /// A year is outside the bundle's reporting periods.
    UnsupportedPeriod {
        /// The rejected calendar year.
        year: u16,
    },
}

impl fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => formatter.write_str("fixed-point overflow"),
            Self::DivisionByZero => formatter.write_str("division by zero"),
            Self::InvalidDecimal => formatter.write_str("invalid decimal string"),
            Self::InvalidDate => formatter.write_str("invalid calendar date"),
            Self::UnsupportedPeriod { year } => {
                write!(formatter, "reporting period {year} is outside the bundle")
            }
        }
    }
}

impl core::error::Error for RuleError {}

#[cfg(test)]
mod tests {
    use super::RuleError;

    #[test]
    fn displays_each_variant() {
        assert_eq!(RuleError::Overflow.to_string(), "fixed-point overflow");
        assert_eq!(RuleError::DivisionByZero.to_string(), "division by zero");
        assert_eq!(
            RuleError::InvalidDecimal.to_string(),
            "invalid decimal string"
        );
        assert_eq!(RuleError::InvalidDate.to_string(), "invalid calendar date");
        assert_eq!(
            RuleError::UnsupportedPeriod { year: 2025 }.to_string(),
            "reporting period 2025 is outside the bundle"
        );
    }
}
