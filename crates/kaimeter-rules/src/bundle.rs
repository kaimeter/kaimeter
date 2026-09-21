//! Canonical bundle metadata and hashing.
//!
//! The canonical Merkle commitment and the SHA-256 identity live in
//! `kaimeter-interpreter`, once, so that the pinned native identity and the
//! root recomputed inside the zero-knowledge guest cannot drift (whitepaper
//! §5.4; interpreter contract §4). This module re-exports that construction
//! and adds this crate's embedded `bundle.json`.

pub use kaimeter_interpreter::bundle::{
    BUNDLE_PREFIX, BundleError, BundleFile, BundleHash, BundleMetadata, ChangelogEntry, Opening,
    bundle_hash, open_file, verify_opening,
};

/// The embedded `bundle.json` of this crate.
pub const BUNDLE_JSON: &str = include_str!("../bundle.json");

/// Parses the embedded [`BUNDLE_JSON`].
///
/// # Errors
///
/// Returns the same errors as [`BundleMetadata::from_json`].
pub fn embedded_metadata() -> Result<BundleMetadata, BundleError> {
    BundleMetadata::from_json(BUNDLE_JSON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_embedded_metadata() {
        let metadata = embedded_metadata().unwrap();
        assert_eq!(metadata.bundle, "cbam");
        assert_eq!(metadata.version, "2026.3.0");
        assert_eq!(metadata.jurisdiction, "EU");
        assert_eq!(metadata.sectors, ["aluminium"]);
        assert_eq!(metadata.applies_from, "2026-01-01");
        assert_eq!(metadata.applies_to, None);
        assert_eq!(metadata.legal_basis.len(), 3);
        assert_eq!(metadata.legal_basis[0], "Regulation (EU) 2023/956");
        assert_eq!(metadata.supersedes.as_deref(), Some("2026.2.0"));
        assert_eq!(metadata.changelog.len(), 2);
        assert_eq!(metadata.changelog[1].version, "2026.3.0");
        assert!(!metadata.changelog[1].note.is_empty());
        assert_eq!(
            metadata.excluded_paths,
            ["tests/**", "tools/**", "src/bin/**"]
        );
    }

    #[test]
    fn excludes_paths_by_pattern() {
        let metadata = embedded_metadata().unwrap();
        assert!(metadata.excludes("tests/vectors/aluminium.json"));
        assert!(metadata.excludes("tools/extract-default-values.py"));
        assert!(metadata.excludes("src/bin/bundle-hash.rs"));
        assert!(!metadata.excludes("tests"));
        assert!(!metadata.excludes("tools"));
        assert!(!metadata.excludes("testsx/y"));
        assert!(!metadata.excludes("src/lib.rs"));
        assert!(!metadata.excludes("Cargo.toml"));
    }

    #[test]
    fn reexports_the_canonical_construction() {
        let files = [BundleFile {
            path: "a.txt",
            content: b"x\n",
        }];
        assert!(BUNDLE_PREFIX.starts_with(b"kaimeter-bundle-v2"));
        let root = bundle_hash(&files).unwrap();
        assert_eq!(root.to_hex().len(), 64);
        let opening = open_file(&files, "a.txt").unwrap();
        assert_eq!(verify_opening(root, &opening), Ok(()));
        assert!(matches!(
            BundleMetadata::from_json("{"),
            Err(BundleError::Metadata { .. })
        ));
    }
}
