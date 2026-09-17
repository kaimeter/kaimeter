//! RISC Zero guest: evaluate a rule bundle and commit its public journal.
//!
//! The witness is the bundle's canonical file set, one invocation record and
//! the claimed bundle hash. The guest rebuilds the canonical byte stream of
//! the accepted set, recomputes `h_B` in circuit and requires it to equal the
//! claimed value before any rule is evaluated; only then does the fixed
//! interpreter run the named rule and the guest commit the journal of
//! interpreter contract §7. Any malformed witness or evaluation error aborts
//! the guest and no proof is produced.

use kaimeter_interpreter::bundle::BundleFile;
use kaimeter_interpreter::rule::RuleBundle;
use risc0_zkvm::guest::env;

fn main() {
    let file_count: u32 = env::read();
    let mut files = Vec::with_capacity(file_count as usize);
    for _ in 0..file_count {
        let path: String = env::read();
        let content: Vec<u8> = env::read();
        files.push((path, content));
    }
    let invocation: String = env::read();
    let claimed: Vec<u8> = env::read();

    let files: Vec<BundleFile<'_>> = files
        .iter()
        .map(|(path, content)| BundleFile { path, content })
        .collect();
    let bundle = RuleBundle::from_files(&files).unwrap();
    let computed = bundle.bundle_hash().unwrap();
    assert_eq!(
        computed.as_bytes().as_slice(),
        claimed.as_slice(),
        "bundle hash mismatch"
    );

    let invocation = bundle.parse_invocation(&invocation).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();
    let journal = outcome.journal(computed, invocation.context);
    env::commit_slice(&journal.canonical_bytes());
}
