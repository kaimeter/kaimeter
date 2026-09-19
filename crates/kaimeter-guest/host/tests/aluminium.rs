//! The aluminium witness: bundle `2026.3.0` evaluated in the guest.
//!
//! The witness is the canonical file set of `kaimeter-rules` — the set the
//! pinned identity covers — plus the Appendix B invocation and the claimed
//! hash. The journal the guest commits must equal the native computation, and
//! its output field must carry the vector's value at full precision.

use std::fs;
use std::path::Path;

use kaimeter_guest_host::KAIMETER_GUEST_ELF;
use kaimeter_interpreter::bundle::{bundle_hash, BundleFile, BundleMetadata};
use kaimeter_interpreter::fixed::Fixed;
use kaimeter_interpreter::journal::JOURNAL_PREFIX;
use kaimeter_interpreter::rule::RuleBundle;
use risc0_zkvm::{default_executor, ExecutorEnv};

/// The rule crate whose canonical file set is the witness.
const RULES_CRATE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kaimeter-rules");

/// The published Appendix B vector.
const VECTOR: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../kaimeter-rules/tests/vectors/aluminium/worked-example.json"
));

/// Collects the canonical file set of a bundle directory.
fn canonical_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let metadata =
        BundleMetadata::from_json(&fs::read_to_string(root.join("bundle.json")).unwrap()).unwrap();
    let mut found = Vec::new();
    walk(root, root, &metadata, &mut found);
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

/// Walks one directory, skipping excluded paths.
fn walk(
    root: &Path,
    directory: &Path,
    metadata: &BundleMetadata,
    found: &mut Vec<(String, Vec<u8>)>,
) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if metadata.excludes(&relative) {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, metadata, found);
        } else if path.is_file() {
            found.push((relative, fs::read(&path).unwrap()));
        }
    }
}

/// Builds the invocation of the vector's rule.
fn invocation_text() -> String {
    let vector: serde_json::Value = serde_json::from_str(VECTOR).unwrap();
    serde_json::json!({
        "rule": "aluminium.primary.slope",
        "context": {
            "sector": vector["sector"],
            "route": vector["route"],
            "cnCode": vector["cn_code"],
            "period": vector["period"],
        },
        "inputs": vector["inputs"],
    })
    .to_string()
}

#[test]
fn aluminium_appendix_b_journal_matches_natively() {
    let files = canonical_files(Path::new(RULES_CRATE));
    let borrowed: Vec<BundleFile<'_>> = files
        .iter()
        .map(|(path, content)| BundleFile { path, content })
        .collect();
    let hash = bundle_hash(&borrowed).unwrap();
    let bundle = RuleBundle::from_files(&borrowed).unwrap();
    let invocation_text = invocation_text();
    let invocation = bundle.parse_invocation(&invocation_text).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();
    assert_eq!(outcome.output.to_string(), "1.835038");
    let expected = outcome.journal(hash, invocation.context).canonical_bytes();

    let file_count = files.len() as u32;
    let claimed = hash.as_bytes().to_vec();
    let mut builder = ExecutorEnv::builder();
    builder.write(&file_count).unwrap();
    for (path, content) in &files {
        builder.write(path).unwrap();
        builder.write(content).unwrap();
    }
    builder.write(&invocation_text).unwrap();
    builder.write(&claimed).unwrap();
    let env = builder.build().unwrap();

    let session = default_executor().execute(env, KAIMETER_GUEST_ELF).unwrap();
    let committed = session.journal.bytes;
    assert_eq!(committed, expected);

    let hash_offset = JOURNAL_PREFIX.len();
    assert_eq!(
        &committed[hash_offset..hash_offset + 32],
        hash.as_bytes().as_slice()
    );
    let mut scaled = [0_u8; 16];
    scaled.copy_from_slice(&committed[hash_offset + 32..hash_offset + 48]);
    assert_eq!(
        Fixed::from_scaled(i128::from_le_bytes(scaled)).to_string(),
        "1.835038"
    );
}
