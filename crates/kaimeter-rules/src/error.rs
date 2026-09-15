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
}

impl fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Overflow => "fixed-point overflow",
            Self::DivisionByZero => "division by zero",
            Self::InvalidDecimal => "invalid decimal string",
        };
        formatter.write_str(message)
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
    }
}
