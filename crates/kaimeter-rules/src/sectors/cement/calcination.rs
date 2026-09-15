//! Calcination CO2.
//!
//! Process emissions from the calcination of limestone, attributed with the
//! clinker and cement routes. Structure only in bundle 2026.2.0; arrives with
//! the cement sector (whitepaper §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the calcination CO2 of the reporting period.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn calcination_co2() -> Result<Fixed, RuleError> {
    Err(RuleError::NotYetImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_not_implemented() {
        assert_eq!(calcination_co2(), Err(RuleError::NotYetImplemented));
    }
}
