//! Scaffold check for the guest pipeline: the guest echoes its witness.

use kaimeter_guest_host::KAIMETER_GUEST_ELF;
use risc0_zkvm::{default_executor, ExecutorEnv};

#[test]
fn guest_echoes_the_witness() {
    let witness = b"kaimeter".to_vec();
    let env = ExecutorEnv::builder()
        .write(&witness)
        .unwrap()
        .build()
        .unwrap();
    let session = default_executor().execute(env, KAIMETER_GUEST_ELF).unwrap();
    let output: Vec<u8> = session.journal.decode().unwrap();
    assert_eq!(output, witness);
}
