//! Canonical bundle metadata and hashing.
//!
//! A bundle's identity is the SHA-256 digest of its canonical byte stream:
//! the semantic files only (paths excluded in `bundle.json` are omitted),
//! sorted by their `/`-separated relative path, with CRLF and CR line
//! endings normalised to LF and every entry length-prefixed so that no two
//! file sets can produce the same stream. The construction is itself part of
//! the identity: changing it changes every hash.
//!
//! The stream is:
//!
//! ```text
//! "kaimeter-bundle-v1\n"
//! for each file, in ascending path order:
//!     path length    (u64, little endian)
//!     path bytes     (UTF-8, `/` separators)
//!     content length (u64, little endian)
//!     content bytes  (UTF-8, LF line endings)
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Domain separation prefix of the canonical byte stream.
const CANONICAL_PREFIX: &[u8] = b"kaimeter-bundle-v1\n";

/// The embedded `bundle.json` of this crate.
pub const BUNDLE_JSON: &str = include_str!("../bundle.json");

/// One file offered to the canonical serialiser.
#[derive(Clone, Copy, Debug)]
pub struct BundleFile<'a> {
    /// Bundle-relative path; `/` and `\` separators are normalised to `/`.
    pub path: &'a str,
    /// Raw file bytes; CRLF and CR line endings are normalised to LF.
    pub content: &'a [u8],
}

/// Canonical `bundle.json` metadata (whitepaper §9.1).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BundleMetadata {
    /// Bundle family name, `cbam`.
    pub bundle: String,
    /// Semantic bundle version, for example `2026.2.0`.
    pub version: String,
    /// Jurisdiction whose rules the bundle encodes.
    pub jurisdiction: String,
    /// Sectors the bundle addresses.
    pub sectors: Vec<String>,
    /// First day the bundle applies, ISO 8601.
    pub applies_from: String,
    /// Last day the bundle applies, ISO 8601; `null` for open-ended.
    pub applies_to: Option<String>,
    /// Legal instruments the bundle encodes.
    pub legal_basis: Vec<String>,
    /// Bundle version this one supersedes, if any.
    pub supersedes: Option<String>,
    /// What changed in each version and why.
    pub changelog: Vec<ChangelogEntry>,
    /// Paths excluded from the bundle hash.
    pub excluded_paths: Vec<String>,
}

/// One `bundle.json` changelog entry.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ChangelogEntry {
    /// Bundle version the entry describes.
    pub version: String,
    /// Why that version exists.
    pub note: String,
}

/// Errors from canonical serialisation or metadata parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BundleError {
    /// No files were offered; a bundle is never empty.
    Empty,
    /// A path was empty, absolute, or contained an empty, `.` or `..` part.
    InvalidPath(String),
    /// Two files resolved to the same canonical path.
    DuplicatePath(String),
    /// File content was not valid UTF-8.
    NonUtf8Content(String),
    /// `bundle.json` was not valid metadata.
    Metadata {
        /// Line of the first parse error.
        line: usize,
        /// Column of the first parse error.
        column: usize,
    },
}

impl fmt::Display for BundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("bundle contains no files"),
            Self::InvalidPath(path) => write!(formatter, "invalid bundle path `{path}`"),
            Self::DuplicatePath(path) => write!(formatter, "duplicate bundle path `{path}`"),
            Self::NonUtf8Content(path) => write!(formatter, "non-UTF-8 content in `{path}`"),
            Self::Metadata { line, column } => {
                write!(
                    formatter,
                    "invalid bundle metadata at line {line}, column {column}"
                )
            }
        }
    }
}

impl core::error::Error for BundleError {}

/// SHA-256 digest identifying a canonical bundle serialisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BundleHash([u8; 32]);

impl BundleHash {
    /// Wraps a digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the raw digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the lowercase hexadecimal digest without a prefix.
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut hex = String::with_capacity(64);
        for byte in self.0 {
            hex.push(hex_digit(byte >> 4));
            hex.push(hex_digit(byte & 0x0f));
        }
        hex
    }
}

impl fmt::Display for BundleHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sha256:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Returns the lowercase hexadecimal digit for a nibble.
fn hex_digit(nibble: u8) -> char {
    char::from(b"0123456789abcdef"[usize::from(nibble)])
}

impl BundleMetadata {
    /// Parses `bundle.json` content.
    ///
    /// # Errors
    ///
    /// Returns [`BundleError::Metadata`] with the position of the first
    /// error when the text is not valid metadata.
    pub fn from_json(text: &str) -> Result<Self, BundleError> {
        serde_json::from_str(text).map_err(|error| BundleError::Metadata {
            line: error.line(),
            column: error.column(),
        })
    }

    /// Returns `true` when a canonical relative path is excluded.
    ///
    /// A pattern is either an exact path (`Cargo.toml`) or a directory
    /// prefix ending in `/**` (`tests/**`), which matches every path below
    /// that directory.
    #[must_use]
    pub fn excludes(&self, path: &str) -> bool {
        self.excluded_paths
            .iter()
            .any(|pattern| matches_pattern(pattern, path))
    }
}

/// Matches one exclusion pattern against a `/`-separated path.
fn matches_pattern(pattern: &str, path: &str) -> bool {
    match pattern.strip_suffix("/**") {
        Some(directory) => {
            path.len() > directory.len()
                && path.starts_with(directory)
                && path.as_bytes()[directory.len()] == b'/'
        }
        None => pattern == path,
    }
}

/// Validates a bundle-relative path and normalises separators to `/`.
fn canonical_path(path: &str) -> Result<String, BundleError> {
    let mut normalized = String::with_capacity(path.len());
    for (index, component) in path.split(['/', '\\']).enumerate() {
        if component.is_empty() || component == "." || component == ".." {
            return Err(BundleError::InvalidPath(String::from(path)));
        }
        if index > 0 {
            normalized.push('/');
        }
        normalized.push_str(component);
    }
    Ok(normalized)
}

/// Returns content with CRLF and CR line endings normalised to LF.
fn normalize_newlines(content: &[u8], path: &str) -> Result<Vec<u8>, BundleError> {
    if core::str::from_utf8(content).is_err() {
        return Err(BundleError::NonUtf8Content(String::from(path)));
    }
    let mut normalized = Vec::with_capacity(content.len());
    let mut index = 0;
    while index < content.len() {
        if content[index] != b'\r' {
            normalized.push(content[index]);
            index += 1;
            continue;
        }
        normalized.push(b'\n');
        let step = if content.get(index + 1) == Some(&b'\n') {
            2
        } else {
            1
        };
        index += step;
    }
    Ok(normalized)
}

/// Parses the embedded [`BUNDLE_JSON`].
///
/// # Errors
///
/// Returns the same errors as [`BundleMetadata::from_json`].
pub fn embedded_metadata() -> Result<BundleMetadata, BundleError> {
    BundleMetadata::from_json(BUNDLE_JSON)
}

/// Builds the canonical byte stream of a set of bundle files.
///
/// # Errors
///
/// Returns [`BundleError::Empty`] for an empty set; [`BundleError::InvalidPath`]
/// for an empty, absolute or dot-containing path; [`BundleError::DuplicatePath`]
/// when two files normalise to the same path; and
/// [`BundleError::NonUtf8Content`] when a file is not valid UTF-8.
pub fn canonical_bytes(files: &[BundleFile<'_>]) -> Result<Vec<u8>, BundleError> {
    if files.is_empty() {
        return Err(BundleError::Empty);
    }
    let mut entries = Vec::with_capacity(files.len());
    for file in files {
        let path = canonical_path(file.path)?;
        let content = normalize_newlines(file.content, &path)?;
        entries.push((path, content));
    }
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    let mut stream = Vec::new();
    stream.extend_from_slice(CANONICAL_PREFIX);
    let mut previous: Option<&str> = None;
    for (path, content) in &entries {
        if previous == Some(path.as_str()) {
            return Err(BundleError::DuplicatePath(path.clone()));
        }
        previous = Some(path);
        stream.extend_from_slice(&(path.len() as u64).to_le_bytes());
        stream.extend_from_slice(path.as_bytes());
        stream.extend_from_slice(&(content.len() as u64).to_le_bytes());
        stream.extend_from_slice(content);
    }
    Ok(stream)
}

/// Hashes the canonical byte stream of a set of bundle files.
///
/// # Errors
///
/// Returns the same errors as [`canonical_bytes`].
pub fn bundle_hash(files: &[BundleFile<'_>]) -> Result<BundleHash, BundleError> {
    let canonical = canonical_bytes(files)?;
    let digest = Sha256::digest(&canonical);
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&digest);
    Ok(BundleHash(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical stream for the two-file pin below.
    const EXPECTED_CANONICAL: &[u8] = b"kaimeter-bundle-v1\n\
\x05\x00\x00\x00\x00\x00\x00\x00a.txt\
\x02\x00\x00\x00\x00\x00\x00\x00x\n\
\x08\x00\x00\x00\x00\x00\x00\x00b/c.json\
\x03\x00\x00\x00\x00\x00\x00\x00{}\n";

    /// Independently computed with `hashlib.sha256` over `EXPECTED_CANONICAL`.
    const EXPECTED_HASH: &str = "2684974b8aaee3d99e69933aa56c9782d86f3641a7fded5ce54ab17e228a8840";

    fn pin_files() -> [BundleFile<'static>; 2] {
        [
            BundleFile {
                path: "b/c.json",
                content: b"{}\r\n",
            },
            BundleFile {
                path: "a.txt",
                content: b"x\n",
            },
        ]
    }

    #[test]
    fn parses_embedded_metadata() {
        let metadata = embedded_metadata().unwrap();
        assert_eq!(metadata.bundle, "cbam");
        assert_eq!(metadata.version, "2026.2.0");
        assert_eq!(metadata.jurisdiction, "EU");
        assert_eq!(metadata.sectors, ["aluminium"]);
        assert_eq!(metadata.applies_from, "2026-01-01");
        assert_eq!(metadata.applies_to, None);
        assert_eq!(metadata.legal_basis.len(), 3);
        assert_eq!(metadata.legal_basis[0], "Regulation (EU) 2023/956");
        assert_eq!(metadata.supersedes.as_deref(), Some("2026.1.x"));
        assert_eq!(metadata.changelog.len(), 1);
        assert_eq!(metadata.changelog[0].version, "2026.2.0");
        assert!(!metadata.changelog[0].note.is_empty());
        assert_eq!(
            metadata.excluded_paths,
            ["tests/**", "tools/**", "src/bin/**"]
        );
    }

    #[test]
    fn reports_invalid_metadata() {
        match BundleMetadata::from_json("{") {
            Err(BundleError::Metadata { line, column }) => {
                assert!(line >= 1);
                assert!(column >= 1);
            }
            other => panic!("expected a metadata error, got {other:?}"),
        }
        assert!(matches!(
            BundleMetadata::from_json(r#"{"bundle":"cbam"}"#),
            Err(BundleError::Metadata { .. })
        ));
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
    fn rejects_an_empty_bundle() {
        assert_eq!(canonical_bytes(&[]), Err(BundleError::Empty));
    }

    #[test]
    fn rejects_invalid_paths() {
        for path in ["", "/", "/a", "a/", "a//b", "./a", "a/./b", "../a", "a/.."] {
            let entries = [BundleFile {
                path,
                content: b"x",
            }];
            assert_eq!(
                canonical_bytes(&entries),
                Err(BundleError::InvalidPath(path.to_owned())),
                "{path}"
            );
        }
    }

    #[test]
    fn normalises_windows_separators() {
        let windows = [BundleFile {
            path: "a\\b.rs",
            content: b"x\n",
        }];
        let posix = [BundleFile {
            path: "a/b.rs",
            content: b"x\n",
        }];
        assert_eq!(
            canonical_bytes(&windows).unwrap(),
            canonical_bytes(&posix).unwrap()
        );
    }

    #[test]
    fn rejects_duplicate_paths() {
        let entries = [
            BundleFile {
                path: "a\\b",
                content: b"x",
            },
            BundleFile {
                path: "a/b",
                content: b"y",
            },
        ];
        assert_eq!(
            canonical_bytes(&entries),
            Err(BundleError::DuplicatePath("a/b".to_owned()))
        );
    }

    #[test]
    fn rejects_non_utf8_content() {
        let entries = [BundleFile {
            path: "raw.bin",
            content: &[0xff, 0xfe],
        }];
        assert_eq!(
            canonical_bytes(&entries),
            Err(BundleError::NonUtf8Content("raw.bin".to_owned()))
        );
    }

    #[test]
    fn writes_the_canonical_layout() {
        let canonical = canonical_bytes(&pin_files()).unwrap();
        assert_eq!(canonical, EXPECTED_CANONICAL);
    }

    #[test]
    fn pins_the_bundle_hash() {
        let hash = bundle_hash(&pin_files()).unwrap();
        assert_eq!(hash.to_hex(), EXPECTED_HASH);
        assert_eq!(hash.to_string(), format!("sha256:{EXPECTED_HASH}"));
        assert_eq!(BundleHash::from_bytes(*hash.as_bytes()), hash);
    }

    #[test]
    fn hash_is_order_independent() {
        let forward = [
            BundleFile {
                path: "a.txt",
                content: b"x\n",
            },
            BundleFile {
                path: "b/c.json",
                content: b"{}\r\n",
            },
        ];
        let mut reversed = forward;
        reversed.reverse();
        assert_eq!(
            bundle_hash(&forward).unwrap(),
            bundle_hash(&reversed).unwrap()
        );
    }

    #[test]
    fn hash_changes_with_content() {
        let first = [BundleFile {
            path: "a.txt",
            content: b"x\n",
        }];
        let second = [BundleFile {
            path: "a.txt",
            content: b"y\n",
        }];
        assert_ne!(bundle_hash(&first).unwrap(), bundle_hash(&second).unwrap());
    }

    #[test]
    fn normalises_line_endings() {
        let crlf = [BundleFile {
            path: "a.txt",
            content: b"a\r\nb\rc\n",
        }];
        let lf = [BundleFile {
            path: "a.txt",
            content: b"a\nb\nc\n",
        }];
        assert_eq!(bundle_hash(&crlf).unwrap(), bundle_hash(&lf).unwrap());
    }

    #[test]
    fn displays_errors() {
        assert_eq!(BundleError::Empty.to_string(), "bundle contains no files");
        assert_eq!(
            BundleError::InvalidPath("x".to_owned()).to_string(),
            "invalid bundle path `x`"
        );
        assert_eq!(
            BundleError::DuplicatePath("x".to_owned()).to_string(),
            "duplicate bundle path `x`"
        );
        assert_eq!(
            BundleError::NonUtf8Content("x".to_owned()).to_string(),
            "non-UTF-8 content in `x`"
        );
        assert_eq!(
            BundleError::Metadata { line: 2, column: 7 }.to_string(),
            "invalid bundle metadata at line 2, column 7"
        );
    }
}
