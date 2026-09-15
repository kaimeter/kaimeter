//! Sector-specific rule modules.
//!
//! Each sector contributes only what is specific to it; shared logic lives in
//! [`crate::common`] (whitepaper §4.2).

pub mod aluminium;
pub mod cement;
pub mod downstream;
pub mod fertilisers;
pub mod hydrogen;
pub mod steel;
