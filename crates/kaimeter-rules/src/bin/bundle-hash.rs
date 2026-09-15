//! Canonical bundle hashing CLI.
//!
//! Walks a rule-bundle directory, drops the paths `bundle.json` excludes,
//! and prints the SHA-256 of the canonical serialisation (whitepaper §4.3).
//!
//! ```text
//! bundle-hash [BUNDLE-ROOT]
//! ```
//!
//! `BUNDLE-ROOT` defaults to the working directory. The hash goes to stdout
//! as `sha256:<hex>`; diagnostics go to stderr.

#![deny(clippy::float_arithmetic)]

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use kaimeter_rules::bundle::{BundleFile, BundleHash, BundleMetadata, bundle_hash};

/// One readable bundle file.
#[derive(Debug)]
struct FoundFile {
    path: String,
    content: Vec<u8>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("bundle-hash: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let root = arguments.next().unwrap_or_else(|| String::from("."));
    if arguments.next().is_some() {
        return Err(String::from("usage: bundle-hash [bundle-root]"));
    }
    let hash = hash_bundle(Path::new(&root))?;
    println!("{hash}");
    Ok(())
}

fn hash_bundle(root: &Path) -> Result<BundleHash, String> {
    let metadata_path = root.join("bundle.json");
    let metadata_text = fs::read_to_string(&metadata_path)
        .map_err(|error| format!("cannot read {}: {error}", metadata_path.display()))?;
    let metadata = BundleMetadata::from_json(&metadata_text)
        .map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let files = collect_files(root, &metadata)?;
    let entries: Vec<BundleFile<'_>> = files
        .iter()
        .map(|file| BundleFile {
            path: &file.path,
            content: &file.content,
        })
        .collect();
    bundle_hash(&entries).map_err(|error| error.to_string())
}

fn collect_files(root: &Path, metadata: &BundleMetadata) -> Result<Vec<FoundFile>, String> {
    let mut files = Vec::new();
    collect_into(root, root, metadata, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn collect_into(
    root: &Path,
    directory: &Path,
    metadata: &BundleMetadata,
    files: &mut Vec<FoundFile>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
        let path = entry.path();
        let relative = relative_path(root, &path)?;
        if metadata.excludes(&relative) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if file_type.is_dir() {
            collect_into(root, &path, metadata, files)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let content =
            fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        files.push(FoundFile {
            path: relative,
            content,
        });
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let mut text = String::new();
    for (index, component) in relative.components().enumerate() {
        let part = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 path: {}", path.display()))?;
        if index > 0 {
            text.push('/');
        }
        text.push_str(part);
    }
    Ok(text)
}
