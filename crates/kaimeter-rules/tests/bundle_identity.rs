//! Conformance tests for the `bundle-hash` binary and this crate's identity.
//!
//! The pinned hash is the deliberate record of the bundle's identity: any
//! change to a semantic file under `crates/kaimeter-rules/` (source,
//! parameters, `Cargo.toml`, `bundle.json`, `CITATIONS.md`) changes it, and
//! the constant must be regenerated in the same commit.

use std::path::{Path, PathBuf};
use std::process::Command;

use kaimeter_rules::bundle::{BundleFile, BundleMetadata, bundle_hash, embedded_metadata};

/// SHA-256 of this crate's semantic files under the canonical serialisation.
///
/// Regenerate with:
///
/// ```text
/// cargo run -p kaimeter-rules --bin bundle-hash -- crates/kaimeter-rules
/// ```
const PINNED_BUNDLE_HASH: &str =
    "sha256:cd4f4446cf09ac8c0b5bb5a5cf91dd774eeb885c034674c09507696f60604c27";

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn bundle_hash_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bundle-hash"))
}

fn unique_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kaimeter-bundle-identity-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn binary_matches_the_pinned_bundle_identity() {
    let output = bundle_hash_command().arg(crate_root()).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), PINNED_BUNDLE_HASH);
}

#[test]
fn binary_agrees_with_the_library_hash() {
    let output = bundle_hash_command().arg(crate_root()).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), library_hash(crate_root()));
}

#[test]
fn hashes_the_working_directory_by_default() {
    let output = bundle_hash_command()
        .current_dir(crate_root())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), PINNED_BUNDLE_HASH);
}

#[test]
fn fails_without_bundle_metadata() {
    let directory = unique_temp_dir("missing-bundle");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("Cargo.toml"), "[package]").unwrap();

    let output = bundle_hash_command().arg(&directory).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("bundle.json"), "{stderr}");

    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn fails_on_invalid_metadata() {
    let directory = unique_temp_dir("invalid-metadata");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("bundle.json"), "{").unwrap();

    let output = bundle_hash_command().arg(&directory).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid bundle metadata"), "{stderr}");

    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn fails_on_a_missing_directory() {
    let directory = unique_temp_dir("missing-directory");
    let output = bundle_hash_command().arg(&directory).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot read"), "{stderr}");
}

#[test]
fn rejects_extra_arguments() {
    let output = bundle_hash_command().args([".", "extra"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("usage"), "{stderr}");
}

/// Recomputes the bundle hash with an independent walk of the same tree.
fn library_hash(root: &Path) -> String {
    let metadata = embedded_metadata().unwrap();
    let mut found: Vec<(String, Vec<u8>)> = Vec::new();
    walk(root, root, &metadata, &mut found);
    let files: Vec<BundleFile<'_>> = found
        .iter()
        .map(|(path, content)| BundleFile { path, content })
        .collect();
    bundle_hash(&files).unwrap().to_string()
}

fn walk(
    root: &Path,
    directory: &Path,
    metadata: &BundleMetadata,
    found: &mut Vec<(String, Vec<u8>)>,
) {
    for entry in std::fs::read_dir(directory).unwrap() {
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
            found.push((relative, std::fs::read(&path).unwrap()));
        }
    }
}
