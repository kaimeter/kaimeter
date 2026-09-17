//! RISC Zero guest for the fixed-interpreter evaluation of a rule bundle.
//!
//! Scaffold: the witness is echoed to the journal so the host harness can
//! exercise the build and execution pipeline. The bundle evaluation and the
//! in-circuit bundle-hash check arrive in the following units (interpreter
//! contract §4-§7).

use risc0_zkvm::guest::env;

fn main() {
    let witness: Vec<u8> = env::read();
    env::commit(&witness);
}
