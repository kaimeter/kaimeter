//! Hydrogen system boundaries and routes.
//!
//! Steam methane reforming and electrolysis routes. Structure only in bundle
//! 2026.2.0; arrives with the hydrogen sector (whitepaper §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the specific embedded emissions of a hydrogen route.
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
