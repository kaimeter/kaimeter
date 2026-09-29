//! The aluminium witness: bundle `2026.3.0` evaluated in the guest.
//!
//! The witness is the pinned root of the `kaimeter-rules` canonical file set
//! plus inclusion proofs for the files the evaluation reads, so the guest
//! authenticates every byte it touches without seeing the rest of the crate.
//! The journal the guest commits must equal the native computation, and its
//! output field must carry the vector's value at full precision.

use std::fs;
use std::path::Path;

use kaimeter_guest_host::{KAIMETER_GUEST_ELF, KAIMETER_GUEST_ID};
use kaimeter_interpreter::bundle::{
    bundle_hash, open_file, BundleFile, BundleHash, BundleMetadata, Opening,
};
use kaimeter_interpreter::fixed::Fixed;
use kaimeter_interpreter::journal::JOURNAL_PREFIX;
use kaimeter_interpreter::rule::RuleBundle;
use risc0_zkvm::{default_executor, default_prover, ExecutorEnv};

/// The rule crate whose canonical file set is the witness.
const RULES_CRATE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kaimeter-rules");

/// The files the Appendix B evaluation reads.
const READ_PATHS: [&str; 2] = ["rules.json", "parameters/gwp.json"];

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

/// The pinned root of the full file set and the openings the evaluation reads.
fn setup() -> (BundleHash, Vec<Opening>) {
    let files = canonical_files(Path::new(RULES_CRATE));
    let borrowed: Vec<BundleFile<'_>> = files
        .iter()
        .map(|(path, content)| BundleFile { path, content })
        .collect();
    let root = bundle_hash(&borrowed).unwrap();
    let openings = READ_PATHS
        .iter()
        .map(|path| open_file(&borrowed, path).unwrap())
        .collect();
    (root, openings)
}

/// Builds the executor environment for one witness.
fn witness_env(root: BundleHash, openings: &[Opening], invocation: &str) -> ExecutorEnv<'static> {
    let mut builder = ExecutorEnv::builder();
    builder.write(root.as_bytes()).unwrap();
    builder.write(&(openings.len() as u32)).unwrap();
    for opening in openings {
        builder.write(&opening.path).unwrap();
        builder.write(&opening.content).unwrap();
        builder.write(&opening.index).unwrap();
        builder.write(&opening.size).unwrap();
        builder.write(&(opening.proof.len() as u32)).unwrap();
        for sibling in &opening.proof {
            builder.write(sibling.as_bytes()).unwrap();
        }
    }
    builder.write(&invocation.to_string()).unwrap();
    builder.build().unwrap()
}

#[test]
fn aluminium_appendix_b_journal_matches_natively() {
    let (root, openings) = setup();
    let invocation_text = invocation_text();
    let borrowed: Vec<BundleFile<'_>> = openings
        .iter()
        .map(|opening| BundleFile {
            path: &opening.path,
            content: &opening.content,
        })
        .collect();
    let bundle = RuleBundle::from_files(&borrowed).unwrap();
    let invocation = bundle.parse_invocation(&invocation_text).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();
    assert_eq!(outcome.output.to_string(), "1.835038");
    let expected = outcome.journal(root, invocation.context).canonical_bytes();

    let env = witness_env(root, &openings, &invocation_text);
    let session = default_executor().execute(env, KAIMETER_GUEST_ELF).unwrap();
    let committed = session.journal.bytes;
    assert_eq!(committed, expected);

    let hash_offset = JOURNAL_PREFIX.len();
    assert_eq!(
        &committed[hash_offset..hash_offset + 32],
        root.as_bytes().as_slice()
    );
    let mut scaled = [0_u8; 16];
    scaled.copy_from_slice(&committed[hash_offset + 32..hash_offset + 48]);
    assert_eq!(
        Fixed::from_scaled(i128::from_le_bytes(scaled)).to_string(),
        "1.835038"
    );
}

/// Measures the Appendix B execution and prints its cycle counts.
///
/// Ignored: pull requests execute without proving (interpreter contract §6).
/// Run on demand with
/// `cargo test --test aluminium -- --ignored --nocapture`.
#[test]
#[ignore = "measurement runs with the release metadata, not on pull requests"]
fn measures_the_appendix_b_execution() {
    let (root, openings) = setup();
    let invocation_text = invocation_text();
    let env = witness_env(root, &openings, &invocation_text);

    let session = default_executor().execute(env, KAIMETER_GUEST_ELF).unwrap();
    let cycles: u32 = session.segments.iter().map(|segment| segment.cycles).sum();
    println!(
        "v0.2 execution: segments={} cycles={cycles}",
        session.segments.len(),
    );
}

/// Proves the Appendix B witness and prints the release measurement.
///
/// Ignored: pull requests execute without proving (interpreter contract §6).
/// Proving needs more than a few gigabytes of RAM; run on CI or a larger
/// instance with `cargo test --test aluminium -- --ignored --nocapture`.
#[test]
#[ignore = "proving runs with the release metadata, not on pull requests"]
fn proves_the_appendix_b_witness() {
    let (root, openings) = setup();
    let invocation_text = invocation_text();
    let env = witness_env(root, &openings, &invocation_text);

    let started = std::time::Instant::now();
    let info = default_prover().prove(env, KAIMETER_GUEST_ELF).unwrap();
    let elapsed = started.elapsed();
    info.receipt.verify(KAIMETER_GUEST_ID).unwrap();
    println!(
        "v0.2 measurement: total_cycles={} user_cycles={} paging_cycles={} segments={} proving={elapsed:?}",
        info.stats.total_cycles,
        info.stats.user_cycles,
        info.stats.paging_cycles,
        info.stats.segments,
    );
}
