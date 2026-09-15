//! Pre-consumer scrap treatment.
//!
//! The proposed downstream extension would bring pre-consumer scrap into
//! scope from 2028, following whatever treatment the adopted text specifies
//! (whitepaper §2.2, §6.4). Structure only in bundle 2026.2.0.

use crate::error::RuleError;
use crate::fixed::Fixed;

/// Returns the emissions attributed to consumed pre-consumer scrap.
///
/// # Errors
///
/// Always [`RuleError::NotYetImplemented`] in bundle 2026.2.0.
pub fn pre_consumer_scrap() -> Result<Fixed, RuleError> {
    Err(RuleError::NotYetImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_not_implemented() {
        assert_eq!(pre_consumer_scrap(), Err(RuleError::NotYetImplemented));
    }
}
