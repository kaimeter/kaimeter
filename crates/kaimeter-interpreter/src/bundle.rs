//! Canonical bundle metadata and hashing.
//!
//! A bundle's identity is the root of a SHA-256 Merkle tree over its semantic
//! files: the same file set and normalisation as the linear construction,
//! committed per file so that a zero-knowledge guest can verify only the files
//! it actually reads (interpreter contract §4). Paths are sorted, CRLF and CR
//! line endings are normalised to LF, and excluded paths are dropped before
//! hashing.
//!
//! The construction is:
//!
//! ```text
//! leaf_i = SHA-256(0x00 || "kaimeter-bundle-v2\n"
//!                  || u64le(path length) || path
//!                  || u64le(content length) || content)
//! node(l, r) = SHA-256(0x01 || l || r)
//! root = MTH(leaves)   (RFC 6962: split at the largest power of two below n,
//!                       promote an odd trailing node instead of duplicating)
//! ```
//!
//! [`open_file`] returns the audit path of one file and [`verify_opening`]
//! checks it against the root, so a verifier that pins the root needs neither
//! the whole bundle nor its own copy of the tree.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Domain separation prefix of the canonical construction.
pub const BUNDLE_PREFIX: &[u8] = b"kaimeter-bundle-v2\n";

/// Domain separation tag of a leaf hash.
const LEAF_TAG: u8 = 0x00;

/// Domain separation tag of an internal node hash.
const NODE_TAG: u8 = 0x01;

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
    /// Semantic bundle version, for example `2026.3.0`.
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
    /// Paths excluded from the bundle identity.
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
    /// No file in the set carries the requested canonical path.
    UnknownFile(String),
    /// An inclusion proof does not reach the pinned root.
    InvalidProof,
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
            Self::UnknownFile(path) => write!(formatter, "unknown bundle file `{path}`"),
            Self::InvalidProof => formatter.write_str("invalid bundle inclusion proof"),
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

/// An inclusion proof for one file in the canonical tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opening {
    /// Canonical path of the file.
    pub path: String,
    /// LF-normalised content of the file.
    pub content: Vec<u8>,
    /// Leaf index in path order.
    pub index: u64,
    /// Number of leaves in the tree.
    pub size: u64,
    /// Sibling hashes from the leaf to the root.
    pub proof: Vec<BundleHash>,
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
    let mut previous = None;
    for &byte in content {
        if byte == b'\r' {
            normalized.push(b'\n');
        } else if byte != b'\n' || previous != Some(b'\r') {
            normalized.push(byte);
        }
        previous = Some(byte);
    }
    Ok(normalized)
}

/// Returns the canonical entries of a file set, sorted by path bytes.
fn sorted_entries(files: &[BundleFile<'_>]) -> Result<Vec<(String, Vec<u8>)>, BundleError> {
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
    let mut previous: Option<&str> = None;
    for (path, _) in &entries {
        if previous == Some(path.as_str()) {
            return Err(BundleError::DuplicatePath(path.clone()));
        }
        previous = Some(path);
    }
    Ok(entries)
}

/// Hashes one canonical file entry.
fn leaf_hash(path: &str, content: &[u8]) -> BundleHash {
    let mut hasher = Sha256::new();
    hasher.update([LEAF_TAG]);
    hasher.update(BUNDLE_PREFIX);
    hasher.update((path.len() as u64).to_le_bytes());
    hasher.update(path.as_bytes());
    hasher.update((content.len() as u64).to_le_bytes());
    hasher.update(content);
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&hasher.finalize());
    BundleHash(bytes)
}

/// Hashes one internal tree node.
fn node_hash(left: &BundleHash, right: &BundleHash) -> BundleHash {
    let mut hasher = Sha256::new();
    hasher.update([NODE_TAG]);
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&hasher.finalize());
    BundleHash(bytes)
}

/// Returns the largest power of two strictly below `n`, for `n >= 2`.
fn split_point(n: usize) -> usize {
    let mut k = 1 << (usize::BITS - 1 - (n - 1).leading_zeros());
    if k >= n {
        k /= 2;
    }
    k
}

/// Returns the Merkle tree root of a non-empty slice of leaves.
fn tree_root(leaves: &[BundleHash]) -> Result<BundleHash, BundleError> {
    match leaves.len() {
        0 => Err(BundleError::Empty),
        1 => Ok(leaves[0]),
        n => {
            let k = split_point(n);
            let left = tree_root(&leaves[..k])?;
            let right = tree_root(&leaves[k..])?;
            Ok(node_hash(&left, &right))
        }
    }
}

/// Appends the audit path of `index` in `leaves`, leaf to root.
fn build_proof(
    leaves: &[BundleHash],
    index: usize,
    proof: &mut Vec<BundleHash>,
) -> Result<(), BundleError> {
    match leaves.len() {
        0 => Err(BundleError::Empty),
        1 => {
            if index == 0 {
                Ok(())
            } else {
                Err(BundleError::InvalidProof)
            }
        }
        n => {
            let k = split_point(n);
            if index < k {
                build_proof(&leaves[..k], index, proof)?;
                proof.push(tree_root(&leaves[k..])?);
            } else {
                build_proof(&leaves[k..], index - k, proof)?;
                proof.push(tree_root(&leaves[..k])?);
            }
            Ok(())
        }
    }
}

/// Computes the canonical root of a set of bundle files.
///
/// # Errors
///
/// Returns [`BundleError::Empty`] for an empty set; [`BundleError::InvalidPath`]
/// for an empty, absolute or dot-containing path; [`BundleError::DuplicatePath`]
/// when two files normalise to the same path; and
/// [`BundleError::NonUtf8Content`] when a file is not valid UTF-8.
pub fn bundle_hash(files: &[BundleFile<'_>]) -> Result<BundleHash, BundleError> {
    let entries = sorted_entries(files)?;
    let leaves: Vec<BundleHash> = entries
        .iter()
        .map(|(path, content)| leaf_hash(path, content))
        .collect();
    tree_root(&leaves)
}

/// Returns the inclusion proof for one file of a set.
///
/// # Errors
///
/// Returns the errors of [`bundle_hash`] and [`BundleError::UnknownFile`]
/// when the canonical path is not part of the set.
pub fn open_file(files: &[BundleFile<'_>], path: &str) -> Result<Opening, BundleError> {
    let wanted = canonical_path(path)?;
    let entries = sorted_entries(files)?;
    let index = entries
        .iter()
        .position(|(entry, _)| entry == &wanted)
        .ok_or_else(|| BundleError::UnknownFile(wanted.clone()))?;
    let leaves: Vec<BundleHash> = entries
        .iter()
        .map(|(path, content)| leaf_hash(path, content))
        .collect();
    let mut proof = Vec::new();
    build_proof(&leaves, index, &mut proof)?;
    let (path, content) = entries[index].clone();
    Ok(Opening {
        path,
        content,
        index: index as u64,
        size: entries.len() as u64,
        proof,
    })
}

/// Folds a leaf up the tree, consuming the top sibling at each level.
fn fold(leaf: BundleHash, index: usize, size: usize, proof: &[BundleHash]) -> Option<BundleHash> {
    if size == 1 {
        return if proof.is_empty() { Some(leaf) } else { None };
    }
    let (top, rest) = proof.split_last()?;
    let k = split_point(size);
    if index < k {
        let left = fold(leaf, index, k, rest)?;
        Some(node_hash(&left, top))
    } else {
        let right = fold(leaf, index - k, size - k, rest)?;
        Some(node_hash(top, &right))
    }
}

/// Checks one inclusion proof against a pinned root.
///
/// # Errors
///
/// Returns [`BundleError::InvalidProof`] when the path does not reach the
/// root, and [`BundleError::InvalidPath`] when the opening names a path that
/// is not canonical.
pub fn verify_opening(root: BundleHash, opening: &Opening) -> Result<(), BundleError> {
    let index = usize::try_from(opening.index).map_err(|_| BundleError::InvalidProof)?;
    let size = usize::try_from(opening.size).map_err(|_| BundleError::InvalidProof)?;
    if size == 0 || index >= size {
        return Err(BundleError::InvalidProof);
    }
    let path = canonical_path(&opening.path)?;
    let leaf = leaf_hash(&path, &opening.content);
    match fold(leaf, index, size, &opening.proof) {
        Some(node) if node == root => Ok(()),
        _ => Err(BundleError::InvalidProof),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_LEAF_A: &str =
        "e5941b03171b410ce700f350493cc69f73e0d94b832bb447ef48468b206d8040";
    const EXPECTED_LEAF_B: &str =
        "b325f2a6802da7d7be3cb5077582f5031ada02f30480e166f42180e60f40ad6d";
    /// Independently computed with `hashlib.sha256` over the same construction.
    const EXPECTED_ROOT: &str = "d8d06827344f4eddc6946b55c0bb39dbd157545d5dc483a953a348d704942ddd";

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

    fn files_of(size: usize) -> Vec<(String, String)> {
        (0..size)
            .map(|index| {
                (
                    alloc::format!("f{index}.txt"),
                    alloc::format!("content {index}\n"),
                )
            })
            .collect()
    }

    fn borrowed(owned: &[(String, String)]) -> Vec<BundleFile<'_>> {
        owned
            .iter()
            .map(|(path, content)| BundleFile {
                path,
                content: content.as_bytes(),
            })
            .collect()
    }

    #[test]
    fn pins_an_independently_computed_root() {
        let files = pin_files();
        let entries = sorted_entries(&files).unwrap();
        assert_eq!(
            leaf_hash(&entries[0].0, &entries[0].1).to_hex(),
            EXPECTED_LEAF_A
        );
        assert_eq!(
            leaf_hash(&entries[1].0, &entries[1].1).to_hex(),
            EXPECTED_LEAF_B
        );
        let root = bundle_hash(&files).unwrap();
        assert_eq!(root.to_hex(), EXPECTED_ROOT);
        assert_eq!(root.to_string(), alloc::format!("sha256:{EXPECTED_ROOT}"));
        assert_eq!(BundleHash::from_bytes(*root.as_bytes()), root);
    }

    #[test]
    fn rejects_an_empty_bundle() {
        assert_eq!(bundle_hash(&[]), Err(BundleError::Empty));
    }

    #[test]
    fn rejects_invalid_paths() {
        for path in ["", "/", "/a", "a/", "a//b", "./a", "a/./b", "../a", "a/.."] {
            let entries = [BundleFile {
                path,
                content: b"x",
            }];
            assert_eq!(
                bundle_hash(&entries),
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
        assert_eq!(bundle_hash(&windows), bundle_hash(&posix));
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
            bundle_hash(&entries),
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
            bundle_hash(&entries),
            Err(BundleError::NonUtf8Content("raw.bin".to_owned()))
        );
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
        assert_eq!(bundle_hash(&forward), bundle_hash(&reversed));
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
        assert_eq!(bundle_hash(&crlf), bundle_hash(&lf));

        let mixed = [BundleFile {
            path: "a.txt",
            content: b"\n\r\r\nx\r",
        }];
        let mixed_lf = [BundleFile {
            path: "a.txt",
            content: b"\n\n\nx\n",
        }];
        assert_eq!(bundle_hash(&mixed), bundle_hash(&mixed_lf));
    }

    #[test]
    fn openings_verify_for_every_index_and_size() {
        for size in 1..=16_usize {
            check_all_openings(size);
        }
    }

    /// Checks every index of one tree size.
    fn check_all_openings(size: usize) {
        let owned = files_of(size);
        let files = borrowed(&owned);
        let root = bundle_hash(&files).unwrap();
        for index in 0..size {
            let path = alloc::format!("f{index}.txt");
            let opening = open_file(&files, &path).unwrap();
            assert_eq!(opening.size, size as u64);
            assert_eq!(verify_opening(root, &opening), Ok(()), "{size}/{index}");
        }
    }

    #[test]
    fn openings_accept_windows_separators() {
        let files = pin_files();
        let root = bundle_hash(&files).unwrap();
        let opening = open_file(&files, "a.txt").unwrap();
        assert_eq!(verify_opening(root, &opening), Ok(()));
    }

    #[test]
    fn openings_reject_a_wrong_root() {
        let files = pin_files();
        let mut root = bundle_hash(&files).unwrap();
        root = BundleHash::from_bytes([root.as_bytes()[0] ^ 0xff; 32]);
        let opening = open_file(&files, "a.txt").unwrap();
        assert_eq!(
            verify_opening(root, &opening),
            Err(BundleError::InvalidProof)
        );
    }

    #[test]
    fn openings_reject_tampered_content() {
        let files = pin_files();
        let root = bundle_hash(&files).unwrap();
        let mut opening = open_file(&files, "a.txt").unwrap();
        opening.content = b"y\n".to_vec();
        assert_eq!(
            verify_opening(root, &opening),
            Err(BundleError::InvalidProof)
        );
    }

    #[test]
    fn openings_reject_tampered_paths_and_proofs() {
        let files = pin_files();
        let root = bundle_hash(&files).unwrap();
        let opening = open_file(&files, "a.txt").unwrap();

        let mut renamed = opening.clone();
        renamed.path = String::from("b.txt");
        assert_eq!(
            verify_opening(root, &renamed),
            Err(BundleError::InvalidProof)
        );

        let mut extended = opening.clone();
        extended.proof.push(BundleHash::from_bytes([0; 32]));
        assert_eq!(
            verify_opening(root, &extended),
            Err(BundleError::InvalidProof)
        );

        let mut resized = opening.clone();
        resized.size += 1;
        assert_eq!(
            verify_opening(root, &resized),
            Err(BundleError::InvalidProof)
        );

        let mut reindexed = opening;
        reindexed.index = 1;
        assert_eq!(
            verify_opening(root, &reindexed),
            Err(BundleError::InvalidProof)
        );
    }

    #[test]
    fn reports_unknown_files() {
        let files = pin_files();
        assert_eq!(
            open_file(&files, "missing.txt"),
            Err(BundleError::UnknownFile(String::from("missing.txt")))
        );
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
            BundleError::UnknownFile("x".to_owned()).to_string(),
            "unknown bundle file `x`"
        );
        assert_eq!(
            BundleError::InvalidProof.to_string(),
            "invalid bundle inclusion proof"
        );
        assert_eq!(
            BundleError::Metadata { line: 2, column: 7 }.to_string(),
            "invalid bundle metadata at line 2, column 7"
        );
    }
}
