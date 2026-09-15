//! Cross-sector rule logic shared by every sector.
//!
//! Sector modules contribute only what is specific to them; period,
//! precursors, defaults, mark-ups and scope live here (whitepaper §4.2).

pub mod defaults;
pub mod markups;
pub mod period;
pub mod precursors;

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
