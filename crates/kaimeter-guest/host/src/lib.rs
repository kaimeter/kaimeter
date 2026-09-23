//! Host harness for the kaimeter RISC Zero guest.
//!
//! `risc0-build` compiles the guest at build time and embeds its ELF and
//! image ID; the image ID is the interpreter identity a verifier pins
//! (interpreter contract §2, §5).

include!(concat!(env!("OUT_DIR"), "/methods.rs"));
