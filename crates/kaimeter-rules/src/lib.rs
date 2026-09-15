//! Deterministic, hash-identified CBAM embedded-emissions rule bundle.
//!
//! The crate encodes the CBAM methodology of the working paper *Prove,
//! Don't Disclose* as pure rules: no I/O, no floating-point arithmetic and no
//! dependency on the surrounding application, so the same code can run
//! natively and, from v0.2, inside a fixed rule-bundle interpreter that takes
//! the bundle as witness.
//!
//! # Arithmetic
//!
//! Every quantity is an integer scaled by [`fixed::SCALE`], held in an
//! `i128`. Rounding mirrors the Regulation's stated precision, with
//! round-half-to-even where the Regulation is silent.

#![cfg_attr(not(test), no_std)]
// The bundle stays free of floating-point arithmetic (whitepaper §4.4).
#![deny(clippy::float_arithmetic)]

extern crate alloc;

pub mod bundle;
pub mod common;
pub mod error;
pub mod fixed;
