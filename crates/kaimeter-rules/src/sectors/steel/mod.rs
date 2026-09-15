//! Steel sector rules.
//!
//! Routes: blast furnace and basic oxygen furnace (BF-BOF), direct-reduced
//! iron, scrap electric arc furnace (EAF) and rolling/finishing, with pig
//! iron, DRI and crude steel as precursors. Structure only in bundle
//! 2026.2.0, arriving with v0.3 (whitepaper §10).

pub mod boundaries;
pub mod scrap;
