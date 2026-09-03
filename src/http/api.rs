// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! The JSON API the wizard calls (`/api/...`, the wizard ↔ core
//! integration contract). Handlers are async but hit storage synchronously (the backend
//! is behind a mutex); domain logic is reused from the domain modules, never
//! re-implemented.
//!
//! Error shape (all handlers): `{"error": {"key": <i18n key>, "message":
//! <Display>}}` — 400 for domain/validation failures, 409 for a missing ETS
//! price, 500 for storage failures. Transport-level problems (bad JSON body,
//! missing query parameter) use the stable keys `api.error.invalid_json` and
//! `api.error.missing_param` at 400.

use std::collections::BTreeMap;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::calendar::{self, Quarter};
use crate::compliance::{
    assess_holding, shortfall_tco2e, AlertLevel, AlertThresholds, HoldingPosition, HOLDING_SHARE,
};
use crate::customs::{self, CbamStatus};
use crate::db::Storage;
use crate::domain::errors::DomainError;
use crate::domain::lookup::Lookup;
use crate::domain::types::Consignment;
use crate::export::{
    apply_masks, build_declaration, preflight_validate, preview, DeclarationField, FieldMask,
    SchemaEntry, REQUIRED_DECLARATION_FIELDS,
};
use crate::math::{
    cbam_factor, consignment_emissions_default, gross_exposure, net_exposure, DeMinimisTracker,
    Formula, DE_MINIMIS_THRESHOLD_TONNES,
};
use crate::provenance::{self, AuditEvent};
use crate::roles::{self, Role, RoleSelection};
use crate::state::AppState;
use crate::store;
use crate::validate::{validate_consignment, Severity};

/// Schema version tag the API validates declaration exports against (R30;
/// the official registry schemas remain a 1.0 verification item).
const DECLARATION_SCHEMA_VERSION: &str = "2027.1";

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the `/api` sub-router (mounted by `crate::http::router`).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reference/cn-codes", get(reference_cn_codes))
        .route("/reference/defaults", get(reference_defaults))
        .route(
            "/consignments",
            get(list_consignments_api).post(create_consignment),
        )
        .route("/consignments/import-sad", post(import_sad))
        .route("/deminimis", get(deminimis))
        .route("/exposure", get(exposure))
        .route("/calendar", get(calendar))
        .route("/holding", get(holding))
        .route(
            "/role",
            get(get_role)
                .put(put_role)
                .patch(patch_role)
                .delete(delete_role),
        )
        .route("/export/declaration", post(export_declaration))
        .route("/audit", get(audit_api))
        .route("/attachments", post(create_attachment))
        .route("/price", get(get_price_api).put(put_price))
}

// ---------------------------------------------------------------------------
// Error surface
// ---------------------------------------------------------------------------

/// One API error: HTTP status plus the localized-message envelope.
struct ApiError {
    status: StatusCode,
    key: String,
    message: String,
}

impl ApiError {
    fn bad_request(key: &str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            key: key.to_string(),
            message: message.into(),
        }
    }

    /// A required query parameter is absent.
    fn missing_param(name: &str) -> Self {
        Self::bad_request(
            "api.error.missing_param",
            format!("missing required query parameter `{name}`"),
        )
    }

    /// The JSON body could not be parsed.
    fn invalid_json(detail: String) -> Self {
        Self::bad_request(
            "api.error.invalid_json",
            format!("invalid JSON body: {detail}"),
        )
    }

    /// No ETS price is available anywhere (409: the projection refuses to
    /// guess — R7).
    fn no_price() -> Self {
        Self {
            status: StatusCode::CONFLICT,
            key: DomainError::InvalidEtsPrice(0.0).i18n_key().to_string(),
            message: "no ETS price available: pass `ets_price` or configure the price cache"
                .to_string(),
        }
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        let key = err.i18n_key();
        match &err {
            // Backend failures are transport-internal: 500, never 400.
            DomainError::Storage(_) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                key: key.to_string(),
                message: err.to_string(),
            },
            _ => Self::bad_request(key, err.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"error": {"key": self.key, "message": self.message}})),
        )
            .into_response()
    }
}

type ApiResult = Result<Response, ApiError>;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Current UTC time as ISO-8601 `YYYY-MM-DDTHH:MM:SSZ` (audit timestamps),
/// computed through the calendar's frozen day core — no clock dependency
/// beyond the Unix epoch.
fn now_iso_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = calendar::civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Record one audit event for an API-driven action (actor `api`, payload
/// digest over the canonical JSON of the change).
fn audit(
    storage: &dyn Storage,
    action: &str,
    subject: &str,
    payload: &str,
) -> Result<(), DomainError> {
    store::append_audit(
        storage,
        &now_iso_utc(),
        "api",
        action,
        subject,
        &store::payload_hash(payload),
    )
    .map(|_| ())
}

/// Parse a JSON body extracted as raw bytes (keeps the error envelope in the
/// API's own shape instead of axum's rejection defaults).
fn parse_body<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|e| ApiError::invalid_json(e.to_string()))
}

/// The CBAM status token for the `consignments.status` column (the CHECK
/// constraint in migration 0003 is the authority: `LIABLE`, `DEFERRED`,
/// `IPR_TRACKED`, `OPR_TRACKED`, `EXCLUDED`).
fn status_token(status: CbamStatus) -> String {
    serde_json::to_string(&status)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

/// A required, parseable query parameter.
fn query_i32(q: &BTreeMap<String, String>, name: &str) -> Result<i32, ApiError> {
    let raw = q.get(name).ok_or_else(|| ApiError::missing_param(name))?;
    raw.trim().parse::<i32>().map_err(|_| {
        ApiError::bad_request(
            "api.error.missing_param",
            format!("query parameter `{name}` must be an integer, got `{raw}`"),
        )
    })
}

// ---------------------------------------------------------------------------
// Reference data
// ---------------------------------------------------------------------------

/// `GET /api/reference/cn-codes` — the seeded CN catalog.
async fn reference_cn_codes(State(state): State<AppState>) -> ApiResult {
    let lookup = Lookup::from_storage(state.storage().as_ref()).map_err(ApiError::from)?;
    let codes: Vec<Value> = lookup
        .cn_codes()
        .iter()
        .map(|cn| {
            json!({
                "code": cn.code(),
                "description": cn.description(),
                "sector": cn.sector().as_str(),
            })
        })
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(codes))).into_response())
}

/// `GET /api/reference/defaults?cn=73181500` — the default values registered
/// for one CN code (all production routes).
async fn reference_defaults(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> ApiResult {
    let cn = q.get("cn").ok_or_else(|| ApiError::missing_param("cn"))?;
    let lookup = Lookup::from_storage(state.storage().as_ref()).map_err(ApiError::from)?;
    if lookup.cn_code(cn).is_none() {
        return Err(ApiError::from(DomainError::NoDefaultForCnCode(cn.clone())));
    }
    let defaults: Vec<Value> = lookup
        .defaults_for_cn(cn)
        .iter()
        .map(|dv| {
            json!({
                "production_route": dv.production_route,
                "direct_tco2e_per_t": dv.direct_tco2e_per_t,
                "indirect_tco2e_per_t": dv.indirect_tco2e_per_t,
                "sector": dv.cn_code.sector().as_str(),
            })
        })
        .collect();
    Ok((StatusCode::OK, Json(Value::Array(defaults))).into_response())
}

// ---------------------------------------------------------------------------
// Consignments
// ---------------------------------------------------------------------------

/// POST body: a consignment plus the two persistence-side optionals.
#[derive(Deserialize)]
struct NewConsignmentBody {
    #[serde(flatten)]
    consignment: Consignment,
    status: Option<String>,
    eori: Option<String>,
}

/// `POST /api/consignments` — validate (rejecting on any Error-severity
/// issue with the full issues array as data; warnings pass through),
/// classify the customs status (default `40 00` = free circulation, R15),
/// persist, and audit. The response is `201 {"id", "status", "issues":
/// [warnings only]}`.
async fn create_consignment(State(state): State<AppState>, body: Bytes) -> ApiResult {
    let body: NewConsignmentBody = parse_body(&body)?;
    let storage: &dyn Storage = state.storage().as_ref();

    // R12: validate against reference data. `validate_consignment` includes
    // the domain invariants (mass, date) as Error issues, so one gate covers
    // both `Consignment::validate` and the plausibility checks.
    let lookup = Lookup::from_storage(storage).map_err(ApiError::from)?;
    let issues = validate_consignment(&body.consignment, &lookup);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    if let Some(reject) = errors.first() {
        // 400 with the issues array: validation findings are data for the
        // wizard to render, the status only signals the rejection.
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "key": "validate.issue.rejected",
                    "message": format!(
                        "consignment rejected: {} on `{}` ({} Error issue(s))",
                        reject.code,
                        reject.field,
                        errors.len()
                    ),
                },
                "issues": issues,
            })),
        )
            .into_response());
    }
    let warnings: Vec<&crate::validate::ValidationIssue> = issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .collect();

    // Box 37 classification (R15): explicit procedure code or the free-
    // circulation default.
    let cbam_status = match body.status.as_deref() {
        Some(code) => customs::classify(code).map_err(ApiError::from)?,
        None => customs::classify("40 00").map_err(ApiError::from)?,
    };
    let row_id = store::insert_consignment(
        storage,
        &body.consignment,
        &status_token(cbam_status),
        body.eori.as_deref(),
    )
    .map_err(ApiError::from)?;
    let payload = serde_json::to_string(&body.consignment)
        .map_err(|e| ApiError::bad_request("api.error.invalid_json", e.to_string()))?;
    audit(
        storage,
        "consignment.created",
        &format!("consignment:{row_id}"),
        &payload,
    )
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row_id,
            "status": status_token(cbam_status),
            "issues": warnings,
        })),
    )
        .into_response())
}

/// `GET /api/consignments?year=2026&eori=DE...` — the year's records,
/// optionally scoped to one declarant workspace (R25).
async fn list_consignments_api(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> ApiResult {
    let year = query_i32(&q, "year")?;
    let eori = q.get("eori").map(|s| s.trim()).filter(|s| !s.is_empty());
    let listed =
        store::list_consignments(state.storage().as_ref(), year, eori).map_err(ApiError::from)?;
    Ok((
        StatusCode::OK,
        Json(json!({ "year": year, "consignments": listed })),
    )
        .into_response())
}

/// POST body for the SAD/H1 bulk import: either `xml` or `csv`.
#[derive(Deserialize)]
struct ImportSadBody {
    xml: Option<String>,
    csv: Option<String>,
}

/// Ensure the FK-valid `UNMAPPED` installation exists: SAD rows carry no
/// installation reference, and `consignments.installation_id` REFERENCES the
/// seeded `installations` table.
fn ensure_unmapped_installation(storage: &dyn Storage) -> Result<(), DomainError> {
    storage
        .execute(
            "INSERT OR IGNORE INTO installations (id, name, address, production_routes) \
             VALUES ('UNMAPPED', 'Unmapped installation', '', '')",
            &[],
        )
        .map_err(|e| DomainError::Storage(e.to_string()))?;
    Ok(())
}

/// `POST /api/consignments/import-sad` — bulk import of a broker H1 (SAD)
/// XML or CSV export (R15): parse, classify every row through the Box 37
/// rule engine, and insert each as a consignment record with its classified
/// status (installations unmapped, DEFAULT determination basis).
async fn import_sad(State(state): State<AppState>, body: Bytes) -> ApiResult {
    let body: ImportSadBody = parse_body(&body)?;
    let storage: &dyn Storage = state.storage().as_ref();

    let rows = match (body.xml.as_deref(), body.csv.as_deref()) {
        (Some(xml), _) => crate::registry::parse_sad_xml(xml),
        (None, Some(csv)) => crate::registry::parse_sad_csv(csv),
        (None, None) => {
            return Err(ApiError::bad_request(
                "api.error.missing_param",
                "the body must carry either `xml` or `csv`",
            ));
        }
    }
    .map_err(ApiError::from)?;
    let classified = crate::registry::classify_imports(&rows).map_err(ApiError::from)?;

    ensure_unmapped_installation(storage).map_err(ApiError::from)?;
    let mut statuses = Vec::with_capacity(classified.len());
    for import in &classified {
        let consignment = Consignment {
            cn_code: import.row.cn_code.clone(),
            net_mass_kg: import.row.net_mass_kg,
            country_of_origin: import.row.country_of_origin.clone(),
            production_country: import.row.country_of_origin.clone(),
            installation_id: "UNMAPPED".to_string(),
            import_date: import.row.clearance_date.clone(),
            determination_basis: crate::domain::types::DeterminationBasis::Default,
            carbon_price_eur_per_tco2e: None,
            carbon_price_country: None,
        };
        let status = status_token(import.status);
        store::insert_consignment(storage, &consignment, &status, None).map_err(ApiError::from)?;
        statuses.push(status);
    }
    audit(
        storage,
        "consignment.imported_sad",
        "consignment:import",
        &format!("{{\"rows\":{}}}", classified.len()),
    )
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "imported": classified.len(), "statuses": statuses })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// De-minimis (R1)
// ---------------------------------------------------------------------------

/// `GET /api/deminimis?year=2026` — the 50 t calendar-year net-mass tracker
/// over that year's LIABLE consignments only (R15: deferred/tracked/excluded
/// regimes never count; exempt origins are a later overlay).
async fn deminimis(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> ApiResult {
    let year = query_i32(&q, "year")?;
    let listed =
        store::list_consignments(state.storage().as_ref(), year, None).map_err(ApiError::from)?;
    let mut tracker = DeMinimisTracker::new();
    for record in listed.iter().filter(|r| r.status == "LIABLE") {
        tracker.add(record.consignment.net_mass_kg);
    }
    Ok((
        StatusCode::OK,
        Json(json!({
            "year": year,
            "ytd_net_mass_kg": tracker.ytd_net_mass_kg(),
            "threshold_kg": DE_MINIMIS_THRESHOLD_TONNES * 1000.0,
            "crossed": tracker.crossed(),
            "is_exempt": tracker.is_exempt(),
        })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Exposure (R3/R4/R7)
// ---------------------------------------------------------------------------

/// Resolve the ETS price for a projection: the explicit query parameter wins,
/// then the cached price with its staleness flag; nowhere → 409 (R7: the
/// projection never guesses).
fn resolve_price(
    storage: &dyn Storage,
    q: &BTreeMap<String, String>,
) -> Result<(f64, bool, bool), ApiError> {
    match q.get("ets_price") {
        Some(raw) => {
            let eur: f64 = raw.trim().parse().map_err(|_| {
                ApiError::bad_request(
                    "core.error.invalid_ets_price",
                    format!("query parameter `ets_price` must be a number, got `{raw}`"),
                )
            })?;
            Ok((eur, false, false))
        }
        None => match store::get_price(storage).map_err(ApiError::from)? {
            Some((eur, _as_of, manual, stale)) => Ok((eur, stale, manual)),
            None => Err(ApiError::no_price()),
        },
    }
}

/// `GET /api/exposure?year=2026&formula=A|B&ets_price=80.5` — per-consignment
/// embedded emissions (default path, year mark-up applied, R3/R4) plus gross
/// and net exposure (R7), with totals, the resolved price, and the year's
/// CBAM factor.
async fn exposure(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> ApiResult {
    let year = query_i32(&q, "year")?;
    let formula = match q.get("formula").map(|s| s.trim().to_ascii_uppercase()) {
        None => Formula::A,
        Some(f) if f == "A" => Formula::A,
        Some(f) if f == "B" => Formula::B,
        Some(f) => {
            return Err(ApiError::bad_request(
                "api.error.missing_param",
                format!("query parameter `formula` must be A or B, got `{f}`"),
            ));
        }
    };
    let storage: &dyn Storage = state.storage().as_ref();
    let (price_eur, stale, manual) = resolve_price(storage, &q)?;
    let factor = cbam_factor(year).map_err(ApiError::from)?;
    let lookup = Lookup::from_storage(storage).map_err(ApiError::from)?;
    let listed = store::list_consignments(storage, year, None).map_err(ApiError::from)?;

    let mut rows_out = Vec::new();
    let (mut total_emissions, mut total_gross, mut total_net) = (0.0_f64, 0.0_f64, 0.0_f64);
    for record in listed.iter().filter(|r| r.status == "LIABLE") {
        let c = &record.consignment;
        // Default path via the CN's first registered route; no default is a
        // per-row error note, never a failed request (the rest of the year
        // still projects).
        let Some(default) = lookup.defaults_for_cn(&c.cn_code).first().copied() else {
            rows_out.push(json!({
                "row_id": record.row_id,
                "cn_code": c.cn_code,
                "net_mass_kg": c.net_mass_kg,
                "error": DomainError::NoDefaultForCnCode(c.cn_code.clone()).to_string(),
            }));
            continue;
        };
        let emissions = match consignment_emissions_default(
            c,
            default.direct_tco2e_per_t,
            default.indirect_tco2e_per_t,
        ) {
            Ok(emissions) => emissions,
            Err(err) => {
                rows_out.push(json!({
                    "row_id": record.row_id,
                    "cn_code": c.cn_code,
                    "net_mass_kg": c.net_mass_kg,
                    "error": err.to_string(),
                }));
                continue;
            }
        };
        // Art 9 deduction: the carbon price paid abroad offsets the
        // obligation on the embedded tonnes.
        let carbon_paid = c.carbon_price_eur_per_tco2e.unwrap_or(0.0) * emissions;
        let gross = gross_exposure(emissions, price_eur, carbon_paid).map_err(ApiError::from)?;
        let net = net_exposure(emissions, price_eur, carbon_paid, factor, formula)
            .map_err(ApiError::from)?;
        total_emissions += emissions;
        total_gross += gross;
        total_net += net;
        rows_out.push(json!({
            "row_id": record.row_id,
            "cn_code": c.cn_code,
            "net_mass_kg": c.net_mass_kg,
            "import_date": c.import_date,
            "emissions_tco2e": emissions,
            "gross_eur": gross,
            "net_eur": net,
        }));
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "year": year,
            "formula": match formula { Formula::A => "A", Formula::B => "B" },
            "consignments": rows_out,
            "totals": {
                "emissions_tco2e": total_emissions,
                "gross_eur": total_gross,
                "net_eur": total_net,
            },
            "price": { "eur": price_eur, "stale": stale, "manual": manual },
            "factor": factor,
        })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Calendar (R14/R24/R34)
// ---------------------------------------------------------------------------

/// `GET /api/calendar?year=2027` — the year's fixed obligation dates with
/// their Brussels UTC offsets and i18n label keys.
async fn calendar(
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> ApiResult {
    let year = query_i32(&q, "year")?;
    let deadlines = calendar::deadlines_for_year(year).map_err(ApiError::from)?;
    let items: Vec<Value> = deadlines
        .iter()
        .map(|d| {
            json!({
                "kind": d.kind,
                "date": d.date_iso,
                "brussels_offset_hours": d.brussels_offset_hours,
                "label_key": d.label_key,
            })
        })
        .collect();
    Ok((
        StatusCode::OK,
        Json(json!({ "year": year, "deadlines": items })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Quarterly holding monitor (R24)
// ---------------------------------------------------------------------------

/// `GET /api/holding?year=2027&quarter=1` — the quarter-end certificate
/// position. The R24 basis is Annex IV default values WITHOUT the mark-up
/// (unlike the exposure endpoint); certificates held are purchases minus
/// cancellations and surrenders year-to-date.
async fn holding(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> ApiResult {
    let year = query_i32(&q, "year")?;
    let quarter_no: u32 = query_i32(&q, "quarter")?.try_into().map_err(|_| {
        ApiError::bad_request(
            "core.error.invalid_quarter",
            format!(
                "quarter must be 1..=4, got {}",
                q.get("quarter").map(String::as_str).unwrap_or("")
            ),
        )
    })?;
    let quarter = Quarter::new(year, quarter_no).map_err(ApiError::from)?;

    let storage: &dyn Storage = state.storage().as_ref();
    let lookup = Lookup::from_storage(storage).map_err(ApiError::from)?;
    let listed = store::list_consignments(storage, year, None).map_err(ApiError::from)?;
    let mut basis = 0.0_f64;
    for record in listed.iter().filter(|r| r.status == "LIABLE") {
        // R24 basis: raw Annex IV defaults, no mark-up, no rounding.
        if let Some(default) = lookup.defaults_for_cn(&record.consignment.cn_code).first() {
            basis += (default.direct_tco2e_per_t + default.indirect_tco2e_per_t)
                * record.consignment.net_mass_kg
                / 1000.0;
        }
    }
    let (purchased, cancelled, surrendered) =
        store::certificate_position(storage, year).map_err(ApiError::from)?;
    let position = HoldingPosition {
        quarter,
        basis_tco2e: basis,
        required_tco2e: basis * HOLDING_SHARE,
        held_tco2e: purchased - cancelled - surrendered,
        purchased_ytd_tco2e: purchased,
        cancelled_ytd_tco2e: cancelled + surrendered,
    };
    let level: AlertLevel = assess_holding(&position, &AlertThresholds::default());
    Ok((
        StatusCode::OK,
        Json(json!({
            "position": position,
            "level": level,
            "shortfall_tco2e": shortfall_tco2e(&position),
        })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Role selection (R47)
// ---------------------------------------------------------------------------

/// `GET /api/role` — the stored selection, or `null` before the first run.
async fn get_role(State(state): State<AppState>) -> ApiResult {
    match store::get_role(state.storage().as_ref()).map_err(ApiError::from)? {
        Some(stored) => {
            let value: Value = serde_json::from_str(&stored).map_err(|e| {
                ApiError::bad_request("core.error.storage", format!("corrupt stored role: {e}"))
            })?;
            Ok((StatusCode::OK, Json(value)).into_response())
        }
        None => Ok((StatusCode::OK, Json(Value::Null)).into_response()),
    }
}

/// A role token in a request body.
#[derive(Deserialize)]
struct RoleBody {
    role: Option<String>,
    add: Option<String>,
    switch: Option<String>,
}

/// `PUT /api/role` — first-run selection: `{"role": "EXPORTER"}`.
async fn put_role(State(state): State<AppState>, body: Bytes) -> ApiResult {
    let body: RoleBody = parse_body(&body)?;
    let role_name = body.role.ok_or_else(|| {
        ApiError::bad_request("api.error.missing_param", "the body must carry `role`")
    })?;
    let role = Role::parse(&role_name).map_err(ApiError::from)?;
    let selection = RoleSelection::first_run(role);
    let payload = roles::persist(&selection);
    store::set_role(state.storage().as_ref(), &payload).map_err(ApiError::from)?;
    audit(state.storage().as_ref(), "role.selected", "role", &payload).map_err(ApiError::from)?;
    let value: Value = serde_json::from_str(&payload)
        .map_err(|e| ApiError::bad_request("core.error.storage", e.to_string()))?;
    Ok((StatusCode::OK, Json(value)).into_response())
}

/// `PATCH /api/role` — `{"add": "VERIFIER"}` (add + activate, idempotent) or
/// `{"switch": "EXPORTER"}` (activate an already-configured role).
async fn patch_role(State(state): State<AppState>, body: Bytes) -> ApiResult {
    let body: RoleBody = parse_body(&body)?;
    let storage: &dyn Storage = state.storage().as_ref();
    let stored = store::get_role(storage)
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::bad_request(
                "core.error.storage",
                "no role configured yet: PUT /api/role first",
            )
        })?;
    let mut selection = roles::restore(&stored).map_err(ApiError::from)?;

    if let Some(add) = body.add.as_deref() {
        selection.add_role(Role::parse(add).map_err(ApiError::from)?);
    }
    if let Some(switch) = body.switch.as_deref() {
        // A switch to a never-added role is a client error (400), not a
        // backend failure — map it explicitly instead of the 500 that the
        // Storage variant would otherwise receive.
        if let Err(err) = selection.switch_active(Role::parse(switch).map_err(ApiError::from)?) {
            return Err(ApiError::bad_request(err.i18n_key(), err.to_string()));
        }
    }
    let payload = roles::persist(&selection);
    store::set_role(storage, &payload).map_err(ApiError::from)?;
    audit(storage, "role.updated", "role", &payload).map_err(ApiError::from)?;
    let value: Value = serde_json::from_str(&payload)
        .map_err(|e| ApiError::bad_request("core.error.storage", e.to_string()))?;
    Ok((StatusCode::OK, Json(value)).into_response())
}

/// `DELETE /api/role` — reset to the first-run state (R47: roles are
/// resettable without a reinstall).
async fn delete_role(State(state): State<AppState>) -> ApiResult {
    store::clear_role(state.storage().as_ref()).map_err(ApiError::from)?;
    Ok((StatusCode::OK, Json(Value::Null)).into_response())
}

// ---------------------------------------------------------------------------
// Declaration export (R9/R21/R30)
// ---------------------------------------------------------------------------

/// One masking entry: a bare field name (redact) or a name + policy object.
#[derive(Deserialize)]
#[serde(untagged)]
enum MaskSpec {
    Name(String),
    Detailed { name: String, policy: String },
}

impl MaskSpec {
    fn to_mask(&self) -> Result<(String, FieldMask), ApiError> {
        match self {
            MaskSpec::Name(name) => Ok((name.clone(), FieldMask::Redact)),
            MaskSpec::Detailed { name, policy } => {
                let mask = match policy.to_ascii_uppercase().as_str() {
                    "KEEP" => FieldMask::Keep,
                    "REDACT" => FieldMask::Redact,
                    "ANONYMIZE" => FieldMask::Anonymize,
                    other => {
                        return Err(ApiError::bad_request(
                            "api.error.invalid_json",
                            format!(
                                "unknown mask policy `{other}`: expected KEEP, REDACT or ANONYMIZE"
                            ),
                        ));
                    }
                };
                Ok((name.clone(), mask))
            }
        }
    }
}

/// The declaration export request.
#[derive(Deserialize)]
struct ExportBody {
    year: i32,
    eori: Option<String>,
    #[serde(default)]
    mask: Vec<MaskSpec>,
}

/// `POST /api/export/declaration` — build the year's declaration file from
/// stored data (one object per consignment carrying the 8 mandatory fields,
/// R9), apply per-field masks (R21), pre-flight against schema `2027.1`
/// (R30), and persist with the audit chain root at submission (R10).
///
/// Validation findings are DATA in the response (`valid: false` +
/// `violations`), never transport errors: the self-audit preview is the
/// point of the endpoint. Only a mask that would drop a mandatory field
/// fails closed at 400 (R9: an incomplete file never leaves the machine).
async fn export_declaration(State(state): State<AppState>, body: Bytes) -> ApiResult {
    let body: ExportBody = parse_body(&body)?;
    let storage: &dyn Storage = state.storage().as_ref();
    let eori = body
        .eori
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let listed = store::list_consignments(storage, body.year, eori).map_err(ApiError::from)?;
    let lookup = Lookup::from_storage(storage).map_err(ApiError::from)?;
    let masks: Vec<(String, FieldMask)> = body
        .mask
        .iter()
        .map(MaskSpec::to_mask)
        .collect::<Result<Vec<_>, _>>()?;

    let schema = SchemaEntry {
        version: DECLARATION_SCHEMA_VERSION.to_string(),
        required: REQUIRED_DECLARATION_FIELDS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        types: REQUIRED_DECLARATION_FIELDS
            .iter()
            .map(|name| {
                let kind = if *name == "net_mass_kg" || *name == "embedded_emissions_tco2e" {
                    "number"
                } else {
                    "string"
                };
                ((*name).to_string(), kind.to_string())
            })
            .collect(),
    };

    let mut objects: Vec<Value> = Vec::new();
    let mut violations: Vec<Value> = Vec::new();
    for (index, record) in listed.iter().enumerate() {
        let c = &record.consignment;
        // The embedded-emissions field on the default path (R3/R4, mark-up
        // applied). Without a registered default the consignment is reported
        // as a violation instead of silently dropped (R30 fail-closed).
        let Some(default) = lookup.defaults_for_cn(&c.cn_code).first().copied() else {
            violations.push(json!({
                "field": format!("consignment[{index}].embedded_emissions_tco2e"),
                "code": "NO_DEFAULT",
            }));
            continue;
        };
        let emissions = match consignment_emissions_default(
            c,
            default.direct_tco2e_per_t,
            default.indirect_tco2e_per_t,
        ) {
            Ok(emissions) => emissions,
            Err(err) => {
                violations.push(json!({
                    "field": format!("consignment[{index}].embedded_emissions_tco2e"),
                    "code": err.to_string(),
                }));
                continue;
            }
        };
        let fields = vec![
            DeclarationField {
                name: "cn_code".into(),
                value: json!(c.cn_code),
            },
            DeclarationField {
                name: "net_mass_kg".into(),
                value: json!(c.net_mass_kg),
            },
            DeclarationField {
                name: "country_of_origin".into(),
                value: json!(c.country_of_origin),
            },
            DeclarationField {
                name: "production_country".into(),
                value: json!(c.production_country),
            },
            DeclarationField {
                name: "installation_id".into(),
                value: json!(c.installation_id),
            },
            DeclarationField {
                name: "import_date".into(),
                value: json!(c.import_date),
            },
            DeclarationField {
                name: "determination_basis".into(),
                value: json!(c.determination_basis.as_str()),
            },
            DeclarationField {
                name: "embedded_emissions_tco2e".into(),
                value: json!(emissions),
            },
        ];
        // R21: masks first, then the R9 gate — a mask that redacts a
        // mandatory field fails the export closed.
        let masked = apply_masks(&fields, &masks);
        let object = build_declaration(&masked).map_err(ApiError::from)?;
        // R30 pre-flight: findings are data, not transport errors.
        if let Err(DomainError::SchemaViolation(detail)) = preflight_validate(&object, &schema) {
            violations.push(json!({ "field": format!("consignment[{index}]"), "code": detail }));
        }
        objects.push(object);
    }
    if listed.is_empty() {
        violations.push(json!({ "field": "consignments", "code": "MISSING" }));
    }

    let file = json!({
        "declaration_year": body.year,
        "schema_version": DECLARATION_SCHEMA_VERSION,
        "consignments": objects,
    });
    let file_json = serde_json::to_string(&file)
        .map_err(|e| ApiError::bad_request("core.error.storage", e.to_string()))?;
    // R10: the declaration ships with the audit root at submission.
    let chain_root = store::audit_root(storage).map_err(ApiError::from)?;
    store::save_declaration(
        storage,
        &format!("declaration-{}", body.year),
        body.year,
        DECLARATION_SCHEMA_VERSION,
        &file_json,
        &chain_root,
    )
    .map_err(ApiError::from)?;

    // R21 self-audit preview over the (field, mask) plan of the first
    // consignment's field set (all consignments share the field names).
    let field_names: Vec<(String, FieldMask)> = if listed.is_empty() {
        Vec::new()
    } else {
        REQUIRED_DECLARATION_FIELDS
            .iter()
            .map(|name| ((*name).to_string(), FieldMask::Keep))
            .chain(masks)
            .collect()
    };
    let report = preview(&field_names);

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "file": file,
            "preview": report,
            "schema_version": DECLARATION_SCHEMA_VERSION,
            "violations": violations,
            "valid": violations.is_empty(),
        })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Audit trail (R10)
// ---------------------------------------------------------------------------

/// `GET /api/audit?subject=consignment:3` — events (optionally scoped to one
/// subject), the chain root, and the end-to-end integrity verdict.
async fn audit_api(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>,
) -> ApiResult {
    let storage: &dyn Storage = state.storage().as_ref();
    let sql = match q.get("subject").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(subject) => (
            "SELECT seq, ts_utc, actor, action, subject, payload_hash, prev_hash, hash \
             FROM audit_events WHERE subject = ?1 ORDER BY seq ASC"
                .to_string(),
            vec![subject.to_string()],
        ),
        None => (
            "SELECT seq, ts_utc, actor, action, subject, payload_hash, prev_hash, hash \
             FROM audit_events ORDER BY seq ASC"
                .to_string(),
            Vec::new(),
        ),
    };
    let refs: Vec<&str> = sql.1.iter().map(String::as_str).collect();
    let rows = storage
        .query_rows(&sql.0, &refs)
        .map_err(|e| ApiError::from(DomainError::Storage(e.to_string())))?;
    let events: Vec<AuditEvent> = rows
        .into_iter()
        .map(|row| -> Result<AuditEvent, DomainError> {
            let cell = |col: usize| -> Result<String, DomainError> {
                row.get(col)
                    .cloned()
                    .flatten()
                    .ok_or_else(|| DomainError::Storage(format!("NULL in audit column {col}")))
            };
            Ok(AuditEvent {
                seq: cell(0)?
                    .parse()
                    .map_err(|e| DomainError::Storage(format!("audit seq: {e}")))?,
                ts_utc: cell(1)?,
                actor: cell(2)?,
                action: cell(3)?,
                subject: cell(4)?,
                payload_hash: cell(5)?,
                prev_hash: cell(6)?,
                hash: cell(7)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::from)?;
    let root = store::audit_root(storage).map_err(ApiError::from)?;
    let intact = store::verify_audit(storage).is_ok();
    Ok((
        StatusCode::OK,
        Json(json!({
            "events": events,
            "root": root,
            "intact": intact,
        })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Attachments (R16)
// ---------------------------------------------------------------------------

/// The attachment upload body: base64 content, metadata, and the mandatory
/// human-verification attestation.
#[derive(Deserialize)]
struct AttachmentBody {
    id: String,
    subject: String,
    filename: String,
    mime_type: String,
    content_b64: String,
    verified_by_human: bool,
    #[serde(default)]
    note: String,
}

/// `POST /api/attachments` — the R16 gate at the API boundary: an unverified
/// attachment is rejected (400 `core.error.human_verification_required`);
/// only the hash and metadata of a verified document are ever stored
/// (R16/R22: the document never leaves the device).
async fn create_attachment(State(state): State<AppState>, body: Bytes) -> ApiResult {
    let body: AttachmentBody = parse_body(&body)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body.content_b64.as_bytes())
        .map_err(|e| {
            ApiError::bad_request(
                "api.error.invalid_json",
                format!("content_b64 is not valid base64: {e}"),
            )
        })?;
    let attachment = provenance::new_attachment(
        &body.id,
        &body.filename,
        &body.mime_type,
        &bytes,
        body.verified_by_human,
        &body.note,
    )
    .map_err(ApiError::from)?;
    store::add_attachment(state.storage().as_ref(), &attachment, &body.subject)
        .map_err(ApiError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": attachment.id,
            "subject": body.subject,
            "filename": attachment.filename,
            "mime_type": attachment.mime_type,
            "sha256": attachment.sha256,
            "verified_by_human": attachment.verified_by_human,
        })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// ETS price cache (R7/R14)
// ---------------------------------------------------------------------------

/// `GET /api/price` — the cached price with its flags, or `null`.
async fn get_price_api(State(state): State<AppState>) -> ApiResult {
    let price = store::get_price(state.storage().as_ref()).map_err(ApiError::from)?;
    let value = match price {
        Some((eur, as_of, manual, stale)) => json!({
            "eur_per_tco2e": eur,
            "as_of": as_of,
            "manual": manual,
            "stale": stale,
        }),
        None => Value::Null,
    };
    Ok((StatusCode::OK, Json(json!({ "price": value }))).into_response())
}

/// The manual price-entry body.
#[derive(Deserialize)]
struct PriceBody {
    eur_per_tco2e: f64,
    as_of: String,
}

/// `PUT /api/price` — manual entry (R7/R22 fallback): the user is the source
/// of truth; the entry is stored manual and fresh.
async fn put_price(State(state): State<AppState>, body: Bytes) -> ApiResult {
    let body: PriceBody = parse_body(&body)?;
    // R7: a manual entry must never silently poison the projection.
    if !(body.eur_per_tco2e.is_finite() && body.eur_per_tco2e >= 0.0) {
        return Err(ApiError::from(DomainError::InvalidEtsPrice(
            body.eur_per_tco2e,
        )));
    }
    store::set_price(
        state.storage().as_ref(),
        body.eur_per_tco2e,
        body.as_of.trim(),
        true,
        false,
    )
    .map_err(ApiError::from)?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "price": {
                "eur_per_tco2e": body.eur_per_tco2e,
                "as_of": body.as_of.trim(),
                "manual": true,
                "stale": false,
            }
        })),
    )
        .into_response())
}
