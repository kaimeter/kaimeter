//! Cross-sector rule logic shared by every sector.
//!
//! Sector modules contribute only what is specific to them; period,
//! precursors, defaults, mark-ups and scope live here (whitepaper §4.2).

use alloc::string::String;

use crate::error::RuleError;

pub mod defaults;
pub mod markups;
pub mod period;
pub mod precursors;
pub mod scope;

/// Normalizes a CN code to bare digits.
///
/// # Errors
///
/// Returns [`RuleError::InvalidCnCode`] unless the text is four, six or
/// eight digits, ignoring ASCII whitespace.
pub(crate) fn normalize_cn_code(text: &str) -> Result<String, RuleError> {
    let mut normalized = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_whitespace() {
            continue;
        }
        if !character.is_ascii_digit() {
            return Err(RuleError::InvalidCnCode);
        }
        normalized.push(character);
    }
    if !matches!(normalized.len(), 4 | 6 | 8) {
        return Err(RuleError::InvalidCnCode);
    }
    Ok(normalized)
}

/// A CBAM sector covered by this bundle.
///
/// Electricity is outside the bundle's scope: it is a grid-operator regime
/// with its own default-factor logic (whitepaper §1.2). Downstream goods are
/// not a sector; their emissions follow the precursors they contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Sector {
    /// Cement.
    Cement,
    /// Iron and steel.
    IronAndSteel,
    /// Aluminium.
    Aluminium,
    /// Fertilisers.
    Fertilisers,
    /// Hydrogen.
    Hydrogen,
}
