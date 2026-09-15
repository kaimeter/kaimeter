//! Nitrous oxide from nitric acid production.
//!
//! N2O emissions from nitric acid, using the abatement and formation factors
//! of the applicable annexes. Structure only in bundle 2026.2.0; arrives with
//! the fertilisers sector (whitepaper §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the N2O emissions of nitric acid production, t `CO2e`.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn n2o_emissions() -> Result<Fixed, RuleError> {
    Err(RuleError::NotYetImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_not_implemented() {
        assert_eq!(n2o_emissions(), Err(RuleError::NotYetImplemented));
    }
}
