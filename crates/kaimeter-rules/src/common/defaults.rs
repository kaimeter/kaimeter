//! Default-value selection.
//!
//! A default value is a country- and CN-code-specific figure from Annex I of
//! the Default Values Act. Where the good is not listed for the country of
//! production, or the country row shows `-`, the `Other Countries and
//! Territories` table applies; precursors of unknown origin use the Annex IV
//! highest value. Every selected value carries the sector- and
//! year-dependent mark-up.
//!
//! @legal  Implementing Regulation (EU) 2025/2621, Article 4, as corrected
//!         by Implementing Regulation (EU) 2026/1740
//! @source <https://taxation-customs.ec.europa.eu/document/download/29b9eec7-1a4b-4eb6-ab85-96a0c9e35fd0_en?filename=Guidance%20No.%203%20-%20CBAM%20methods%20for%20the%20calculation%20of%20emissions%20embedded%20in%20goods.pdf>
//! @since  bundle 2026.2.0

use alloc::string::String;

use crate::common::Sector;
use crate::common::markups;
use crate::common::period::ReportingPeriod;
use crate::error::RuleError;
use crate::fixed::Fixed;
use crate::parameters::{AnnexIvRow, DefaultValueRow, DefaultValuesAnnexI, DefaultValuesAnnexIv};

/// A default-value request for a good of known origin.
#[derive(Clone, Copy, Debug)]
pub struct DefaultQuery<'a> {
    /// CN code, with or without spaces.
    pub cn_code: &'a str,
    /// Country of production as named in Annex I.
    pub country: &'a str,
    /// Sector the good belongs to.
    pub sector: Sector,
}

/// A selected default value with its provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultValue {
    /// Base value before mark-up, t `CO2e` per t.
    pub base: Fixed,
    /// Applied mark-up rate.
    pub markup: Fixed,
    /// Value after mark-up, t `CO2e` per t.
    pub applied: Fixed,
    /// Which table supplied the base value.
    pub source: DefaultSource,
}

/// Where a selected default value came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultSource {
    /// Country-specific Annex I row.
    Country {
        /// CN code of the matched row.
        cn_code: String,
        /// Country of the matched row.
        country: String,
    },
    /// Other-countries fallback row.
    OtherCountries {
        /// CN code of the matched row.
        cn_code: String,
    },
    /// Annex IV highest value for an unknown-origin precursor.
    AnnexIv {
        /// CN code of the matched row.
        cn_code: String,
    },
}

/// Selects the default value for a good of known country of production.
///
/// # Errors
///
/// Returns [`RuleError::InvalidCnCode`] for a malformed code,
/// [`RuleError::NoDefaultValue`] when no table covers the good, and
/// arithmetic or parse errors from the fixed-point layer.
pub fn default_value(
    query: &DefaultQuery<'_>,
    period: ReportingPeriod,
    annex_i: &DefaultValuesAnnexI,
) -> Result<DefaultValue, RuleError> {
    let code = normalize_cn_code(query.cn_code)?;
    let markup = markups::markup(query.sector, period);
    let (total, source) = country_value(&annex_i.rows, query.country, &code)
        .or_else(|| other_countries_value(&annex_i.other_countries, &code))
        .ok_or(RuleError::NoDefaultValue)?;
    assemble(total, source, markup)
}

/// Selects the Annex IV highest default value for an unknown-origin precursor.
///
/// # Errors
///
/// Returns the same errors as [`default_value`].
pub fn precursor_default_value(
    cn_code: &str,
    sector: Sector,
    period: ReportingPeriod,
    annex_iv: &DefaultValuesAnnexIv,
) -> Result<DefaultValue, RuleError> {
    let code = normalize_cn_code(cn_code)?;
    let markup = markups::markup(sector, period);
    let row = best_annex_iv_row(&annex_iv.rows, &code).ok_or(RuleError::NoDefaultValue)?;
    let source = DefaultSource::AnnexIv {
        cn_code: row.cn_code.clone(),
    };
    assemble(&row.highest, source, markup)
}

/// Resolves the longest country-specific code match, if the row has a value.
fn country_value<'a>(
    rows: &'a [DefaultValueRow],
    country: &str,
    code: &str,
) -> Option<(&'a str, DefaultSource)> {
    let row = best_row(rows, Some(country), code)?;
    let total = row.total.as_deref()?;
    let source = DefaultSource::Country {
        cn_code: row.cn_code.clone(),
        country: row.country.clone(),
    };
    Some((total, source))
}

/// Resolves the longest other-countries code match, if the row has a value.
fn other_countries_value<'a>(
    rows: &'a [DefaultValueRow],
    code: &str,
) -> Option<(&'a str, DefaultSource)> {
    let row = best_row(rows, None, code)?;
    let total = row.total.as_deref()?;
    let source = DefaultSource::OtherCountries {
        cn_code: row.cn_code.clone(),
    };
    Some((total, source))
}

/// Finds the longest 8-, 6- or 4-digit prefix listed in a row set.
fn best_row<'a>(
    rows: &'a [DefaultValueRow],
    country: Option<&str>,
    code: &str,
) -> Option<&'a DefaultValueRow> {
    [8_usize, 6, 4]
        .into_iter()
        .filter_map(|length| code.get(..length))
        .find_map(|prefix| {
            rows.iter()
                .find(|row| row.cn_code == prefix && country.is_none_or(|name| row.country == name))
        })
}

/// Finds the longest 8-, 6- or 4-digit prefix listed in Annex IV.
fn best_annex_iv_row<'a>(rows: &'a [AnnexIvRow], code: &str) -> Option<&'a AnnexIvRow> {
    [8_usize, 6, 4]
        .into_iter()
        .filter_map(|length| code.get(..length))
        .find_map(|prefix| rows.iter().find(|row| row.cn_code == prefix))
}

/// Applies the mark-up to a base value.
fn assemble(total: &str, source: DefaultSource, markup: Fixed) -> Result<DefaultValue, RuleError> {
    let base: Fixed = total.parse()?;
    let factor = Fixed::ONE.try_add(markup)?;
    let applied = base.try_mul(factor)?;
    Ok(DefaultValue {
        base,
        markup,
        applied,
        source,
    })
}

/// Normalizes a CN code to bare digits.
fn normalize_cn_code(text: &str) -> Result<String, RuleError> {
    let mut normalized = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_whitespace() {
            continue;
        }
        if !character.is_ascii_digit() {
            return Err(RuleError::InvalidCnCode);
        }
        normalized.push(character);
    }
    if !matches!(normalized.len(), 4 | 6 | 8) {
        return Err(RuleError::InvalidCnCode);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;

    use super::*;
    use crate::parameters::ParameterTables;

    fn period(year: u16) -> ReportingPeriod {
        ReportingPeriod::new(year).unwrap()
    }

    fn fixed(text: &str) -> Fixed {
        text.parse().unwrap()
    }

    fn tables() -> ParameterTables {
        ParameterTables::embedded().unwrap()
    }

    fn country_query<'a>(cn_code: &'a str, country: &'a str) -> DefaultQuery<'a> {
        DefaultQuery {
            cn_code,
            country,
            sector: Sector::Aluminium,
        }
    }

    fn row(cn_code: &str, country: &str, total: Option<&str>) -> DefaultValueRow {
        DefaultValueRow {
            cn_code: cn_code.to_string(),
            country: country.to_string(),
            direct: total.map(ToString::to_string),
            indirect: None,
            total: total.map(ToString::to_string),
            route: Some(String::from("K")),
            legal: String::from("test"),
            source: String::from("https://example.test"),
            retrieved: String::from("2026-09-16"),
        }
    }

    fn synthetic(rows: Vec<DefaultValueRow>, other: Vec<DefaultValueRow>) -> DefaultValuesAnnexI {
        DefaultValuesAnnexI {
            table: String::from("test"),
            legal: String::from("test"),
            source: String::from("https://example.test"),
            retrieved: String::from("2026-09-16"),
            workbook_sha256: String::from("test"),
            codes: BTreeMap::new(),
            rows,
            other_countries: other,
        }
    }

    #[test]
    fn selects_country_values_with_the_markup() {
        let tables = tables();
        let query = country_query("7601", "China");
        let selected = default_value(&query, period(2026), &tables.default_values_annex_i).unwrap();
        assert_eq!(selected.base, fixed("3.000"));
        assert_eq!(selected.markup, fixed("0.10"));
        assert_eq!(selected.applied, fixed("3.300"));
        assert_eq!(
            selected.source,
            DefaultSource::Country {
                cn_code: String::from("7601"),
                country: String::from("China"),
            }
        );

        let later = default_value(&query, period(2027), &tables.default_values_annex_i).unwrap();
        assert_eq!(later.applied, fixed("3.600"));
        let latest = default_value(&query, period(2028), &tables.default_values_annex_i).unwrap();
        assert_eq!(latest.applied, fixed("3.900"));
    }

    #[test]
    fn accepts_spaced_and_specific_codes() {
        let tables = tables();
        let query = country_query("7604 10 90", "China");
        let selected = default_value(&query, period(2026), &tables.default_values_annex_i).unwrap();
        assert_eq!(selected.base, fixed("4.881"));
        assert_eq!(selected.applied, fixed("5.3691"));
        assert_eq!(
            selected.source,
            DefaultSource::Country {
                cn_code: String::from("76041090"),
                country: String::from("China"),
            }
        );
    }

    #[test]
    fn resolves_shorter_codes_within_the_country() {
        let tables = tables();
        let query = country_query("76109099", "China");
        let selected = default_value(&query, period(2026), &tables.default_values_annex_i).unwrap();
        assert_eq!(selected.base, fixed("4.896"));
        assert_eq!(
            selected.source,
            DefaultSource::Country {
                cn_code: String::from("761090"),
                country: String::from("China"),
            }
        );
    }

    #[test]
    fn falls_back_to_other_countries() {
        let tables = tables();
        let query = country_query("7601", "Albania");
        let selected = default_value(&query, period(2026), &tables.default_values_annex_i).unwrap();
        assert_eq!(selected.base, fixed("2.203"));
        assert_eq!(selected.applied, fixed("2.4233"));
        assert_eq!(
            selected.source,
            DefaultSource::OtherCountries {
                cn_code: String::from("7601"),
            }
        );
    }

    #[test]
    fn falls_back_when_the_country_row_shows_no_value() {
        let annex_i = synthetic(
            vec![row("7601", "Ruritania", None)],
            vec![row(
                "7601",
                "Other Countries and Territories",
                Some("2.203"),
            )],
        );
        let query = country_query("7601", "Ruritania");
        let selected = default_value(&query, period(2026), &annex_i).unwrap();
        assert_eq!(selected.base, fixed("2.203"));
        assert_eq!(
            selected.source,
            DefaultSource::OtherCountries {
                cn_code: String::from("7601"),
            }
        );
    }

    #[test]
    fn forwards_the_sector_to_the_markup_schedule() {
        let tables = tables();
        let query = DefaultQuery {
            cn_code: "7601",
            country: "China",
            sector: Sector::Fertilisers,
        };
        let selected = default_value(&query, period(2026), &tables.default_values_annex_i).unwrap();
        assert_eq!(selected.applied, fixed("3.030"));
    }

    #[test]
    fn handles_unknown_origin_precursors() {
        let tables = tables();
        let selected = precursor_default_value(
            "7601",
            Sector::Aluminium,
            period(2026),
            &tables.default_values_annex_iv,
        )
        .unwrap();
        assert_eq!(selected.base, fixed("3.198"));
        assert_eq!(selected.applied, fixed("3.5178"));
        assert_eq!(
            selected.source,
            DefaultSource::AnnexIv {
                cn_code: String::from("7601"),
            }
        );

        let prefix = precursor_default_value(
            "76109099",
            Sector::Aluminium,
            period(2027),
            &tables.default_values_annex_iv,
        )
        .unwrap();
        assert_eq!(prefix.base, fixed("4.896"));
        assert_eq!(prefix.applied, fixed("5.8752"));
        assert_eq!(
            prefix.source,
            DefaultSource::AnnexIv {
                cn_code: String::from("761090"),
            }
        );
    }

    #[test]
    fn rejects_malformed_cn_codes() {
        let tables = tables();
        for text in ["", "76", "760", "76011", "760410101", "7604-10", "ABCD"] {
            let query = country_query(text, "China");
            assert_eq!(
                default_value(&query, period(2026), &tables.default_values_annex_i),
                Err(RuleError::InvalidCnCode),
                "{text:?}"
            );
        }
    }

    #[test]
    fn reports_missing_defaults() {
        let tables = tables();
        for text in ["9999", "76169999"] {
            let query = country_query(text, "China");
            assert_eq!(
                default_value(&query, period(2026), &tables.default_values_annex_i),
                Err(RuleError::NoDefaultValue),
                "{text:?}"
            );
        }
        assert_eq!(
            precursor_default_value(
                "9999",
                Sector::Aluminium,
                period(2026),
                &tables.default_values_annex_iv,
            ),
            Err(RuleError::NoDefaultValue)
        );
    }
}
