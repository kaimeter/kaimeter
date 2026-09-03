// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Registry bridge (R15/R36): bulk structured import of customs/broker H1
//! (SAD) XML/CSV exports straight into consignment records, operator-ID
//! mapping, and offline EORI/VIES format validation.

use serde::{Deserialize, Serialize};

use crate::customs::{classify, counts_toward_net_mass, CbamStatus};
use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// SAD/H1 bulk import (R15)
// ---------------------------------------------------------------------------

/// One parsed row of a SAD/H1 export (single administrative document).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SadRow {
    /// Box 33 — commodity CN code (8 digits; whitespace tolerated).
    pub cn_code: String,
    /// Box 35 — net mass, kg.
    pub net_mass_kg: f64,
    /// Box 37 — customs procedure code (e.g. `40 00`, `71 00`).
    pub procedure_code: String,
    /// Box 15 — country of origin, ISO-3166 alpha-2.
    pub country_of_origin: String,
    /// Box 40 — acceptance/clearance date, ISO `YYYY-MM-DD`.
    pub clearance_date: String,
    /// Additional code text when present (e.g. the F-family marker).
    pub additional_code: Option<String>,
}

/// A consignment record derived from a SAD row, with the Box 37 rule
/// engine's classification attached (users never re-key data Kaimeter
/// already holds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedImport {
    /// The parsed SAD row.
    pub row: SadRow,
    /// The Box 37 classification (`customs::classify`).
    pub status: CbamStatus,
    /// True when this row counts toward the 50 t de-minimis net mass.
    pub counts_toward_net_mass: bool,
}

/// Parse an SAD/H1 XML export into rows. Box numbers are matched on their
/// standard SAD XML element names (`GoodsItemNetMass`, `CommodityCode`,
/// `AdditionalProcedure`, `CountryOfOrigin`, `AcceptanceDate` families) with
/// tolerant whitespace; malformed rows raise [`DomainError::RegistryParseError`].
///
/// # Errors
///
/// [`DomainError::RegistryParseError`] on malformed XML or non-numeric
/// masses/dates.
pub fn parse_sad_xml(xml: &str) -> Result<Vec<SadRow>, DomainError> {
    let mut rows = Vec::new();
    let mut rest = xml;
    while let Some((content_start, self_closing)) = find_open(rest, "GoodsItem", 0) {
        if self_closing {
            rest = &rest[content_start..];
            continue;
        }
        let close_abs = find_close(rest, "GoodsItem", content_start).ok_or_else(|| {
            DomainError::RegistryParseError(
                "malformed XML: unterminated <GoodsItem> element".to_string(),
            )
        })?;
        let block = &rest[content_start..close_abs];
        rows.push(goods_item_row(block)?);
        rest = &rest[close_abs..];
    }
    Ok(rows)
}

/// Parse an SAD/H1 CSV export into rows. Expected header:
/// `cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date[,additional_code]`.
///
/// # Errors
///
/// [`DomainError::RegistryParseError`] on malformed rows.
pub fn parse_sad_csv(csv: &str) -> Result<Vec<SadRow>, DomainError> {
    let mut lines: Vec<&str> = csv.split('\n').collect();
    // Skip a single trailing newline (the common export shape); a second
    // trailing newline leaves a blank data row, which is malformed.
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let Some(header_line) = lines.first() else {
        return Err(DomainError::RegistryParseError(
            "missing header row: expected \
             cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date\
             [,additional_code]"
                .to_string(),
        ));
    };
    let header: Vec<String> = header_line
        .split(',')
        .map(str::trim)
        .map(String::from)
        .collect();
    let base = [
        "cn_code",
        "net_mass_kg",
        "procedure_code",
        "country_of_origin",
        "clearance_date",
    ];
    let expected5: Vec<String> = base.iter().map(|s| (*s).to_string()).collect();
    let mut expected6 = expected5.clone();
    expected6.push("additional_code".to_string());
    if header != expected5 && header != expected6 {
        return Err(DomainError::RegistryParseError(format!(
            "row 1: bad header `{header_line}`: expected \
             cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date\
             [,additional_code]"
        )));
    }
    let has_additional = header.len() == 6;

    let mut rows = Vec::new();
    for (idx, line) in lines.iter().enumerate().skip(1) {
        let row_no = idx + 1; // 1-based; the header is row 1
        let row_err =
            |detail: String| DomainError::RegistryParseError(format!("row {row_no}: {detail}"));
        let cells: Vec<&str> = line.split(',').map(str::trim).collect();
        if cells.len() != header.len() {
            return Err(row_err(format!(
                "expected {} columns, found {}",
                header.len(),
                cells.len()
            )));
        }
        let cn_code = normalize_cn("cn_code", cells[0]).map_err(row_err)?;
        let net_mass_kg = parse_mass("net_mass_kg", cells[1]).map_err(row_err)?;
        if cells[2].is_empty() {
            return Err(row_err(
                "missing required field: procedure_code".to_string(),
            ));
        }
        let procedure_code = cells[2].to_string();
        let country_of_origin = normalize_origin("country_of_origin", cells[3]).map_err(row_err)?;
        let clearance_date = normalize_date("clearance_date", cells[4]).map_err(row_err)?;
        let additional_code = if has_additional {
            Some(cells[5]).filter(|c| !c.is_empty()).map(String::from)
        } else {
            None
        };
        rows.push(SadRow {
            cn_code,
            net_mass_kg,
            procedure_code,
            country_of_origin,
            clearance_date,
            additional_code,
        });
    }
    Ok(rows)
}

/// Run parsed rows through the Box 37 rule engine, marking which rows count
/// toward net-mass tracking. Origin-exempt rows (R43/R45) are excluded by
/// the caller via [`crate::customs::counts_toward_net_mass`] when known.
///
/// # Errors
///
/// [`DomainError::UnknownProcedureCode`] propagation.
pub fn classify_imports(rows: &[SadRow]) -> Result<Vec<ClassifiedImport>, DomainError> {
    rows.iter()
        .map(|row| {
            let status = classify(&row.procedure_code)?;
            Ok(ClassifiedImport {
                row: row.clone(),
                status,
                // Origin exemptions (R43/R45) are applied by the caller when
                // known: the bridge sees only the SAD row, so classification
                // here assumes no exemption claim (origin_exempt = false).
                counts_toward_net_mass: counts_toward_net_mass(status, false),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// SAD field normalization + hand-rolled XML scanning (no xml/csv crates)
// ---------------------------------------------------------------------------

/// Accepted element spellings per SAD box, tried in order.
const CN_TAGS: &[&str] = &["CommodityCode", "GoodsItemCommodityCode"];
const MASS_TAGS: &[&str] = &["GoodsItemNetMass", "NetMass"];
const PROC_TAGS: &[&str] = &["AdditionalProcedure", "ProcedureCode"];
const ORIGIN_TAGS: &[&str] = &["CountryOfOrigin", "OriginCountry"];
const DATE_TAGS: &[&str] = &["AcceptanceDate", "ClearanceDate"];

/// Build one [`SadRow`] from a single `<GoodsItem>` element's inner text.
fn goods_item_row(block: &str) -> Result<SadRow, DomainError> {
    let missing = |field: &str| {
        DomainError::RegistryParseError(format!("missing required <GoodsItem> field: {field}"))
    };
    let cn_raw = extract_field(block, CN_TAGS)
        .ok_or_else(|| missing("CommodityCode/GoodsItemCommodityCode"))?;
    let cn_code = normalize_cn("CommodityCode/GoodsItemCommodityCode", &cn_raw)
        .map_err(DomainError::RegistryParseError)?;
    let mass_raw =
        extract_field(block, MASS_TAGS).ok_or_else(|| missing("GoodsItemNetMass/NetMass"))?;
    let net_mass_kg = parse_mass("GoodsItemNetMass/NetMass", &mass_raw)
        .map_err(DomainError::RegistryParseError)?;
    let procedure_code = extract_field(block, PROC_TAGS)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| missing("AdditionalProcedure/ProcedureCode"))?;
    let origin_raw = extract_field(block, ORIGIN_TAGS)
        .ok_or_else(|| missing("CountryOfOrigin/OriginCountry"))?;
    let country_of_origin = normalize_origin("CountryOfOrigin/OriginCountry", &origin_raw)
        .map_err(DomainError::RegistryParseError)?;
    let date_raw =
        extract_field(block, DATE_TAGS).ok_or_else(|| missing("AcceptanceDate/ClearanceDate"))?;
    let clearance_date = normalize_date("AcceptanceDate/ClearanceDate", &date_raw)
        .map_err(DomainError::RegistryParseError)?;
    let additional_code = extract_field(block, &["AdditionalCode"]).filter(|c| !c.is_empty());
    Ok(SadRow {
        cn_code,
        net_mass_kg,
        procedure_code,
        country_of_origin,
        clearance_date,
        additional_code,
    })
}

/// Find the first `<Tag ...>` opening at or after `from`, returning
/// `(content_start, self_closing)` where `content_start` is the byte offset
/// just past the opening tag's `>`. Minimal by design: the tag name must be
/// preceded by `<` and followed by a name terminator, so `<CommodityCodes>`
/// never matches `<CommodityCode>`; attribute text inside the open tag is
/// skipped over, and nothing else about XML is understood.
fn find_open(source: &str, tag: &str, from: usize) -> Option<(usize, bool)> {
    let bytes = source.as_bytes();
    let mut pos = from;
    while pos <= source.len() {
        let rel = source[pos..].find(tag)?;
        let start = pos + rel;
        let name_end = start + tag.len();
        let preceded = start > 0 && bytes[start - 1] == b'<';
        let followed = match source[name_end..].chars().next() {
            None => true,
            Some(c) => matches!(c, '>' | '/' | ' ' | '\t' | '\r' | '\n'),
        };
        if preceded && followed {
            let gt = source[name_end..].find('>')?;
            let gt_abs = name_end + gt;
            let self_closing = gt > 0 && bytes[gt_abs - 1] == b'/';
            return Some((gt_abs + 1, self_closing));
        }
        pos = name_end;
    }
    None
}

/// Extract the first present element among `tags` from a block, returning
/// its trimmed text content (`None` when none of the spellings occur).
fn extract_field(block: &str, tags: &[&str]) -> Option<String> {
    for tag in tags {
        if let Some((content_start, self_closing)) = find_open(block, tag, 0) {
            if self_closing {
                return Some(String::new());
            }
            if let Some(close_abs) = find_close(block, tag, content_start) {
                return Some(block[content_start..close_abs].trim().to_string());
            }
        }
    }
    None
}

/// Find the first `</Tag>` close at or after `from`. The name terminator is
/// checked so `</GoodsItemNetMass>` never closes `</GoodsItem>`; returns the
/// byte offset of the leading `</`.
fn find_close(source: &str, tag: &str, from: usize) -> Option<usize> {
    let needle = format!("</{tag}");
    let mut pos = from;
    while let Some(rel) = source[pos..].find(needle.as_str()) {
        let start = pos + rel;
        let after = start + needle.len();
        let followed = match source[after..].chars().next() {
            None => true,
            Some(c) => matches!(c, '>' | ' ' | '\t' | '\r' | '\n'),
        };
        if followed {
            return Some(start);
        }
        pos = after;
    }
    None
}

/// Normalize a CN code: strip spaces and dots, then require exactly 8 ASCII
/// digits. Error detail names the offending field.
fn normalize_cn(field: &str, raw: &str) -> Result<String, String> {
    let stripped: String = raw.chars().filter(|c| !matches!(c, ' ' | '.')).collect();
    if stripped.len() == 8 && stripped.bytes().all(|b| b.is_ascii_digit()) {
        Ok(stripped)
    } else {
        Err(format!(
            "malformed {field} `{raw}`: expected 8 digits after stripping spaces/dots"
        ))
    }
}

/// Parse a net-mass figure in kg: whitespace is stripped outright, and
/// thousands-grouped separators (`12,000`, `12.000`) are removed before the
/// numeric parse. Error detail names the offending field.
fn parse_mass(field: &str, raw: &str) -> Result<f64, String> {
    let mut s: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    for sep in [',', '.'] {
        if is_thousands_grouped(&s, sep) {
            s = s.replace(sep, "");
        }
    }
    match s.parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Ok(v),
        _ => Err(format!(
            "malformed {field} `{raw}`: not a valid net mass in kg"
        )),
    }
}

/// True when `s` is a plain thousands-grouped number for separator `sep`
/// (`12,000`, `1.000.000`); decimal fractions like `500.5` do not match, so
/// they survive to the numeric parse untouched.
fn is_thousands_grouped(s: &str, sep: char) -> bool {
    let mut groups = s.split(sep);
    match groups.next() {
        Some(first)
            if (1..=3).contains(&first.len()) && first.bytes().all(|b| b.is_ascii_digit()) => {}
        _ => return false,
    }
    let mut grouped = false;
    for group in groups {
        grouped = true;
        if group.len() != 3 || !group.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    grouped
}

/// Normalize an ISO-3166 alpha-2 origin: trimmed, exactly 2 uppercase ASCII
/// letters. Error detail names the offending field.
fn normalize_origin(field: &str, raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.len() == 2 && s.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "malformed {field} `{raw}`: expected a 2-letter ISO-3166 alpha-2 code"
        ))
    }
}

/// Normalize a clearance/acceptance date to ISO `YYYY-MM-DD`; accepts
/// `YYYY-MM-DD` and compact `YYYYMMDD`. Error detail names the field.
fn normalize_date(field: &str, raw: &str) -> Result<String, String> {
    let bad = || format!("malformed {field} `{raw}`: expected ISO YYYY-MM-DD or compact YYYYMMDD");
    let s = raw.trim();
    if !s.is_ascii() {
        return Err(bad());
    }
    let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    let (y, m, d) = if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        (&s[..4], &s[5..7], &s[8..])
    } else if s.len() == 8 {
        (&s[..4], &s[4..6], &s[6..])
    } else {
        return Err(bad());
    };
    if !(digits(y) && digits(m) && digits(d)) {
        return Err(bad());
    }
    let month: u32 = m.parse().map_err(|_| bad())?;
    let day: u32 = d.parse().map_err(|_| bad())?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad());
    }
    Ok(format!("{y}-{m}-{d}"))
}

// ---------------------------------------------------------------------------
// Operator-ID mapping (R36)
// ---------------------------------------------------------------------------

/// A third-country installation operator's Registry registration record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorRecord {
    /// CBAM Registry operator identifier.
    pub registry_operator_id: String,
    /// The installation the operator record maps to.
    pub installation_id: String,
    /// Registration status (data: `REGISTERED`, `PENDING`, `REVOKED`...).
    pub status: String,
    /// Last refresh date, ISO `YYYY-MM-DD` (stale-flagged when old; R36).
    pub refreshed_iso: String,
}

/// Map a Registry operator record onto an installation so verifier-approved
/// actuals can flow from the operator's registered data.
///
/// # Errors
///
/// [`DomainError::Storage`] when the operator record's installation is
/// empty or the status is not a known token.
pub fn map_operator(
    record: &OperatorRecord,
    installation_id: &str,
) -> Result<OperatorRecord, DomainError> {
    if installation_id.trim().is_empty() {
        return Err(DomainError::Storage(
            "operator mapping requires a non-empty installation id".to_string(),
        ));
    }
    const KNOWN: [&str; 4] = ["REGISTERED", "PENDING", "REVOKED", "WITHDRAWN"];
    if !KNOWN.contains(&record.status.trim()) {
        return Err(DomainError::Storage(format!(
            "unknown operator registration status `{}`: expected one of \
             REGISTERED, PENDING, REVOKED, WITHDRAWN",
            record.status
        )));
    }
    Ok(OperatorRecord {
        installation_id: installation_id.to_string(),
        ..record.clone()
    })
}

// ---------------------------------------------------------------------------
// EORI/VIES offline format validation (R14/R15 build note)
// ---------------------------------------------------------------------------

/// Validate an EORI number offline by format: two-letter country prefix
/// followed by the national-format body (length/alphabet rules cached
/// offline — prevents silent registry rejections at filing time).
///
/// # Errors
///
/// [`DomainError::Storage`] carries the specific format failure.
pub fn validate_eori(eori: &str) -> Result<(), DomainError> {
    let s = eori.trim();
    let invalid = |why: &str| DomainError::Storage(format!("invalid EORI `{eori}`: {why}"));
    if !s.is_ascii() {
        return Err(invalid("must be ASCII alphanumeric"));
    }
    let bytes = s.as_bytes();
    if !(3..=17).contains(&bytes.len()) {
        return Err(invalid("total length must be 3..=17 characters"));
    }
    if !bytes[..2].iter().all(|b| b.is_ascii_uppercase()) {
        return Err(invalid(
            "must start with a two-letter uppercase ISO country prefix",
        ));
    }
    let body = &s[2..];
    match &s[..2] {
        // Pinned national format: DE = "DE" + 8..=9 digits.
        "DE" => {
            let ok = (8..=9).contains(&body.len()) && body.bytes().all(|b| b.is_ascii_digit());
            if !ok {
                return Err(invalid("DE EORI must be `DE` followed by 8..=9 digits"));
            }
        }
        // Pinned national format, kept lenient: FR = "FR" + 11 alphanumeric
        // (positions 3-4 may be letters or digits; no deeper structure is
        // enforced offline).
        "FR" => {
            let ok = body.len() == 11 && body.bytes().all(|b| b.is_ascii_alphanumeric());
            if !ok {
                return Err(invalid(
                    "FR EORI must be `FR` followed by 11 alphanumeric characters",
                ));
            }
        }
        // Generic rule: 2-letter prefix + 1..=15 alphanumeric body. The
        // offline format cache pins the big member states; every other
        // prefix passes the generic rule (offline validation only — the
        // authoritative check happens at the NCA/Registry, R14 build note).
        _ => {
            let ok = !body.is_empty()
                && body.len() <= 15
                && body.bytes().all(|b| b.is_ascii_alphanumeric());
            if !ok {
                return Err(invalid("EORI body must be 1..=15 alphanumeric characters"));
            }
        }
    }
    Ok(())
}

/// Validate a VAT identification number offline by format (VIES structure
/// rules; the live check is a sync-time feature, never a launch assumption).
///
/// # Errors
///
/// [`DomainError::Storage`] carries the specific format failure.
pub fn validate_vies_format(vat: &str) -> Result<(), DomainError> {
    let s = vat.trim();
    let invalid = |why: &str| DomainError::Storage(format!("invalid VAT format `{vat}`: {why}"));
    if !s.is_ascii() {
        return Err(invalid("must be ASCII alphanumeric"));
    }
    let bytes = s.as_bytes();
    if !(4..=19).contains(&bytes.len()) {
        return Err(invalid("total length must be 4..=19 characters"));
    }
    if !bytes[..2].iter().all(|b| b.is_ascii_uppercase()) {
        return Err(invalid(
            "must start with a two-letter uppercase ISO country prefix",
        ));
    }
    let body = &s[2..];
    match &s[..2] {
        // Pinned: DE = "DE" + exactly 9 digits ("DE136695976" style).
        "DE" => {
            let ok = body.len() == 9 && body.bytes().all(|b| b.is_ascii_digit());
            if !ok {
                return Err(invalid("DE VAT must be `DE` followed by 9 digits"));
            }
        }
        // Pinned: NL = "NL" + 12 characters — 9 alphanumeric, then "B", then
        // 2 digits ("NL004495445B01").
        "NL" => {
            let ok = body.len() == 12
                && body[..9].bytes().all(|b| b.is_ascii_alphanumeric())
                && body.as_bytes()[9] == b'B'
                && body[10..].bytes().all(|b| b.is_ascii_digit());
            if !ok {
                return Err(invalid(
                    "NL VAT must be `NL` followed by 10 characters ending `B` + 2 digits",
                ));
            }
        }
        // Generic rule: 2-letter prefix + 2..=17 alphanumeric body.
        _ => {
            let ok =
                (2..=17).contains(&body.len()) && body.bytes().all(|b| b.is_ascii_alphanumeric());
            if !ok {
                return Err(invalid("VAT body must be 2..=17 alphanumeric characters"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customs::classify;

    #[test]
    fn classify_is_reusable_from_customs_core() {
        // The frozen Box 37 table is shared, not duplicated.
        assert_eq!(classify("40 00").expect("4000"), CbamStatus::Liable);
    }

    /// Whitespace/dot tolerance on CN codes, thousands separators on mass,
    /// and compact YYYYMMDD dates all normalize (R15).
    #[test]
    fn xml_alternate_spellings_normalize() {
        let xml = "<GoodsItem><GoodsItemCommodityCode>76 04.10 10</GoodsItemCommodityCode>\
                   <NetMass>12,000</NetMass><ProcedureCode>40 00</ProcedureCode>\
                   <OriginCountry>IN</OriginCountry><ClearanceDate>20260401</ClearanceDate>\
                   <AdditionalCode>F51</AdditionalCode></GoodsItem>";
        let rows = parse_sad_xml(xml).expect("parse");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cn_code, "76041010");
        assert_eq!(rows[0].net_mass_kg, 12_000.0);
        assert_eq!(rows[0].clearance_date, "2026-04-01");
        assert_eq!(rows[0].additional_code.as_deref(), Some("F51"));
    }

    /// Empty input is an empty batch, not an error.
    #[test]
    fn xml_empty_input_yields_empty_vec() {
        assert!(parse_sad_xml("").expect("empty").is_empty());
        assert!(parse_sad_xml("<Declaration></Declaration>")
            .expect("no items")
            .is_empty());
    }

    /// Parse failures name the offending field (RegistryParseError, R15).
    #[test]
    fn xml_malformed_fields_name_the_field() {
        let bad_cn = "<GoodsItem><CommodityCode>73X181500</CommodityCode>\
                      <NetMass>1</NetMass><AdditionalProcedure>40 00</AdditionalProcedure>\
                      <CountryOfOrigin>CN</CountryOfOrigin><AcceptanceDate>2026-03-15</AcceptanceDate>\
                      </GoodsItem>";
        match parse_sad_xml(bad_cn) {
            Err(DomainError::RegistryParseError(detail)) => {
                assert!(detail.contains("CommodityCode"), "{detail}");
            }
            other => panic!("expected RegistryParseError, got {other:?}"),
        }
        let bad_date = "<GoodsItem><CommodityCode>73181500</CommodityCode>\
                        <NetMass>1</NetMass><AdditionalProcedure>40 00</AdditionalProcedure>\
                        <CountryOfOrigin>CN</CountryOfOrigin><AcceptanceDate>15/03/2026</AcceptanceDate>\
                        </GoodsItem>";
        match parse_sad_xml(bad_date) {
            Err(DomainError::RegistryParseError(detail)) => {
                assert!(detail.contains("AcceptanceDate"), "{detail}");
            }
            other => panic!("expected RegistryParseError, got {other:?}"),
        }
    }

    /// CSV: a single trailing newline is skipped; an extra blank line is a
    /// malformed row naming its 1-based number.
    #[test]
    fn csv_trailing_newline_and_blank_rows() {
        let one_row = concat!(
            "cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date\n",
            "73181500,12000,40 00,CN,2026-03-15\n",
        );
        assert_eq!(parse_sad_csv(one_row).expect("parse").len(), 1);
        assert_eq!(parse_sad_csv(one_row).expect("no newline").len(), 1);

        let blank_row = concat!(
            "cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date\n",
            "73181500,12000,40 00,CN,2026-03-15\n",
            "\n",
        );
        match parse_sad_csv(blank_row) {
            Err(DomainError::RegistryParseError(detail)) => {
                assert!(detail.contains("row 3"), "{detail}");
            }
            other => panic!("expected RegistryParseError, got {other:?}"),
        }
    }

    /// EORI/VIES offline validation never blocks unknown prefixes that pass
    /// the generic rule (the format cache pins only the big states).
    #[test]
    fn eori_vies_generic_rules() {
        assert!(validate_eori("NL123456789").is_ok(), "NL via generic rule");
        assert!(validate_eori(" PL123456780AB ").is_ok(), "trimmed input");
        assert!(validate_eori("").is_err());
        assert!(validate_eori("DE12345678").is_ok());
        assert!(validate_vies_format("").is_err());
        assert!(validate_vies_format("NL004495445B01").is_ok());
    }
}
