//! True-origin and anti-circumvention rules.
//!
//! Where goods are found to have undergone only slight modification in an
//! intermediate country, the default of the true origin applies; a proof
//! bound to an installation commitment is itself evidence of origin at the
//! material level (whitepaper §6.4). Structure only in bundle 2026.2.0.

use crate::error::RuleError;

/// Returns the true origin of a downstream good.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn true_origin() -> Result<&'static str, RuleError> {
    Err(RuleError::NotYetImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_not_implemented() {
        assert_eq!(true_origin(), Err(RuleError::NotYetImplemented));
    }
}
