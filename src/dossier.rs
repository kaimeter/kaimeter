// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Dossier assembly (R23): the three document classes a full CBAM dossier
//! requires, with completeness flags, plus the sub-installation heat
//! (Q_net) and waste-gas balance tables (R35) and 数电发票 (e-fapiao)
//! XML-first parsing for the energy class (R23/0.9.0).

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::types::{Completeness, Consignment, Dossier, DossierClass};

// ---------------------------------------------------------------------------
// Three-class document set (R23)
// ---------------------------------------------------------------------------

/// A scrap record (R23): scrap carries zero embedded emissions today; the
/// pre-consumer/post-consumer split and any future loading (COM(2025)989
/// proposes pre-consumer emissions from 2028) is modeled as data, never
/// hardcoded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrapRecord {
    /// Scrap consumed, tonnes per tonne of product.
    pub tonnes_per_t_product: f64,
    /// Pre-consumer share, percent (a mandatory reporting parameter).
    pub pre_consumer_share_pct: f64,
    /// Embedded emissions of this scrap class, tCO2e per tonne (0.0 today;
    /// a future data patch loads pre-consumer emissions here from 2028).
    pub embedded_tco2e_per_t: f64,
}

/// Assemble a dossier for a consignment and report completeness across the
/// three mandatory classes (energy & fuel bills, raw-material invoices +
/// mill test certificates incl. scrap records, production output + customs
/// records).
#[must_use]
pub fn assemble(consignment: Consignment) -> Dossier {
    Dossier::new(consignment)
}

/// The completeness report, in reporting order (R23: a dossier is only
/// complete with all three sets; the wizard flags whichever class is
/// missing).
#[must_use]
pub fn completeness(dossier: &Dossier) -> Completeness {
    dossier.completeness()
}

/// The i18n message key for a missing class (wizard flags).
#[must_use]
pub fn missing_class_key(class: DossierClass) -> &'static str {
    match class {
        DossierClass::EnergyFuel => "dossier.missing.energy_fuel",
        DossierClass::Materials => "dossier.missing.materials",
        DossierClass::Production => "dossier.missing.production",
    }
}

// ---------------------------------------------------------------------------
// 数电发票 (e-fapiao) XML-first parsing (R23) — agent-owned
// ---------------------------------------------------------------------------

/// Fields extracted from a 数电发票 structured XML (electricity settlement).
/// The signed XML carries tax-authority metadata, so parsing is deterministic
/// and OCR is skipped entirely when the XML is present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EFapiaoFields {
    /// Invoice number (发票号码).
    pub invoice_number: String,
    /// Issue date, ISO `YYYY-MM-DD`.
    pub issue_date: String,
    /// Seller name (the mill/utility).
    pub seller_name: String,
    /// Electricity quantity, kWh.
    pub electricity_kwh: f64,
    /// Total amount in CNY (pre-tax).
    pub amount_cny: f64,
    /// True when the XML carries the tax-authority signature metadata.
    pub tax_authority_signed: bool,
}

/// Parse a 数电发票 (e-fapiao) structured XML document into its fields
/// (R23, XML-first: when the signed structured XML is present, OCR is
/// skipped entirely).
///
/// The extractor is a hand-rolled, minimal scanner — no XML dependency. It
/// finds elements by tag name, tolerates insignificant whitespace and XML
/// comments, and handles the nested `Items`/`Item` structure. Multiple
/// `Item` entries are summed into one electricity quantity and one total
/// amount. `tax_authority_signed` is true only when the
/// `TaxAuthoritySignature` element carries `present="true"` (a missing
/// element means unsigned).
///
/// # Errors
///
/// [`DomainError::RegistryParseError`] when the document is empty or
/// malformed, when `InvoiceNumber`, `IssueDate` or `SellerName` is missing
/// or empty, when `IssueDate` is not an ISO `YYYY-MM-DD` date, or when an
/// item's `Quantity` or `Amount` is missing or not a finite number.
pub fn parse_efapiao_xml(xml: &str) -> Result<EFapiaoFields, DomainError> {
    let fail = |detail: String| DomainError::RegistryParseError(format!("e-fapiao XML: {detail}"));

    // XML comments carry no data — strip them so `<Tag><!-- c -->text</Tag>`
    // and commented-out duplicates cannot confuse the scanner (R23/0.9.0).
    let xml = strip_comments(xml);

    let invoice_number = required_text(&xml, "InvoiceNumber")?;
    let issue_date = required_text(&xml, "IssueDate")?;
    if !is_iso_ymd(&issue_date) {
        return Err(fail(format!(
            "IssueDate must be ISO YYYY-MM-DD, got {issue_date:?}"
        )));
    }
    let seller_name = required_text(&xml, "SellerName")?;

    // Sum every <Item> inside <Items> (two records for split settlements).
    let items_section =
        items_section(&xml).ok_or_else(|| fail("missing <Items> section".to_string()))?;
    let mut electricity_kwh = 0.0;
    let mut amount_cny = 0.0;
    for (n, item) in item_texts(&items_section).into_iter().enumerate() {
        let quantity = element_text(&item, "Quantity")
            .ok_or_else(|| fail(format!("item {n}: missing <Quantity>")))?;
        let quantity = parse_number(&quantity, &format!("item {n} Quantity")).map_err(fail)?;
        let amount = element_text(&item, "Amount")
            .ok_or_else(|| fail(format!("item {n}: missing <Amount>")))?;
        let amount = parse_number(&amount, &format!("item {n} Amount")).map_err(fail)?;
        electricity_kwh += quantity;
        amount_cny += amount;
    }

    // Tax-authority signature metadata (missing element => unsigned).
    let tax_authority_signed = element_attrs(&xml, "TaxAuthoritySignature")
        .and_then(|attrs| attr_value(&attrs, "present"))
        .is_some_and(|present| present.trim() == "true");

    Ok(EFapiaoFields {
        invoice_number,
        issue_date,
        seller_name,
        electricity_kwh,
        amount_cny,
        tax_authority_signed,
    })
}

/// Remove `<!-- ... -->` comments from an XML document (an unterminated
/// comment swallows the rest of the input, as in real parsers).
fn strip_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    loop {
        match rest.find("<!--") {
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest[start + 4..].find("-->") {
                    Some(end) => rest = &rest[start + 4 + end + 3..],
                    // Unterminated comment: the remainder is comment.
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

/// Locate the open tag `<tag ...>` at or after `from`, tolerating other
/// elements whose names merely start with `tag` (e.g. `<Items>` vs `<Item>`).
///
/// Returns `(open_start, attrs_start, after_open_gt)` — the byte offsets of
/// the `<`, the attribute region, and the first byte after the closing `>`
/// of the open tag.
fn find_open_tag(xml: &str, tag: &str, from: usize) -> Option<(usize, usize, usize)> {
    let mut from = from;
    loop {
        let candidate = xml[from..].find(&format!("<{tag}"))? + from;
        let attrs_start = candidate + 1 + tag.len();
        // The next byte must end the tag name — this rejects `<Items>`
        // when searching for `<Item>` and vice versa.
        match xml.as_bytes().get(attrs_start) {
            Some(b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') => {
                let gt = candidate + xml[candidate..].find('>')?;
                return Some((candidate, attrs_start, gt + 1));
            }
            _ => from = attrs_start,
        }
    }
}

/// The trimmed text content of the first element named `tag`
/// (`<tag>text</tag>`); a self-closing `<tag/>` yields `None`.
fn element_text(xml: &str, tag: &str) -> Option<String> {
    let (_, _, content_start) = find_open_tag(xml, tag, 0)?;
    // Self-closing elements carry no text.
    if xml[..content_start].trim_end().ends_with("/>") {
        return None;
    }
    let close = content_start + xml[content_start..].find(&format!("</{tag}>"))?;
    Some(xml[content_start..close].trim().to_string())
}

/// The trimmed text of the first element named `tag`, rejecting empty text.
fn required_text(xml: &str, tag: &str) -> Result<String, DomainError> {
    element_text(xml, tag)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| DomainError::RegistryParseError(format!("e-fapiao XML: missing <{tag}>")))
}

/// The raw attribute region of the first element named `tag` (the text
/// between the tag name and the closing `>` of its open tag).
fn element_attrs(xml: &str, tag: &str) -> Option<String> {
    let (_, attrs_start, after_gt) = find_open_tag(xml, tag, 0)?;
    let gt = after_gt - 1;
    Some(xml[attrs_start..gt].to_string())
}

/// The value of attribute `name` inside a raw attribute region, single- or
/// double-quoted.
fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=");
    let idx = attrs.find(&pattern)? + pattern.len();
    let rest = attrs[idx..].trim_start();
    let quote = *rest.as_bytes().first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let quote = char::from(quote);
    let end = rest[1..].find(quote)?;
    Some(rest[1..1 + end].to_string())
}

/// The inner XML of the first `<Items>` element (the line-item section).
fn items_section(xml: &str) -> Option<String> {
    let (_, _, content_start) = find_open_tag(xml, "Items", 0)?;
    let close = content_start + xml[content_start..].find("</Items>")?;
    Some(xml[content_start..close].to_string())
}

/// The inner XML of every `<Item>` element inside a `<Items>` section.
fn item_texts(items_xml: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut from = 0;
    while let Some((_, _, content_start)) = find_open_tag(items_xml, "Item", from) {
        let Some(close_rel) = items_xml[content_start..].find("</Item>") else {
            break;
        };
        let close = content_start + close_rel;
        items.push(items_xml[content_start..close].to_string());
        from = close + "</Item>".len();
    }
    items
}

/// Parse a decimal number from an element's text, rejecting non-numeric and
/// non-finite values as registry parse failures.
fn parse_number(text: &str, field: &str) -> Result<f64, String> {
    let parsed = text
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{field}: not a finite number: {text:?}"))?;
    Ok(parsed)
}

/// True when `s` is a valid-looking ISO `YYYY-MM-DD` calendar date.
fn is_iso_ymd(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let (y, m, d) = (parts[0], parts[1], parts[2]);
    let all_digits = y
        .bytes()
        .chain(m.bytes())
        .chain(d.bytes())
        .all(|b| b.is_ascii_digit());
    all_digits
        && y.len() == 4
        && m.len() == 2
        && d.len() == 2
        && matches!(m.parse::<u32>(), Ok(1..=12))
        && matches!(d.parse::<u32>(), Ok(1..=31))
}

// ---------------------------------------------------------------------------
// Sub-installation heat & waste-gas balance (R35) — agent-owned
// ---------------------------------------------------------------------------

/// One metered heat transfer between sub-installations (R35, Annex IV Sec 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatFlow {
    /// Emitting sub-installation identifier.
    pub source: String,
    /// Receiving sub-installation identifier.
    pub destination: String,
    /// Net measurable heat, MWh (Q_net).
    pub q_net_mwh: f64,
    /// Emissions attributed to the transferred heat, tCO2e.
    pub attributed_tco2e: f64,
    /// Whether the flow is metered (unmetered estimates must be flagged).
    pub metered: bool,
}

/// One waste-gas transfer between sub-installations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WasteGasFlow {
    /// Emitting sub-installation identifier.
    pub source: String,
    /// Receiving sub-installation identifier.
    pub destination: String,
    /// Waste-gas quantity, kNm³.
    pub volume_knm3: f64,
    /// Emissions attributed to the transferred gas, tCO2e.
    pub attributed_tco2e: f64,
}

/// The balance table a complex installation's dossier carries (R35): every
/// flow with per-flow source, destination, and metered quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BalanceTable {
    /// Heat transfers between sub-installations.
    pub heat_flows: Vec<HeatFlow>,
    /// Waste-gas transfers between sub-installations.
    pub waste_gas_flows: Vec<WasteGasFlow>,
}

/// Reconcile the balance (R35, Reg (EU) 2023/956 Annex IV Sec 3; Guidance
/// Doc 3 §4.3): attributed emissions across transfers must cancel exactly —
/// what one sub-installation exports, the other imports; nothing
/// double-counted, nothing omitted.
///
/// Sign convention: one physical flow is TWO records — the exporting side
/// records `+attributed_tco2e`, the importing side `−attributed_tco2e`.
/// The residual is `|Σ heat_flows.attributed_tco2e + Σ
/// waste_gas_flows.attributed_tco2e|`; the caller asserts it is `0.0`.
///
/// # Errors
///
/// [`DomainError::Storage`] carries `balance residual {r} tCO2e` when the
/// residual exceeds `1e-9` tCO2e.
pub fn reconcile_balance(table: &BalanceTable) -> Result<f64, DomainError> {
    let heat: f64 = table.heat_flows.iter().map(|f| f.attributed_tco2e).sum();
    let gas: f64 = table
        .waste_gas_flows
        .iter()
        .map(|f| f.attributed_tco2e)
        .sum();
    let residual = (heat + gas).abs();
    if residual > 1e-9 {
        return Err(DomainError::Storage(format!(
            "balance residual {residual} tCO2e"
        )));
    }
    Ok(residual)
}

/// Indices of the balance table's flows that lack a meter reading (R35: the
/// table carries per-flow metered quantity — unmetered estimates must be
/// flagged, never passed off as measured).
///
/// Indexing: heat flows come first (`0..heat_flows.len()`); waste-gas flows
/// continue the numbering (`heat_flows.len()..`). Waste-gas flows carry no
/// `metered` field in the source data, so they are NEVER listed here — only
/// heat flows can be unmetered.
#[must_use]
pub fn unmetered_flows(table: &BalanceTable) -> Vec<usize> {
    table
        .heat_flows
        .iter()
        .enumerate()
        .filter(|(_, flow)| !flow.metered)
        .map(|(index, _)| index)
        .collect()
}
