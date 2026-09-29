//! Fixed rule-bundle interpreter for CBAM embedded-emissions rules.
//!
//! The interpreter executes the CBAM methodology when the methodology
//! travels as data: a `rules.json` schema plus sourced parameter tables,
//! both covered by the bundle hash. It carries the three pieces the guest
//! and the native evaluator must share exactly — the fixed-point arithmetic
//! ([`fixed`]), the canonical serialisation and hashing ([`bundle`]) and the
//! rule evaluator ([`rule`]) — so that a hash or a rounding rule cannot
//! drift between them (whitepaper §5.4; interpreter contract §4).
//!
//! # Purity
//!
//! Evaluation has no clock, no I/O, no randomness and no floating-point
//! arithmetic; every quantity is the bundle's fixed-point scalar.

#![cfg_attr(not(test), no_std)]
// The bundle stays free of floating-point arithmetic (whitepaper §4.4).
#![deny(clippy::float_arithmetic)]

extern crate alloc;

pub mod bundle;
pub mod date;
pub mod error;
pub mod fixed;
pub mod journal;
pub mod rule;
pub mod schema;
pub mod value;
