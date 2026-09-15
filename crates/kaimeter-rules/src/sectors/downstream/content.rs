//! Precursor-content model for downstream goods.
//!
//! Embedded emissions from the CBAM material content of a downstream good
//! multiplied by the precursor's specific embedded emissions, plus the
//! producer's own processing emissions (whitepaper §6.4). Structure only in
//! bundle 2026.2.0; arrives with the adopted downstream scope.

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the specific embedded emissions of a downstream good.
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
