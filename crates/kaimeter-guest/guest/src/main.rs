//! RISC Zero guest: evaluate a rule bundle and commit its public journal.
//!
//! The witness carries the pinned bundle root, one inclusion proof per file
//! the evaluation reads, and the invocation record. The guest verifies every
//! opening against the root before evaluating, so no rule or parameter byte
//! enters the computation unauthenticated, and only the opened files are
//! read or hashed in circuit (interpreter contract §4, §7). A malformed
//! witness or evaluation error aborts the guest and no proof is produced.

use kaimeter_interpreter::bundle::{BundleFile, BundleHash, Opening, verify_opening};
use kaimeter_interpreter::rule::RuleBundle;
use risc0_zkvm::guest::env;

fn main() {
    let root: [u8; 32] = env::read();
    let root = BundleHash::from_bytes(root);

    let opening_count: u32 = env::read();
    let mut openings = Vec::with_capacity(opening_count as usize);
    for _ in 0..opening_count {
        let path: String = env::read();
        let content: Vec<u8> = env::read();
        let index: u64 = env::read();
        let size: u64 = env::read();
        let proof_len: u32 = env::read();
        let mut proof = Vec::with_capacity(proof_len as usize);
        for _ in 0..proof_len {
            let sibling: [u8; 32] = env::read();
            proof.push(BundleHash::from_bytes(sibling));
        }
        let opening = Opening {
            path,
            content,
            index,
            size,
            proof,
        };
        verify_opening(root, &opening).unwrap();
        openings.push(opening);
    }

    let invocation: String = env::read();

    let files: Vec<BundleFile<'_>> = openings
        .iter()
        .map(|opening| BundleFile {
            path: &opening.path,
            content: &opening.content,
        })
        .collect();
    let bundle = RuleBundle::from_files(&files).unwrap();
    let invocation = bundle.parse_invocation(&invocation).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();
    let journal = outcome.journal(root, invocation.context);
    env::commit_slice(&journal.canonical_bytes());
}
