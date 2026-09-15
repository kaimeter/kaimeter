//! Steel system boundaries and routes.
//!
//! BF-BOF, DRI, scrap-EAF and rolling/finishing, with pig iron, DRI and
//! crude steel as intermediate precursors. Structure only in bundle
//! 2026.2.0; arrives with v0.3 (whitepaper §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the specific embedded emissions of a steel route.
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
