//! Indirect emissions from electricity.
//!
//! Grid and contract-specific emission factors for the sectors where
//! indirect emissions count (cement and fertilisers, Annex II). Structure
//! only in bundle 2026.2.0, arriving with those sectors (whitepaper §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the emission factor of the electricity consumed in production.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn electricity_emission_factor() -> Result<Fixed, RuleError> {
    Err(RuleError::NotYetImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_not_implemented() {
        assert_eq!(
            electricity_emission_factor(),
            Err(RuleError::NotYetImplemented)
        );
    }
}
