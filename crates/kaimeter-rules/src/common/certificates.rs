//! Certificate adjustments.
//!
//! The Article 31 free-allocation adjustment and the Article 9 deduction for
//! a carbon price effectively paid in the country of origin. Structure only
//! in bundle 2026.2.0; the Article 9 implementing act was still pending at
//! the time of writing (whitepaper §2.3, §10).

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the Article 31 free-allocation adjustment.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn free_allocation_adjustment() -> Result<Fixed, RuleError> {
    Err(RuleError::NotYetImplemented)
}

/// Returns the Article 9 carbon-price deduction.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn carbon_price_deduction() -> Result<Fixed, RuleError> {
    Err(RuleError::NotYetImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjustments_are_not_implemented() {
        assert_eq!(
            free_allocation_adjustment(),
            Err(RuleError::NotYetImplemented)
        );
        assert_eq!(carbon_price_deduction(), Err(RuleError::NotYetImplemented));
    }
}
