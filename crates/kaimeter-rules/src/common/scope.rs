//! Annex I scope table.
//!
//! Maps declared CN codes to the sector covered by the bundle, with the
//! dates from which each entry applies. Adding a sector or a downstream CN
//! code is a new row here, not a new architecture (whitepaper §4.2, R8).
//!
//! @legal  Regulation (EU) 2023/956, Annex I, as amended by Regulation (EU) 2025/2083
//! @source <http://data.europa.eu/eli/reg/2023/956/oj>
//! @since  bundle 2026.2.0

use crate::common::Sector;
use crate::common::normalize_cn_code;
use crate::common::period::Date;
use crate::error::RuleError;

/// One Annex I scope entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeEntry {
    /// CN code without spaces; four and six-digit entries cover their
    /// eight-digit children.
    pub cn_code: &'static str,
    /// Sector the code belongs to.
    pub sector: Sector,
    /// First day the entry applies.
    pub applies_from: Date,
    /// Last day the entry applies, inclusive; `None` for open-ended.
    pub applies_to: Option<Date>,
    /// Legal citation for the entry.
    pub legal: &'static str,
}

impl ScopeEntry {
    /// Returns `true` when the entry applies on a date.
    #[must_use]
    pub fn applies_on(&self, on: Date) -> bool {
        self.applies_from <= on && self.applies_to.is_none_or(|end| on <= end)
    }
}

/// Builds a scope date at compile time.
const fn date(year: u16, month: u8, day: u8) -> Date {
    match Date::from_parts(year, month, day) {
        Ok(date) => date,
        Err(_) => panic!("invalid scope date"),
    }
}

/// First day of the CBAM definitive period.
const DEFINITIVE_PERIOD: Date = date(2026, 1, 1);

/// Legal citation shared by the aluminium entries.
const ALUMINIUM_LEGAL: &str =
    "Regulation (EU) 2023/956, Annex I, as amended by Regulation (EU) 2025/2083";

/// Builds an aluminium entry applying from the definitive period.
const fn aluminium(cn_code: &'static str) -> ScopeEntry {
    ScopeEntry {
        cn_code,
        sector: Sector::Aluminium,
        applies_from: DEFINITIVE_PERIOD,
        applies_to: None,
        legal: ALUMINIUM_LEGAL,
    }
}

/// Annex I aluminium codes covered by bundle 2026.2.0.
pub const ANNEX_I_ALUMINIUM: &[ScopeEntry] = &[
    aluminium("7601"),
    aluminium("7603"),
    aluminium("76041010"),
    aluminium("76041090"),
    aluminium("76042100"),
    aluminium("76042910"),
    aluminium("76042990"),
    aluminium("7605"),
    aluminium("7606"),
    aluminium("7607"),
    aluminium("7608"),
    aluminium("76090000"),
    aluminium("76101000"),
    aluminium("761090"),
    aluminium("76109010"),
    aluminium("76109090"),
    aluminium("76110000"),
    aluminium("7612"),
    aluminium("76130000"),
    aluminium("7614"),
    aluminium("76161000"),
    aluminium("76169100"),
    aluminium("76169910"),
    aluminium("76169990"),
];

/// Classifies a declared CN code on a date.
///
/// The most specific entry covering the code governs: an eight-digit exact
/// entry, then a six or four-digit parent. An entry that is not yet in force
/// on the date leaves the good out of scope even when a broader parent
/// exists.
///
/// # Errors
///
/// Returns [`RuleError::InvalidCnCode`] for a malformed code and
/// [`RuleError::OutOfScope`] when no entry covers the code on that date.
pub fn classify(cn_code: &str, on: Date) -> Result<ScopeEntry, RuleError> {
    let code = normalize_cn_code(cn_code)?;
    let entry = [8_usize, 6, 4]
        .into_iter()
        .filter_map(|length| code.get(..length))
        .find_map(|prefix| {
            ANNEX_I_ALUMINIUM
                .iter()
                .find(|entry| entry.cn_code == prefix)
        })
        .filter(|entry| entry.applies_on(on))
        .copied()
        .ok_or(RuleError::OutOfScope)?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(text: &str) -> Date {
        Date::from_iso(text).unwrap()
    }

    #[test]
    fn classifies_every_listed_code() {
        for entry in ANNEX_I_ALUMINIUM {
            let found = classify(entry.cn_code, on("2026-01-01")).unwrap();
            assert_eq!(found, *entry);
            assert_eq!(found.sector, Sector::Aluminium);
        }
    }

    #[test]
    fn covers_eight_digit_children_of_shorter_entries() {
        assert_eq!(
            classify("76011000", on("2026-09-16")).unwrap().cn_code,
            "7601"
        );
        assert_eq!(
            classify("7605 00 00", on("2026-09-16")).unwrap().cn_code,
            "7605"
        );
        assert_eq!(
            classify("76109099", on("2026-09-16")).unwrap().cn_code,
            "761090"
        );
        assert_eq!(
            classify("76041010", on("2026-09-16")).unwrap().cn_code,
            "76041010"
        );
    }

    #[test]
    fn rejects_unlisted_and_foreign_codes() {
        for code in ["7208", "25070080", "76169999", "2814", "2704"] {
            assert_eq!(
                classify(code, on("2026-09-16")),
                Err(RuleError::OutOfScope),
                "{code}"
            );
        }
    }

    #[test]
    fn rejects_malformed_codes() {
        for code in ["", "76", "760", "76011", "760410101", "7604-10", "ABCD"] {
            assert_eq!(
                classify(code, on("2026-09-16")),
                Err(RuleError::InvalidCnCode),
                "{code}"
            );
        }
    }

    #[test]
    fn rejects_dates_before_the_definitive_period() {
        assert_eq!(
            classify("7601", on("2025-12-31")),
            Err(RuleError::OutOfScope)
        );
    }

    #[test]
    fn carries_dates_and_citations() {
        for entry in ANNEX_I_ALUMINIUM {
            assert_eq!(entry.applies_from, on("2026-01-01"));
            assert_eq!(entry.applies_to, None);
            assert!(entry.legal.contains("2023/956"));
            assert!(entry.applies_on(entry.applies_from));
        }
    }

    #[test]
    fn honours_the_end_of_an_entry_window() {
        let entry = ScopeEntry {
            cn_code: "7601",
            sector: Sector::Aluminium,
            applies_from: on("2026-01-01"),
            applies_to: Some(on("2026-12-31")),
            legal: "test",
        };
        assert!(entry.applies_on(on("2026-12-31")));
        assert!(!entry.applies_on(on("2027-01-01")));
    }

    #[test]
    fn lists_the_v0_1_aluminium_codes() {
        let codes: Vec<&str> = ANNEX_I_ALUMINIUM
            .iter()
            .map(|entry| entry.cn_code)
            .collect();
        assert_eq!(codes.len(), 24);
        for code in [
            "7601", "7603", "76041010", "76041090", "76042100", "76042910", "76042990", "7605",
            "7606", "7607", "7608", "76090000", "76101000", "761090", "76109010", "76109090",
            "76110000", "7612", "76130000", "7614", "76161000", "76169100", "76169910", "76169990",
        ] {
            assert!(codes.contains(&code), "{code}");
        }
    }
}
