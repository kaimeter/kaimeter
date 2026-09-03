// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! The `core` domain: frozen CBAM types, unit normalization, phased mark-ups,
//! in-memory lookups over seeded reference data, and dossier completeness.

pub mod errors;
pub mod lookup;
pub mod markups;
pub mod types;
pub mod units;

pub use errors::DomainError;
