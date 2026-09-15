//! Cement system boundaries and routes.
//!
//! Clinker, cement and calcined clay, with indirect emissions included.
//! Structure only in bundle 2026.2.0; arrives with the cement sector
//! (whitepaper §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the specific embedded emissions of a cement route.
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
