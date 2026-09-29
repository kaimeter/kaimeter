//! The public journal: the `h_B`, output and context a proof commits.
//!
//! The byte layout is fixed here so that a native verifier and the guest
//! commit and check exactly the same bytes. It is:
//!
//! ```text
//! "kaimeter-journal-v1\n"
//! bundle hash      (32 bytes)
//! output           (i128, little endian, scaled by 10⁶)
//! sector           (u64 little-endian length, then UTF-8 bytes)
//! route            (u64 little-endian length, then UTF-8 bytes)
//! CN code          (u64 little-endian length, then UTF-8 bytes)
//! period           (u64 little-endian length, then UTF-8 bytes)
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use serde::Deserialize;

use crate::bundle::BundleHash;
use crate::fixed::Fixed;

/// Domain separation prefix of the journal byte stream.
pub const JOURNAL_PREFIX: &[u8] = b"kaimeter-journal-v1\n";

/// Public context of a proved computation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Context {
    /// Sector the good belongs to, for example `aluminium`.
    pub sector: String,
    /// Production route, for example `primary`.
    pub route: String,
    /// CN code of the good, with or without spaces.
    pub cn_code: String,
    /// Reporting period, for example `2026`.
    pub period: String,
}

/// The public outputs of one rule evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Journal {
    /// Rule-bundle hash the evaluation is bound to.
    pub bundle_hash: BundleHash,
    /// The proven output, as a scaled fixed-point scalar.
    pub output: Fixed,
    /// Public context of the evaluation.
    pub context: Context,
}

impl Journal {
    /// Encodes the journal in its fixed byte layout.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(JOURNAL_PREFIX);
        bytes.extend_from_slice(self.bundle_hash.as_bytes());
        bytes.extend_from_slice(&self.output.scaled().to_le_bytes());
        for field in [
            &self.context.sector,
            &self.context.route,
            &self.context.cn_code,
            &self.context.period,
        ] {
            bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
            bytes.extend_from_slice(field.as_bytes());
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            sector: String::from("aluminium"),
            route: String::from("primary"),
            cn_code: String::from("7601"),
            period: String::from("2026"),
        }
    }

    #[test]
    fn writes_the_fixed_byte_layout() {
        let journal = Journal {
            bundle_hash: BundleHash::from_bytes([0xab; 32]),
            output: "1.835038".parse().unwrap(),
            context: context(),
        };
        let mut expected = Vec::new();
        expected.extend_from_slice(b"kaimeter-journal-v1\n");
        expected.extend_from_slice(&[0xab; 32]);
        expected.extend_from_slice(&1_835_038_i128.to_le_bytes());
        expected.extend_from_slice(&9_u64.to_le_bytes());
        expected.extend_from_slice(b"aluminium");
        expected.extend_from_slice(&7_u64.to_le_bytes());
        expected.extend_from_slice(b"primary");
        expected.extend_from_slice(&4_u64.to_le_bytes());
        expected.extend_from_slice(b"7601");
        expected.extend_from_slice(&4_u64.to_le_bytes());
        expected.extend_from_slice(b"2026");
        assert_eq!(journal.canonical_bytes(), expected);
    }

    #[test]
    fn parses_the_context_shape() {
        let json = r#"{"sector":"aluminium","route":"primary","cnCode":"7601","period":"2026"}"#;
        let parsed: Context = serde_json::from_str(json).unwrap();
        assert_eq!(parsed, context());
        assert!(serde_json::from_str::<Context>(r#"{"sector":"aluminium"}"#).is_err());
        assert!(
            serde_json::from_str::<Context>(
                r#"{"sector":"a","route":"b","cnCode":"c","period":"d","extra":1}"#
            )
            .is_err()
        );
    }
}
