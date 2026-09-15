//! Sourced parameter tables embedded in the bundle.
//!
//! Parameter tables are JSON data with row-level provenance: legal
//! citation, source URL and retrieval date travel with every value
//! (whitepaper §4.2, §9.1). The tables and their extraction are documented
//! in `parameters/README.md`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::RuleError;

/// Embedded global-warming-potential table.
pub const GWP_JSON: &str = include_str!("../parameters/gwp.json");

/// Embedded Annex I default values, aluminium scope.
pub const DEFAULT_VALUES_ANNEX_I_JSON: &str =
    include_str!("../parameters/default-values-annex-I.json");

/// Embedded Annex IV highest default values, aluminium scope.
pub const DEFAULT_VALUES_ANNEX_IV_JSON: &str =
    include_str!("../parameters/default-values-annex-IV.json");

/// One global-warming-potential row.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GwpRow {
    /// Chemical formula, for example `CF4`.
    pub substance: String,
    /// 100-year global warming potential, t `CO2e` per t.
    pub gwp100: String,
    /// Legal citation for this row.
    pub legal: String,
    /// Source URL for this row.
    pub source: String,
    /// Retrieval date for this row, ISO 8601.
    pub retrieved: String,
}

/// Global-warming-potential table.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GwpTable {
    /// Table name.
    pub table: String,
    /// Legal citation for the table.
    pub legal: String,
    /// Source URL for the table.
    pub source: String,
    /// Retrieval date for the table, ISO 8601.
    pub retrieved: String,
    /// Table rows.
    pub rows: Vec<GwpRow>,
}

/// One Annex I default-value row.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DefaultValueRow {
    /// CN code without spaces, for example `76041090`.
    pub cn_code: String,
    /// Country of production, or the other-countries fallback label.
    pub country: String,
    /// Default direct emissions, t `CO2e` per t.
    pub direct: Option<String>,
    /// Default indirect emissions, t `CO2e` per t; absent for direct-only sectors.
    pub indirect: Option<String>,
    /// Default total emissions, t `CO2e` per t.
    pub total: Option<String>,
    /// Underlying production route determining the CBAM benchmark.
    pub route: Option<String>,
    /// Legal citation for this row.
    pub legal: String,
    /// Source URL for this row.
    pub source: String,
    /// Retrieval date for this row, ISO 8601.
    pub retrieved: String,
}

/// Annex I default values for the aluminium scope.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DefaultValuesAnnexI {
    /// Table name.
    pub table: String,
    /// Legal citation for the table.
    pub legal: String,
    /// Source URL for the table.
    pub source: String,
    /// Retrieval date for the table, ISO 8601.
    pub retrieved: String,
    /// SHA-256 of the reviewed workbook.
    pub workbook_sha256: String,
    /// CN code to goods description.
    pub codes: BTreeMap<String, String>,
    /// Country rows.
    pub rows: Vec<DefaultValueRow>,
    /// Fallback table for unlisted countries and missing values.
    pub other_countries: Vec<DefaultValueRow>,
}

/// One Annex IV highest-value row.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnnexIvRow {
    /// CN code without spaces, for example `76041090`.
    pub cn_code: String,
    /// Goods description.
    pub description: String,
    /// Highest default value, t `CO2e` per t.
    pub highest: String,
    /// Underlying production route determining the CBAM benchmark.
    pub route: Option<String>,
    /// Legal citation for this row.
    pub legal: String,
    /// Source URL for this row.
    pub source: String,
    /// Retrieval date for this row, ISO 8601.
    pub retrieved: String,
}

/// Annex IV highest default values for unknown-origin precursors.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DefaultValuesAnnexIv {
    /// Table name.
    pub table: String,
    /// Legal citation for the table.
    pub legal: String,
    /// Source URL for the table.
    pub source: String,
    /// Retrieval date for the table, ISO 8601.
    pub retrieved: String,
    /// SHA-256 of the reviewed workbook.
    pub workbook_sha256: String,
    /// Table rows.
    pub rows: Vec<AnnexIvRow>,
}

impl GwpTable {
    /// Parses the table from JSON.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Parameters`] with the position of the first
    /// error when the text is not valid table data.
    pub fn from_json(text: &str) -> Result<Self, RuleError> {
        parse_table(text, "gwp")
    }
}

impl DefaultValuesAnnexI {
    /// Parses the table from JSON.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Parameters`] with the position of the first
    /// error when the text is not valid table data.
    pub fn from_json(text: &str) -> Result<Self, RuleError> {
        parse_table(text, "default-values-annex-I")
    }
}

impl DefaultValuesAnnexIv {
    /// Parses the table from JSON.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Parameters`] with the position of the first
    /// error when the text is not valid table data.
    pub fn from_json(text: &str) -> Result<Self, RuleError> {
        parse_table(text, "default-values-annex-IV")
    }
}

/// Parameter tables embedded in this bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterTables {
    /// Global warming potentials used by the PFC methods.
    pub gwp: GwpTable,
    /// Annex I default values.
    pub default_values_annex_i: DefaultValuesAnnexI,
    /// Annex IV highest default values.
    pub default_values_annex_iv: DefaultValuesAnnexIv,
}

impl ParameterTables {
    /// Parses all embedded tables.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::Parameters`] when an embedded table is malformed.
    pub fn embedded() -> Result<Self, RuleError> {
        Ok(Self {
            gwp: GwpTable::from_json(GWP_JSON)?,
            default_values_annex_i: DefaultValuesAnnexI::from_json(DEFAULT_VALUES_ANNEX_I_JSON)?,
            default_values_annex_iv: DefaultValuesAnnexIv::from_json(DEFAULT_VALUES_ANNEX_IV_JSON)?,
        })
    }
}

/// Parses one table, attributing errors to it.
fn parse_table<T: DeserializeOwned>(text: &str, table: &'static str) -> Result<T, RuleError> {
    serde_json::from_str(text).map_err(|error| RuleError::Parameters {
        table,
        line: error.line(),
        column: error.column(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::period::Date;
    use crate::fixed::Fixed;

    fn tables() -> ParameterTables {
        ParameterTables::embedded().unwrap()
    }

    fn find<'a>(rows: &'a [DefaultValueRow], country: &str, code: &str) -> &'a DefaultValueRow {
        rows.iter()
            .find(|row| row.country == country && row.cn_code == code)
            .unwrap_or_else(|| panic!("missing {country} {code}"))
    }

    fn assert_route(route: Option<&str>, cn_code: &str) {
        if let Some(route) = route {
            assert_eq!(route.len(), 1, "{cn_code}");
            assert!(route.bytes().all(|byte| byte.is_ascii_uppercase()));
        }
    }

    #[test]
    fn parses_embedded_tables() {
        let tables = tables();
        assert_eq!(tables.gwp.table, "gwp");
        assert_eq!(tables.gwp.rows.len(), 2);
        let annex_i = &tables.default_values_annex_i;
        assert_eq!(annex_i.table, "default-values-annex-I");
        assert_eq!(annex_i.rows.len(), 1608);
        assert_eq!(annex_i.other_countries.len(), 24);
        assert_eq!(annex_i.codes.len(), 24);
        assert_eq!(
            annex_i.workbook_sha256,
            "900583811c7e1194799eb9bdbad2d6d7e1100f5a7d80a664c1584a8fce6f9f35"
        );
        assert_eq!(
            annex_i.legal,
            "Implementing Regulation (EU) 2025/2621, Annex I, as corrected by \
             Implementing Regulation (EU) 2026/1740"
        );
        assert_eq!(tables.default_values_annex_iv.rows.len(), 24);
    }

    #[test]
    fn carries_aluminium_values() {
        let tables = tables();
        let cf4 = tables
            .gwp
            .rows
            .iter()
            .find(|row| row.substance == "CF4")
            .unwrap();
        assert_eq!(cf4.gwp100, "6630");
        let c2f6 = tables
            .gwp
            .rows
            .iter()
            .find(|row| row.substance == "C2F6")
            .unwrap();
        assert_eq!(c2f6.gwp100, "11100");

        let annex_i = &tables.default_values_annex_i;
        assert_eq!(annex_i.codes["7601"], "Unwrought aluminium");
        let china = find(&annex_i.rows, "China", "7601");
        assert_eq!(china.direct.as_deref(), Some("3.000"));
        assert_eq!(china.indirect, None);
        assert_eq!(china.total.as_deref(), Some("3.000"));
        assert_eq!(china.route.as_deref(), Some("K"));
        let profiles = find(&annex_i.rows, "China", "76041090");
        assert_eq!(profiles.total.as_deref(), Some("4.881"));
        let other = find(
            &annex_i.other_countries,
            "Other Countries and Territories",
            "7601",
        );
        assert_eq!(other.total.as_deref(), Some("2.203"));

        let unknown = tables
            .default_values_annex_iv
            .rows
            .iter()
            .find(|row| row.cn_code == "7601")
            .unwrap();
        assert_eq!(unknown.highest, "3.198");
    }

    #[test]
    fn attaches_provenance_to_every_row() {
        let tables = tables();
        let annex_i = &tables.default_values_annex_i;
        assert!(!annex_i.legal.is_empty());
        assert!(annex_i.source.starts_with("https://"));
        assert!(Date::from_iso(&annex_i.retrieved).is_ok());

        for row in annex_i.rows.iter().chain(&annex_i.other_countries) {
            assert_eq!(row.legal, annex_i.legal);
            assert_eq!(row.source, annex_i.source);
            assert_eq!(row.retrieved, annex_i.retrieved);
            assert!(annex_i.codes.contains_key(&row.cn_code), "{}", row.cn_code);
        }
        for row in &tables.default_values_annex_iv.rows {
            assert!(!row.legal.is_empty());
            assert!(row.source.starts_with("https://"));
            assert!(Date::from_iso(&row.retrieved).is_ok());
        }
    }

    #[test]
    fn every_row_is_a_direct_only_aluminium_good() {
        let tables = tables();
        let annex_i = &tables.default_values_annex_i;
        for row in annex_i.rows.iter().chain(&annex_i.other_countries) {
            assert!(row.cn_code.starts_with("76"), "{}", row.cn_code);
            assert!(matches!(row.cn_code.len(), 4 | 6 | 8), "{}", row.cn_code);
            assert!(row.cn_code.bytes().all(|byte| byte.is_ascii_digit()));
            assert!(!row.country.is_empty());
            assert_eq!(row.indirect, None, "{}", row.cn_code);
            let total: Fixed = row.total.as_ref().expect("total").parse().unwrap();
            let direct: Fixed = row.direct.as_ref().expect("direct").parse().unwrap();
            assert_eq!(total, direct, "{}", row.cn_code);
            assert_route(row.route.as_deref(), &row.cn_code);
        }
    }

    #[test]
    fn rejects_malformed_tables() {
        match GwpTable::from_json("{") {
            Err(RuleError::Parameters {
                table,
                line,
                column,
            }) => {
                assert_eq!(table, "gwp");
                assert!(line >= 1);
                assert!(column >= 1);
            }
            other => panic!("expected a parameter error, got {other:?}"),
        }
        assert!(matches!(
            DefaultValuesAnnexI::from_json(r#"{"table":"default-values-annex-I"}"#),
            Err(RuleError::Parameters { .. })
        ));
        assert!(matches!(
            DefaultValuesAnnexIv::from_json("{}"),
            Err(RuleError::Parameters { .. })
        ));
    }
}
