// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Artifact-contract tests for the built wizard.
//!
//! The wizard is a zero-dependency single file — built from `web/` by
//! `build.rs` and embedded in the crate, so it also runs from `file://`. These
//! tests pin the artifact's source-level contract — offline by construction
//! (R22), first-run role selection (R47), and the regulatory numbers it renders
//! (R4/R7/R1, R23). The routes that serve it are covered in `src/http/mod.rs`;
//! browser-level execution tests arrive with the E2E rig.
//!
//! The second half of this file is the end-to-end JSON API integration pass
//! (`/api/...`, the wizard ↔ core contract): every endpoint
//! runs against a real migrated SQLite database, and the routes `/`,
//! `/wizard.html`, `/healthz`, `/i18n/welcome` must keep serving unchanged.

use kaimeter_core::db::Storage;
use std::sync::Arc;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

/// The embedded artifact. `WIZARD_TEMPLATE` is built from `web/` by `build.rs`,
/// so these tests always run against the UI the binary ships — there is no
/// checked-in copy that could drift from the sources.
const WIZARD: &str = kaimeter_core::wizard::WIZARD_TEMPLATE;

// ---------------------------------------------------------------------------
// 1. Artifact contract (R22): one self-contained file, offline by construction.
//
// These parse the built document rather than scanning its text. The artifact
// carries a bundled React runtime whose source contains attribute-shaped string
// literals (`src="`+P(e)+``) and W3C namespace constants, so a regex over the
// whole file reports references that do not exist and misses nothing real.
// Parsing asks the only question that matters: does the document reference
// anything outside itself?
// ---------------------------------------------------------------------------

/// Every URL the parsed document actually loads.
fn loaded_urls(html: &str) -> Vec<String> {
    let doc = scraper::Html::parse_document(html);
    let mut urls = Vec::new();
    for selector in [
        "script[src]",
        "link[href]",
        "img[src]",
        "img[srcset]",
        "source[src]",
        "iframe[src]",
    ] {
        let sel = scraper::Selector::parse(selector).expect("valid selector");
        for element in doc.select(&sel) {
            for attr in ["src", "href", "srcset"] {
                if let Some(value) = element.value().attr(attr) {
                    urls.push(format!("{selector} -> {value}"));
                }
            }
        }
    }
    urls
}

#[test]
fn wizard_references_no_external_resource() {
    let urls = loaded_urls(WIZARD);
    let external: Vec<&String> = urls.iter().filter(|u| !u.contains("data:")).collect();
    assert!(
        external.is_empty(),
        "the artifact must not load anything outside itself (R22); found {external:?}"
    );
}

#[test]
fn wizard_is_a_single_self_contained_file() {
    assert!(
        WIZARD.contains("<!DOCTYPE html>"),
        "a document doctype is required"
    );
    assert!(WIZARD.contains("</html>"));
    let doc = scraper::Html::parse_document(WIZARD);

    // Exactly one stylesheet-worth of CSS, and it is inline.
    let styles = scraper::Selector::parse("style").expect("selector");
    assert!(
        doc.select(&styles).count() >= 1,
        "styles must be inlined into the document"
    );

    // No external stylesheet link: a separate file would break file://.
    let links = scraper::Selector::parse("link[rel=stylesheet]").expect("selector");
    assert_eq!(
        doc.select(&links).count(),
        0,
        "a linked stylesheet would be a second file"
    );

    // The frontend is one inline module; no separate script files.
    let scripts = scraper::Selector::parse("script").expect("selector");
    for script in doc.select(&scripts) {
        assert!(
            script.value().attr("src").is_none(),
            "scripts must be inline, not referenced"
        );
    }
    assert!(
        doc.select(&scripts).count() >= 1,
        "the frontend bundle must be inline in the document"
    );
}

#[test]
fn wizard_keeps_the_locale_injection_region() {
    // The server splices the dictionaries it loaded at startup into this region;
    // `wizard::inject` panics if it is missing or duplicated. The build must
    // therefore preserve it exactly once.
    assert_eq!(
        WIZARD
            .matches(kaimeter_core::wizard::LOCALES_START_MARKER)
            .count(),
        1,
        "exactly one locale start marker"
    );
    assert_eq!(
        WIZARD
            .matches(kaimeter_core::wizard::LOCALES_END_MARKER)
            .count(),
        1,
        "exactly one locale end marker"
    );
}

#[test]
fn wizard_carries_a_fallback_dictionary_for_offline_use() {
    // Opened from `file://` there is no server to inject anything, so the
    // dictionaries must be baked into the bundle. Assert on rendered strings
    // rather than on a variable name: the minifier renames identifiers.
    let em = kaimeter_core::i18n::I18n::embedded().expect("embedded locales");
    let sample = em.t("en", "ui.roleImporter").expect("key");
    assert!(
        WIZARD.contains(sample),
        "the built artifact must embed the dictionaries: {sample:?} not found"
    );
    let zh = em.t("zh-CN", "ui.roleImporter").expect("key");
    assert!(
        WIZARD.contains(zh),
        "the built artifact must embed every shipped locale: {zh:?} not found"
    );
}

/// The bundled fallback dictionary must match `locales/*.json`.
///
/// The bundle embeds the dictionaries at build time (`gen-locales.mjs`), and the
/// artifact is committed — so it can go stale when a locale file changes without
/// a rebuild. This compares the embedded English dictionary against the locale
/// files key by key, which is the check that catches "edited the locale, forgot
/// to rebuild". Comparing whole dictionaries rather than scanning for call sites
/// avoids depending on how the minifier happens to spell a lookup.
#[test]
fn bundled_dictionary_matches_the_locale_files() {
    let i18n = kaimeter_core::i18n::I18n::embedded().expect("embedded locales");

    // The embedded object is the only `{en:{...}}` literal in the bundle; find it
    // by its first key and read to the matching close.
    let anchor = WIZARD
        .find("en:{")
        .expect("the bundle embeds a dictionary keyed by locale");
    let body = &WIZARD[anchor + "en:{".len()..];

    let mut missing = Vec::new();
    let mut checked = 0;
    for key in i18n
        .ui_dictionaries()
        .get("en")
        .expect("english dictionary")
        .keys()
    {
        checked += 1;
        // Keys are emitted unquoted (`massLineTitle:`) or quoted, depending on
        // whether they are valid identifiers.
        if !body.contains(&format!("{key}:")) && !body.contains(&format!("\"{key}\":")) {
            missing.push(key.clone());
        }
    }
    assert!(
        checked > 100,
        "expected the full dictionary, found {checked} keys"
    );
    assert!(
        missing.is_empty(),
        "the built bundle is missing {} locale keys (rebuild the frontend: \
         `npm --prefix web run build`); first few: {:?}",
        missing.len(),
        &missing[..missing.len().min(8)]
    );
}

// ---------------------------------------------------------------------------
// 2. Personas and first run (R47)
// ---------------------------------------------------------------------------

#[test]
fn role_selection_covers_all_four_personas() {
    for id in ["importer", "exporter", "trader", "verifier"] {
        assert!(
            WIZARD.contains(id),
            "persona {id:?} missing from the wizard"
        );
    }
}

#[test]
fn role_is_persisted_and_resettable() {
    assert!(WIZARD.contains("kaimeter.role"), "role persists locally");
}

// ---------------------------------------------------------------------------
// 3. Regulatory pins the artifact renders
//
// The numbers themselves are pinned in Rust by `tests/math.rs` and
// `tests/compliance.rs`; what these check is that the UI still surfaces them.
// The artifact is generated, so match the rendered text, not the source shape.
// ---------------------------------------------------------------------------

#[test]
fn artifact_renders_the_de_minimis_line() {
    assert!(
        WIZARD.contains("50") && WIZARD.contains("tonne"),
        "the 50-tonne line must be visible in the UI"
    );
    assert!(
        WIZARD.contains("massLineTitle") || WIZARD.contains("50-tonne line"),
        "the line card must be present"
    );
}

#[test]
fn artifact_surfaces_the_markup_and_factor_concepts() {
    // R4 mark-ups and the R7 payable share are the two numbers a declarant
    // cannot act without; both must have UI. Their values are pinned in Rust.
    assert!(
        WIZARD.contains("factorLbl"),
        "the CBAM factor card must exist"
    );
    assert!(
        WIZARD.contains("markup") || WIZARD.contains("Mark-up") || WIZARD.contains("mark-up"),
        "the default-value mark-up must be explained"
    );
}

#[test]
fn artifact_explains_electricity_and_hydrogen_are_always_liable() {
    assert!(
        WIZARD.contains("alwaysLiable"),
        "electricity and hydrogen get no exemption (R1) and the UI must say so"
    );
}

#[test]
fn artifact_shows_the_first_run_language_choice() {
    // R47: language, then the plain-words primer, then the role.
    assert!(WIZARD.contains("zh-CN"), "the Chinese locale must ship");
    assert!(WIZARD.contains("English"));
}

// 7. The JSON API integration pass (/api/...) — the wizard ↔ core contract:
//    every endpoint against a real migrated SQLite DB

/// Boot the full router over a temp locale set and a migrated temp SQLite
/// database (mirrors the `test_state` pattern of `src/http/mod.rs`, but with
/// the public constructors an integration test can use). Returns the app and
/// the storage handle for test-side seeding.
fn api_app(tag: &str) -> (axum::Router, Arc<kaimeter_core::db::SqliteStorage>) {
    let dir = std::env::temp_dir().join(format!("kaimeter-web-api-test-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("en.json"), r#"{"welcome":"Welcome to Kaimeter"}"#).expect("en");
    std::fs::write(dir.join("zh-CN.json"), r#"{"welcome":"欢迎使用 Kaimeter"}"#).expect("zh");
    std::fs::write(
        dir.join("termbase.json"),
        r#"{"terms":{"embedded emissions":{"zh-CN":"隐含排放"}}}"#,
    )
    .expect("termbase");
    let i18n = kaimeter_core::i18n::I18n::load(&dir).expect("i18n load");

    let db_dir = dir.join("db");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    let storage = Arc::new(
        kaimeter_core::db::SqliteStorage::open(&db_dir.join("kaimeter.db")).expect("open db"),
    );
    storage.migrate().expect("migrate to schema version 3");
    let app = kaimeter_core::http::router(
        kaimeter_core::state::AppState::new(i18n, storage.clone(), None).0,
    );
    (app, storage)
}

/// One HTTP round trip; a JSON response body is parsed (null on empty).
async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = axum::http::Request::builder().method(method).uri(uri);
    let request = match body {
        Some(v) => builder
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .expect("request with body"),
        None => builder.body(Body::empty()).expect("request without body"),
    };
    let res = app.oneshot(request).await.expect("response");
    let status = res.status();
    let bytes = res.into_body().collect().await.expect("body").to_bytes();
    // Non-JSON bodies (the HTML wizard) surface as `Null`.
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

/// The legacy routes must be untouched by the API mount.
#[tokio::test]
async fn legacy_routes_still_serve_unchanged() {
    let (app, _) = api_app("legacy");
    for uri in ["/", "/wizard.html"] {
        let (status, _) = call(app.clone(), "GET", uri, None).await;
        assert_eq!(status, StatusCode::OK, "{uri} still serves");
    }
    let (status, body) = call(app, "GET", "/healthz", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn api_reference_endpoints() {
    let (app, _) = api_app("reference");
    let (status, codes) = call(app.clone(), "GET", "/api/reference/cn-codes", None).await;
    assert_eq!(status, StatusCode::OK);
    let codes = codes.as_array().expect("code array");
    assert!(codes.len() >= 3, "seeded CN catalog present");
    assert!(
        codes
            .iter()
            .any(|c| c["code"] == "73181500" && c["sector"] == "STEEL"),
        "each entry carries code/description/sector"
    );

    let (status, defaults) = call(
        app.clone(),
        "GET",
        "/api/reference/defaults?cn=73181500",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(defaults[0]["production_route"], "EF");

    // Missing cn parameter -> 400; unknown CN -> 400 domain error.
    let (status, body) = call(app.clone(), "GET", "/api/reference/defaults", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["key"], "api.error.missing_param");
    let (status, body) = call(app, "GET", "/api/reference/defaults?cn=00000000", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["key"], "core.error.no_default_for_cn");
}

/// R47: first-run selection, overlap, switch, reset — all through the API.
#[tokio::test]
async fn api_role_lifecycle() {
    let (app, _) = api_app("role");
    let (status, body) = call(app.clone(), "GET", "/api/role", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_null(), "no role before the first run");

    let (status, body) = call(
        app.clone(),
        "PUT",
        "/api/role",
        Some(json!({"role": "EXPORTER"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], "EXPORTER");

    // PATCH adds an overlapping role and activates it.
    let (status, body) = call(
        app.clone(),
        "PATCH",
        "/api/role",
        Some(json!({"add": "VERIFIER"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], "VERIFIER");
    assert_eq!(body["roles"].as_array().map(Vec::len), Some(2));

    // Switching back only works among configured roles.
    let (status, body) = call(
        app.clone(),
        "PATCH",
        "/api/role",
        Some(json!({"switch": "EXPORTER"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], "EXPORTER");
    let (status, _) = call(
        app.clone(),
        "PATCH",
        "/api/role",
        Some(json!({"switch": "TRADING_HOUSE"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "never-added role cannot activate"
    );

    // DELETE resets to the first-run state.
    let (status, body) = call(app.clone(), "DELETE", "/api/role", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_null());
    let (_, body) = call(app, "GET", "/api/role", None).await;
    assert!(body.is_null(), "reset is durable");
}

#[tokio::test]
async fn api_consignment_validation_rejects_error_issues() {
    let (app, _) = api_app("validation");
    // Unknown CN code: an Error-severity issue -> 400 with the issues array.
    let bad = json!({
        "cn_code": "99999999",
        "net_mass_kg": 100.0,
        "country_of_origin": "CN",
        "production_country": "DE",
        "installation_id": "INST-DE-001",
        "import_date": "2026-03-15",
        "determination_basis": "DEFAULT",
    });
    let (status, body) = call(app.clone(), "POST", "/api/consignments", Some(bad)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["key"], "validate.issue.rejected");
    assert_eq!(body["issues"][0]["code"], "CN_UNKNOWN");
    assert_eq!(body["issues"][0]["severity"], "ERROR");

    // Negative mass is caught by the same gate (R12 domain invariants).
    let negative = json!({
        "cn_code": "73181500",
        "net_mass_kg": -5.0,
        "country_of_origin": "CN",
        "production_country": "DE",
        "installation_id": "INST-DE-001",
        "import_date": "2026-03-15",
        "determination_basis": "DEFAULT",
    });
    let (status, body) = call(app, "POST", "/api/consignments", Some(negative)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["issues"]
            .as_array()
            .expect("issues array")
            .iter()
            .any(|i| i["code"] == "NEGATIVE_MASS"),
        "domain invariant issues surface as data"
    );
}

/// R15: bulk SAD import classifies every row through the Box 37 engine and
/// only LIABLE rows count toward the 50 t line (R1).
#[tokio::test]
async fn api_sad_import_and_deminimis() {
    let (app, _) = api_app("sad");
    let csv = concat!(
        "cn_code,net_mass_kg,procedure_code,country_of_origin,clearance_date\n",
        "73181500,30000,40 00,CN,2026-03-15\n",
        "76041010,40000,71 00,CN,2026-04-01\n",
    );
    let (status, body) = call(
        app.clone(),
        "POST",
        "/api/consignments/import-sad",
        Some(json!({ "csv": csv })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "import accepted");
    assert_eq!(body["imported"], 2);
    assert_eq!(body["statuses"][0], "LIABLE");
    assert_eq!(body["statuses"][1], "DEFERRED", "71 00 rows defer CBAM");

    let (status, listed) = call(app.clone(), "GET", "/api/consignments?year=2026", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["consignments"].as_array().map(Vec::len), Some(2));

    // 30 t liable + 40 t deferred: the tracker sees only the LIABLE mass.
    let (status, demo) = call(app.clone(), "GET", "/api/deminimis?year=2026", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(demo["ytd_net_mass_kg"], 30000.0);
    assert_eq!(demo["threshold_kg"], 50000.0);
    assert_eq!(demo["crossed"], false);
    assert_eq!(demo["is_exempt"], true);

    // Malformed CSV is a 400 registry parse error, never a 500.
    let (status, body) = call(
        app,
        "POST",
        "/api/consignments/import-sad",
        Some(json!({ "csv": "totally,wrong,header\n1,2,3,4,5\n" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["key"], "core.error.registry_parse_error");
}

/// The full wizard session: role, consignments, de-minimis, exposure,
/// calendar, attachments (R16 gate), declaration export, audit chain.
#[tokio::test]
async fn api_end_to_end_pass() {
    let (app, storage) = api_app("e2e");

    // Give the seeded 73181500 default a non-zero intensity (the migration
    // ships structural placeholders; 2.0 tCO2e/t makes the math legible).
    storage
        .execute(
            "UPDATE default_values SET direct_tco2e_per_t = 2.0 WHERE cn_code = '73181500'",
            &[],
        )
        .expect("seed default value");

    // 1. Role first run (R47).
    let (status, _) = call(
        app.clone(),
        "PUT",
        "/api/role",
        Some(json!({"role": "IMPORTER_DECLARANT"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 2. Create a consignment (60 t crosses the 50 t line).
    let consignment = json!({
        "cn_code": "73181500",
        "net_mass_kg": 60000.0,
        "country_of_origin": "CN",
        "production_country": "DE",
        "installation_id": "INST-DE-001",
        "import_date": "2026-03-15",
        "determination_basis": "DEFAULT",
        "eori": "DE12345678",
    });
    let (status, created) = call(app.clone(), "POST", "/api/consignments", Some(consignment)).await;
    assert_eq!(status, StatusCode::CREATED, "created: {created}");
    let row_id = created["id"].as_i64().expect("row id");
    assert!(row_id > 0);
    assert_eq!(
        created["status"], "LIABLE",
        "default 40 00 classification (R15)"
    );

    // 3. List back with the eori workspace filter (R25).
    let (status, listed) = call(
        app.clone(),
        "GET",
        "/api/consignments?year=2026&eori=DE12345678",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["consignments"].as_array().map(Vec::len), Some(1));
    let (_, other_eori) = call(
        app.clone(),
        "GET",
        "/api/consignments?year=2026&eori=FR1",
        None,
    )
    .await;
    assert_eq!(other_eori["consignments"].as_array().map(Vec::len), Some(0));

    // 4. De-minimis crossed (R1).
    let (status, demo) = call(app.clone(), "GET", "/api/deminimis?year=2026", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(demo["ytd_net_mass_kg"], 60000.0);
    assert_eq!(demo["crossed"], true);
    assert_eq!(demo["is_exempt"], false);

    // 5. Exposure with an explicit price (R3/R4/R7):
    //    2.0 t/t × 1.10 mark-up × 60 t = 132 tCO2e; factor 2.5 %.
    let (status, exposure) = call(
        app.clone(),
        "GET",
        "/api/exposure?year=2026&formula=A&ets_price=80.5",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exposure["factor"], 0.025);
    assert_eq!(exposure["price"]["eur_per_tco2e"], 80.5);
    assert_eq!(exposure["consignments"][0]["emissions_tco2e"], 132.0);
    assert_eq!(exposure["consignments"][0]["gross_eur"], 132.0 * 80.5);
    assert_eq!(exposure["consignments"][0]["net_eur"], 132.0 * 0.025 * 80.5);
    assert_eq!(exposure["totals"]["emissions_tco2e"], 132.0);

    // Formula B scales the remainder after the Art 9 deduction (no carbon
    // price paid here, so B == A × factor ... both give the same number).
    let (_, exposure_b) = call(
        app.clone(),
        "GET",
        "/api/exposure?year=2026&formula=B&ets_price=80.5",
        None,
    )
    .await;
    assert_eq!(exposure_b["totals"]["net_eur"], (132.0 * 80.5) * 0.025);

    // 6. Calendar 2027: sales start + first declaration/surrender (R14).
    let (status, calendar) = call(app.clone(), "GET", "/api/calendar?year=2027", None).await;
    assert_eq!(status, StatusCode::OK);
    let deadlines = calendar["deadlines"].as_array().expect("deadlines");
    let sales = deadlines
        .iter()
        .find(|d| d["label_key"] == "calendar.sales_start")
        .expect("sales start deadline");
    assert_eq!(sales["date"], "2027-02-01");
    assert_eq!(sales["brussels_offset_hours"], 1, "February is CET");
    assert!(deadlines
        .iter()
        .any(|d| d["date"] == "2027-09-30" && d["label_key"] == "calendar.declaration_surrender"));

    // 7. Holding monitor (R24): basis EXCLUDES the mark-up (2.0 × 60 t =
    //    120), required 50 %, nothing held -> Shortfall of 60.
    let (status, holding) =
        call(app.clone(), "GET", "/api/holding?year=2026&quarter=1", None).await;
    assert_eq!(status, StatusCode::OK, "holding: {holding}");
    assert_eq!(holding["position"]["basis_tco2e"], 120.0);
    assert_eq!(holding["position"]["required_tco2e"], 60.0);
    assert_eq!(holding["level"], "SHORTFALL");
    assert_eq!(holding["shortfall_tco2e"], 60.0);

    // 8. Attachments: the R16 gate fires at the API boundary.
    let unverified = json!({
        "id": "att-1",
        "subject": format!("consignment:{row_id}"),
        "filename": "invoice.pdf",
        "mime_type": "application/pdf",
        "content_b64": "aGVsbG8=", // "hello"
        "verified_by_human": false,
        "note": "",
    });
    let (status, body) = call(app.clone(), "POST", "/api/attachments", Some(unverified)).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "R16: unverified is rejected"
    );
    assert_eq!(
        body["error"]["key"],
        "core.error.human_verification_required"
    );

    let verified = json!({
        "id": "att-2",
        "subject": format!("consignment:{row_id}"),
        "filename": "invoice.pdf",
        "mime_type": "application/pdf",
        "content_b64": "aGVsbG8=",
        "verified_by_human": true,
        "note": "verified by operator",
    });
    let (status, body) = call(app.clone(), "POST", "/api/attachments", Some(verified)).await;
    assert_eq!(status, StatusCode::CREATED, "verified attachment accepted");
    // SHA-256("hello") — only the hash is stored, never the bytes (R16/R22).
    assert_eq!(
        body["sha256"],
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );

    // 9. Declaration export (R9/R21/R30): the 8 mandatory fields per
    //    consignment, preview, schema validation, persisted with the root.
    let (status, export) = call(
        app.clone(),
        "POST",
        "/api/export/declaration",
        Some(json!({ "year": 2026, "eori": null, "mask": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "export: {export}");
    assert_eq!(export["schema_version"], "2027.1");
    assert_eq!(export["valid"], true);
    assert_eq!(export["violations"].as_array().map(Vec::len), Some(0));
    let objects = export["file"]["consignments"].as_array().expect("objects");
    assert_eq!(objects.len(), 1);
    let object = &objects[0];
    for field in [
        "cn_code",
        "net_mass_kg",
        "country_of_origin",
        "production_country",
        "installation_id",
        "import_date",
        "determination_basis",
        "embedded_emissions_tco2e",
    ] {
        assert!(
            object.get(field).is_some(),
            "mandatory field {field} present (R9)"
        );
    }
    assert_eq!(object["embedded_emissions_tco2e"], 132.0);
    assert!(
        export["preview"]["included"]
            .as_array()
            .expect("preview")
            .iter()
            .any(|f| f == "cn_code"),
        "the self-audit preview lists what ships (R21)"
    );

    // 10. Audit chain: intact, root-shaped, carries the walk's events (R10).
    let (status, audit) = call(app.clone(), "GET", "/api/audit", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(audit["intact"], true);
    assert_eq!(audit["root"].as_str().expect("root").len(), 64);
    let events = audit["events"].as_array().expect("events");
    assert!(
        events.len() >= 2,
        "role + consignment events recorded (R10)"
    );
    assert!(events.iter().any(|e| e["action"] == "consignment.created"));
    assert!(events.iter().any(|e| e["action"] == "role.selected"));

    // Subject-scoped audit queries filter to one record.
    let (status, scoped) = call(
        app,
        "GET",
        &format!("/api/audit?subject=consignment:{row_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let scoped_events = scoped["events"].as_array().expect("scoped events");
    assert!(scoped_events
        .iter()
        .all(|e| e["subject"] == format!("consignment:{row_id}")));
    assert!(!scoped_events.is_empty());
}

/// R21 sealed packs: seal → verify round-trips offline; any payload edit
/// breaks the proof; the signer identity (did:key) is stable across seals
/// (one local key, not a fresh identity per export).
#[tokio::test]
async fn api_pack_seal_verify_and_tamper_evidence() {
    let (app, _) = api_app("packseal");
    let leaves = [
        "evidence:production-log-sample.txt",
        "cn_code=76041010",
        "net_mass_kg=800000",
        "emissions_tco2e_per_t=8.6",
    ];
    let body = json!({
        "installation_ref": "INST-CN-AL-01",
        "cn_code": "76041010",
        "emission_factor_tco2e_per_t": 8.6,
        "embedded_emissions_tco2e": 6880.0,
        "evidence_leaves": leaves,
    });

    let (status, sealed) = call(app.clone(), "POST", "/api/pack/seal", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "seal: {sealed}");
    let vp = &sealed["vp"];
    assert_eq!(vp["type"], json!(["VerifiablePresentation"]));
    let proof = &vp["verifiableCredential"]["proof"];
    assert_eq!(proof["type"], "DataIntegrityProof");
    assert!(
        proof["verificationMethod"]
            .as_str()
            .unwrap_or("")
            .starts_with("did:key:"),
        "the signer is anchored by did:key in the VP itself"
    );
    let subject = &vp["verifiableCredential"]["credentialSubject"];
    assert_eq!(subject["cn_code"], "76041010");
    // The Merkle root over the evidence leaves is pinned in the payload.
    assert_eq!(
        subject["production_log_merkle_root"].as_str().map(str::len),
        Some(64)
    );
    assert!(
        sealed["vc_jwt"].as_str().unwrap_or("").contains('.'),
        "VC-JWT twin present"
    );

    // The untouched VP verifies; the verified content comes back as data.
    let (status, verdict) = call(
        app.clone(),
        "POST",
        "/api/pack/verify",
        Some(json!({ "vp": vp })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(verdict["valid"], true);
    assert_eq!(verdict["content"]["emission_factor_tco2e_per_t"], 8.6);

    // Tampering — the exact Notepad edit the critique describes — breaks
    // the proof: 8.6 becomes 0.1, verify says so without any shared state.
    let mut tampered = vp.clone();
    tampered["verifiableCredential"]["credentialSubject"]["emission_factor_tco2e_per_t"] =
        json!(0.1);
    let (_, verdict) = call(
        app.clone(),
        "POST",
        "/api/pack/verify",
        Some(json!({ "vp": tampered })),
    )
    .await;
    assert_eq!(
        verdict["valid"], false,
        "tamper must be a finding, not a 500"
    );

    // A malformed pack body fails closed at 400.
    let (status, _) = call(
        app.clone(),
        "POST",
        "/api/pack/seal",
        Some(json!({
            "installation_ref": "INST",
            "cn_code": "760410",
            "emission_factor_tco2e_per_t": 1.0,
            "embedded_emissions_tco2e": 1.0,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "short CN code refused");

    // The same local key signs every pack: the did:key is stable.
    let (_, second) = call(app, "POST", "/api/pack/seal", Some(body)).await;
    assert_eq!(second["did"], sealed["did"], "one device key across seals");
}

/// R7: with no price anywhere the exposure endpoint answers 409 rather than
/// guessing; the manual price cache unlocks it with visible flags.
#[tokio::test]
async fn api_exposure_price_fallback_chain() {
    let (app, storage) = api_app("price409");
    storage
        .execute(
            "INSERT INTO consignments (cn_code, net_mass_kg, country_of_origin, \
             production_country, installation_id, import_date, determination_basis, status) \
             VALUES ('73181500', 1000, 'CN', 'DE', 'INST-DE-001', '2026-03-15', 'DEFAULT', 'LIABLE')",
            &[],
        )
        .expect("seed consignment");

    let (status, body) = call(app.clone(), "GET", "/api/exposure?year=2026", None).await;
    assert_eq!(status, StatusCode::CONFLICT, "no price anywhere -> 409");
    assert_eq!(body["error"]["key"], "core.error.invalid_ets_price");

    // Manual entry (R7/R22 fallback)...
    let (status, _) = call(
        app.clone(),
        "PUT",
        "/api/price",
        Some(json!({ "eur_per_tco2e": 75.36, "as_of": "2026-04-07" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, price) = call(app.clone(), "GET", "/api/price", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(price["price"]["eur_per_tco2e"], 75.36);
    assert_eq!(price["price"]["manual"], true);
    assert_eq!(price["price"]["stale"], false);

    // ...unlocks the projection with the cached price and its flags.
    let (status, exposure) = call(app.clone(), "GET", "/api/exposure?year=2026", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exposure["price"]["eur_per_tco2e"], 75.36);
    assert_eq!(exposure["price"]["manual"], true);
    // The cached price is now shared by both endpoints, so it carries its
    // cache metadata here too.
    assert_eq!(exposure["price"]["as_of"], "2026-04-07");

    // A negative manual price is refused (R7: never poison the projection).
    let (status, body) = call(
        app,
        "PUT",
        "/api/price",
        Some(json!({ "eur_per_tco2e": -1.0, "as_of": "2026-04-07" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["key"], "core.error.invalid_ets_price");
}
