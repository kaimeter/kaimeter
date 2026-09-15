//! Fertilisers system boundaries and routes.
//!
//! Ammonia, nitric acid, urea and mixed fertilisers, with indirect emissions
//! included and ammonia and nitric acid as precursors. Structure only in
//! bundle 2026.2.0; arrives with the fertilisers sector (whitepaper §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the specific embedded emissions of a fertiliser route.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn see() -> Result<Fixed, RuleError> {
    Err(RuleError::NotYetImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_not_implemented() {
        assert_eq!(see(), Err(RuleError::NotYetImplemented));
    }
}
