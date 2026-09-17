//! Guest pipeline checks for the in-circuit bundle-hash verification.
//!
//! The guest must accept a witness only when the claimed hash equals the
//! `h_B` recomputed from the file set, and commit exactly the journal the
//! interpreter computes natively for the same witness.

use kaimeter_guest_host::KAIMETER_GUEST_ELF;
use kaimeter_interpreter::bundle::BundleFile;
use kaimeter_interpreter::rule::RuleBundle;
use risc0_zkvm::{default_executor, ExecutorEnv};

/// A self-contained one-file bundle for the pipeline test.
const RULES: &[u8] = br#"{
  "schema": "kaimeter-rules-v1",
  "rules": {
    "test.double": {
      "legal": "test",
      "source": "https://example.test",
      "since": "2026.3.0",
      "inputs": {"x": {"type": "scalar"}},
      "outputs": {
        "y": {"op": "mul", "args": [
          {"op": "input", "name": "x"},
          {"op": "const", "scalar": "2"}
        ]}
      },
      "proves": "y"
    }
  }
}"#;

/// One invocation of `test.double`.
const INVOCATION: &str = r#"{
  "rule": "test.double",
  "context": {"sector": "aluminium", "route": "test", "cnCode": "7601", "period": "2026"},
  "inputs": {"x": "21"}
}"#;

/// The canonical file set of the test bundle.
fn bundle() -> RuleBundle<'static> {
    RuleBundle::from_files(&[BundleFile {
        path: "rules.json",
        content: RULES,
    }])
    .unwrap()
}

/// Executes the guest with the given claimed hash and returns its journal.
fn execute(claimed: Vec<u8>) -> Result<Vec<u8>, String> {
    let file_count: u32 = 1;
    let path = String::from("rules.json");
    let content = RULES.to_vec();
    let invocation = String::from(INVOCATION);
    let env = ExecutorEnv::builder()
        .write(&file_count)
        .unwrap()
        .write(&path)
        .unwrap()
        .write(&content)
        .unwrap()
        .write(&invocation)
        .unwrap()
        .write(&claimed)
        .unwrap()
        .build()
        .unwrap();
    match default_executor().execute(env, KAIMETER_GUEST_ELF) {
        Ok(session) => Ok(session.journal.bytes),
        Err(error) => Err(error.to_string()),
    }
}

#[test]
fn guest_verifies_the_bundle_hash_and_commits_the_journal() {
    let bundle = bundle();
    let hash = bundle.bundle_hash().unwrap();
    let invocation = bundle.parse_invocation(INVOCATION).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();
    let expected = outcome
        .journal(hash, invocation.context.clone())
        .canonical_bytes();
    assert_eq!(outcome.output.to_string(), "42.000000");

    let committed = execute(hash.as_bytes().to_vec()).unwrap();
    assert_eq!(committed, expected);
}

#[test]
fn guest_rejects_a_bundle_hash_mismatch() {
    let mut claimed = bundle().bundle_hash().unwrap().as_bytes().to_vec();
    claimed[0] ^= 0xff;

    match execute(claimed) {
        Ok(committed) => assert!(
            committed.is_empty(),
            "an aborted guest must not commit a journal"
        ),
        Err(_) => {}
    }
}
