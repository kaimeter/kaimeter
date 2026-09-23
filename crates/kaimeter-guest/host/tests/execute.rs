//! Guest pipeline checks for the in-circuit opening verification.
//!
//! The guest must accept a witness only when the opening reaches the claimed
//! root, and commit exactly the journal the interpreter computes natively for
//! the same witness.

use kaimeter_guest_host::KAIMETER_GUEST_ELF;
use kaimeter_interpreter::bundle::{bundle_hash, open_file, BundleFile, BundleHash, Opening};
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

/// The pinned root and the opening of the only file.
fn opening() -> (BundleHash, Opening) {
    let files = [BundleFile {
        path: "rules.json",
        content: RULES,
    }];
    let root = bundle_hash(&files).unwrap();
    let opening = open_file(&files, "rules.json").unwrap();
    (root, opening)
}

/// Executes the guest with the given root and opening, returning its journal.
fn execute(root: BundleHash, opening: &Opening) -> Result<Vec<u8>, String> {
    let invocation = String::from(INVOCATION);
    let mut builder = ExecutorEnv::builder();
    builder.write(root.as_bytes()).unwrap();
    builder.write(&1_u32).unwrap();
    builder.write(&opening.path).unwrap();
    builder.write(&opening.content).unwrap();
    builder.write(&opening.index).unwrap();
    builder.write(&opening.size).unwrap();
    builder.write(&(opening.proof.len() as u32)).unwrap();
    for sibling in &opening.proof {
        builder.write(sibling.as_bytes()).unwrap();
    }
    builder.write(&invocation).unwrap();
    let env = builder.build().unwrap();
    match default_executor().execute(env, KAIMETER_GUEST_ELF) {
        Ok(session) => Ok(session.journal.bytes),
        Err(error) => Err(error.to_string()),
    }
}

#[test]
fn guest_verifies_the_opening_and_commits_the_journal() {
    let (root, opening) = opening();
    let bundle = bundle();
    let invocation = bundle.parse_invocation(INVOCATION).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();
    let expected = outcome
        .journal(root, invocation.context.clone())
        .canonical_bytes();
    assert_eq!(outcome.output.to_string(), "42.000000");

    let committed = execute(root, &opening).unwrap();
    assert_eq!(committed, expected);
}

#[test]
fn guest_rejects_a_root_mismatch() {
    let (root, opening) = opening();
    let mut claimed = *root.as_bytes();
    claimed[0] ^= 0xff;
    let claimed = BundleHash::from_bytes(claimed);

    match execute(claimed, &opening) {
        Ok(committed) => assert!(
            committed.is_empty(),
            "an aborted guest must not commit a journal"
        ),
        Err(_) => {}
    }
}
