// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Artifact-contract tests for the 0.1.0 demo wizard (`web/wizard.html`).
//!
//! The wizard is a zero-dependency single file: no build step,
//! no frameworks, runnable from `file://`. These tests pin the artifact's
//! source-level contract — offline by construction (R22), first-run role
//! selection (R47), and the regulatory numbers it renders (R4/R7/R1, R23).
//! The routes that serve it are covered in `src/http/mod.rs`; browser-level
//! execution tests arrive with the E2E rig.
//!
//! The second half of this file is the end-to-end JSON API integration pass
//! (`/api/...`, the wizard ↔ core contract): every endpoint
//! runs against a real migrated SQLite database, and the routes `/`,
//! `/wizard.html`, `/healthz`, `/i18n/welcome` must keep serving unchanged.

use kaimeter_core::db::Storage;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const WIZARD: &str = include_str!("../web/wizard.html");
const EFAPIAO_FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/samples/energy-bills/efapiao-electricity-sample.txt"
));

// ---------------------------------------------------------------------------
// 1. Offline by construction (R22) + the wizard ↔ core integration contract:
//    the wizard never touches a third party and never phones home; the only
//    network surface allowed is same-origin `/api/...` persistence calls
//    (fire-and-forget, silent on failure) when served by the binary. From
//    `file://` the artifact is fully local.
// ---------------------------------------------------------------------------

#[test]
fn wizard_contains_no_network_surface() {
    // No absolute URLs of any kind — nothing third-party, nothing remote.
    let banned = [
        "http://",
        "https://",
        "<script src",
        "<link ",
        "@import",
        "import(",
        "XMLHttpRequest",
        "WebSocket",
        "EventSource",
        "sendBeacon",
    ];
    for b in banned {
        assert!(
            !WIZARD.contains(b),
            "wizard must not contain {b:?} — offline by construction (R22)"
        );
    }
    // fetch( is allowed ONLY for same-origin relative /api/ calls (the
    // persistence contract). Every fetch target must start
    // with "/api/" or the template-literal form `/api/`.
    let mut fetches = 0;
    let bytes = WIZARD.as_bytes();
    let mut i = 0;
    while let Some(pos) = WIZARD[i..].find("fetch(") {
        let at = i + pos;
        // Pull the first argument up to the matching closing paren.
        let rest = &WIZARD[at + "fetch(".len()..];
        let end = rest.find(')').unwrap_or(0);
        let target = rest[..end].trim();
        let ok = target.starts_with("\"/api/") || target.starts_with("`/api/");
        assert!(
            ok,
            "wizard fetch must target a relative /api path (same-origin \
             persistence only, R21/R22); found: {target}"
        );
        fetches += 1;
        i = at + 6 + end.min(bytes.len());
    }
    assert!(fetches >= 2, "server bridge expected (role + consignments)");
}

#[test]
fn wizard_is_a_single_self_contained_file() {
    assert!(WIZARD.starts_with("<!DOCTYPE html>"));
    assert!(WIZARD.contains("</html>"));
    assert!(WIZARD.contains("<style>"), "styles are inline");
    // Exactly one inline script, no external assets.
    assert_eq!(WIZARD.matches("<script>").count(), 1);
    assert!(!WIZARD.contains("src="), "no external asset references");
}

// ---------------------------------------------------------------------------
// 2. First-run role selection — four personas, resettable (R47)
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
    assert!(
        WIZARD.contains("openRoleModal"),
        "role modal is re-openable"
    );
    assert!(
        WIZARD.contains("if (!role) openRoleModal(true)"),
        "first run asks who the user is before any workflow renders"
    );
    // The role determines the rendered workflow: each persona maps to its views.
    assert!(WIZARD.contains("ROLE_VIEWS"));
    for view in ["dashboard", "consignments", "export", "packs", "review"] {
        assert!(WIZARD.contains(view), "view {view:?} missing");
    }
}

// ---------------------------------------------------------------------------
// 2b. First run = language, then the plain-words CBAM primer, then the role
// ---------------------------------------------------------------------------

#[test]
fn first_run_asks_language_then_cbam_primer_then_role() {
    // Onboarding order is fixed: language first (nothing else is
    // comprehensible in a language the user hasn't chosen), then a
    // plain-words CBAM guide, then the role question.
    for pane in ["wsLang", "wsPrimer", "wsRoles"] {
        assert!(
            WIZARD.contains(&format!("id=\"{pane}\"")),
            "welcome pane {pane:?} missing"
        );
    }
    let at = |m: &str| WIZARD.find(m).expect(m);
    assert!(
        at("id=\"wsLang\"") < at("id=\"wsPrimer\"") && at("id=\"wsPrimer\"") < at("id=\"wsRoles\""),
        "welcome order must be: language, primer, role"
    );
    assert!(
        WIZARD.contains("function welcomePickLang"),
        "the language step must come first"
    );
    assert!(
        WIZARD.contains("showWelcomeStep(3)"),
        "the primer hands off to the role step"
    );
    // The primer must exist in both languages.
    assert!(
        WIZARD.contains("What is CBAM?") && WIZARD.contains("什么是 CBAM"),
        "the CBAM primer must be bilingual"
    );
    // The primer is not a first-run-only surface: a visible top-bar link
    // re-opens it at any time, and from the link it closes instead of
    // forcing the role question.
    assert!(
        WIZARD.contains("id=\"cbamLink\"") && WIZARD.contains("function openPrimer"),
        "a top-bar link must re-open the What-is-CBAM primer"
    );
    assert!(
        WIZARD.contains("id=\"primerClose\""),
        "primer opened from the link must be closable, not role-forcing"
    );
}

// ---------------------------------------------------------------------------
// 3. Regulatory pins rendered by the artifact
// ---------------------------------------------------------------------------

#[test]
fn markup_schedule_pins_r4() {
    assert!(WIZARD.contains("y2026: 10"), "2026 mark-up is +10%");
    assert!(WIZARD.contains("y2027: 20"), "2027 mark-up is +20%");
    assert!(WIZARD.contains("y2028plus: 30"), "2028+ mark-up is +30%");
    assert!(WIZARD.contains("fertilisers: 1"), "fertilisers are +1%");
}

#[test]
fn cbam_factor_schedule_pins_r7_payable_share() {
    // Payable share = complement of the Art 10a(1a) free-allocation factor.
    // Rendered values must match the pin exactly.
    for (year, pct) in [
        ("2026", "2.5"),
        ("2027", "5"),
        ("2028", "10"),
        ("2029", "22.5"),
        ("2030", "48.5"),
        ("2031", "61"),
        ("2032", "73.5"),
        ("2033", "86"),
    ] {
        assert!(
            WIZARD.contains(&format!("{year}: {pct}")),
            "payable factor {year} must be {pct}%"
        );
    }
    assert!(
        WIZARD.contains("if (y >= 2034) return 100"),
        "from 2034 the obligation is 100%"
    );
}

#[test]
fn deadline_and_price_pins_are_rendered() {
    assert!(WIZARD.contains("2027-02-01"), "certificate sales open");
    assert!(WIZARD.contains("2027-09-30"), "first declaration due");
    assert!(WIZARD.contains("75.36"), "seed ETS price (2026-04-07)");
    assert!(WIZARD.contains("50.0 t"), "the 50-tonne de-minimis line");
}

#[test]
fn electricity_and_hydrogen_get_no_exemption_r1() {
    assert!(
        WIZARD.contains("ALWAYS_LIABLE"),
        "always-liable set must gate the 50-tonne line"
    );
    assert!(WIZARD.contains("ALWAYS_LIABLE.has(c.sector)"));
}

#[test]
fn sample_cn_codes_are_eight_digit() {
    for code in [
        "72083800", "73181500", "76041010", "25232100", "31021000", "27160000", "28041000",
    ] {
        assert_eq!(code.len(), 8);
        assert!(
            WIZARD.contains(code),
            "sample CN code {code} missing from the catalog"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. e-fapiao parser ↔ sample fixture contract (R23 + R16 human-verify)
// ---------------------------------------------------------------------------

#[test]
fn efapiao_parser_handles_every_key_in_the_sample_fixture() {
    for line in EFAPIAO_FIXTURE.lines() {
        let key = line.split([':', '：']).next().unwrap_or("").trim();
        if key.is_empty() {
            continue;
        }
        assert!(
            WIZARD.contains(key),
            "parser does not handle fixture key {key:?}"
        );
    }
}

#[test]
fn extracted_fields_route_through_human_verification() {
    assert!(
        WIZARD.contains("parseEfapiao"),
        "the e-fapiao parse demo must exist"
    );
    assert!(
        WIZARD.contains("ext:") && WIZARD.contains("verifyKeys"),
        "extracted fields must become verification rows (R16: the human verifies)"
    );
}

// ---------------------------------------------------------------------------
// 4b. Term tooltips are fully localized (R13)
// ---------------------------------------------------------------------------

#[test]
fn term_tips_resolve_in_every_dictionary() {
    // Every data-tipkey="K" must be defined once per language dictionary in
    // the wizard source (en + zh today) — a tip that renders untranslated
    // breaks the i18n-first contract.
    let mut keys = Vec::new();
    let mut rest = WIZARD;
    while let Some(pos) = rest.find("data-tipkey=\"") {
        let after = &rest[pos + "data-tipkey=\"".len()..];
        let end = after.find('"').expect("closing quote");
        keys.push(&after[..end]);
        rest = after;
    }
    assert!(
        keys.len() >= 5,
        "expected the term tooltips on dashboard + preview headings, found {keys:?}"
    );
    for k in keys {
        let occurrences = WIZARD.matches(&format!("{k}:")).count();
        assert!(
            occurrences >= 2,
            "tooltip key {k} must be defined in BOTH dictionaries, found {occurrences}"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. The wizard stays importable as the binary's embedded asset
// ---------------------------------------------------------------------------

#[test]
fn embedded_wizard_matches_the_file_on_disk() {
    let on_disk =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("web/wizard.html"))
            .expect("web/wizard.html exists");
    assert_eq!(WIZARD, on_disk);
}

// ---------------------------------------------------------------------------
// 6. Exposure & savings card (R7/R6/R4 display aggregation; the client demo)
// ---------------------------------------------------------------------------

#[test]
fn exposure_savings_card_contract() {
    assert!(
        WIZARD.contains("function exposureFor"),
        "the exposure aggregation must exist"
    );
    assert!(
        WIZARD.contains("exposureFor(year, 2027)"),
        "the projection must run the same imports through the pinned 2027 schedules"
    );
    assert!(
        WIZARD.contains("overLine"),
        "the R1 cliff must gate the amount due (under 50 t: €0 for exempt goods)"
    );
    assert!(
        WIZARD.contains("isCons") && WIZARD.contains("isPack"),
        "mill data packs must never appear in importer views (R47 demo hygiene)"
    );
    assert!(
        WIZARD.contains("packWorthTitle"),
        "the mill-side data-value card (the reseller-demo surface) must exist"
    );
}

// ---------------------------------------------------------------------------
// 6b. The math is live, not a step — it recomputes while the user types
// ---------------------------------------------------------------------------

#[test]
fn math_is_live_on_the_fields_step_not_a_third_step() {
    assert!(
        !WIZARD.contains("renderStep3"),
        "the standalone Math step is gone — the money must not sit behind a Next click"
    );
    assert!(
        !WIZARD.contains("data-s=\"4\""),
        "the wizard is a three-step flow: attach, fields (live math), verify"
    );
    assert!(
        WIZARD.contains("function renderLiveMath"),
        "the live math renderer must exist"
    );
    assert!(
        WIZARD.contains("updateNav(); renderLiveMath();"),
        "every field input must drive the live recompute"
    );
    // Both wizard entry points (fields + verify) carry the live panel.
    let panels = WIZARD.matches("mathPanelHtml()").count();
    assert!(
        panels >= 3,
        "panel defined and rendered on fields + verify, found {panels} uses"
    );
    // The mill money line renders in the live panel (pack mode), not only on
    // the saved preview — the reseller-demo moment happens while typing.
    assert!(
        WIZARD.contains("function packWorthLine"),
        "the pack worth line must render live in the panel"
    );
}

// ---------------------------------------------------------------------------
// 6c. Plain-words contract — no EU climate-law knowledge is assumed
// ---------------------------------------------------------------------------

#[test]
fn ets_and_certificates_are_explained_in_plain_words() {
    // Concept first, label second: the copy leads with what the thing IS
    // ("this is the EU's carbon price") and only then names it (ETS, CBAM).
    // A reader must never need to decode an acronym to follow the text —
    // "ETS (Emissions Trading System)" still reads as jargon-first.
    for marker in [
        "This is the EU's carbon price",
        "Emissions Trading System",
        "Carbon Border Adjustment Mechanism",
        "EU CBAM Registry",
        // The practical mechanics must be stated, not implied: nothing is
        // invoiced — the importer buys certificates and files one declaration.
        "What happens next",
        "no bill arrives",
        "接下来会发生什么",
        "不会有账单寄来",
        "这是欧盟的碳价",
        "碳排放交易体系",
        "碳边境调节机制",
        "什么是 CBAM",
        "欧盟 CBAM 登记处",
    ] {
        assert!(
            WIZARD.contains(marker),
            "plain-words explainer must cover {marker:?}"
        );
    }
    assert!(
        !WIZARD.contains("EU ETS price"),
        "the card must lead with the plain concept (EU carbon price), not the acronym"
    );
    // The mill never buys: the pack-mode math panel must say whose bill the
    // euros are.
    assert!(
        WIZARD.contains("packBuyerNote"),
        "the pack-mode buyer note must exist"
    );
}

// ---------------------------------------------------------------------------
// 6d. Verification is the verifier's act, not a self-declared checkbox
// ---------------------------------------------------------------------------

#[test]
fn verification_is_done_by_the_verifier_role_not_self_declared() {
    // A data pack cannot ship with the exporter asserting "verified" —
    // verification happens after the dossier is reviewed (Verifier → Review).
    // The pack wizard points there instead of offering a self-tick, the
    // Review tab lists packs with a one-way verify action, and packs render
    // an honest pending state until then.
    assert!(
        WIZARD.contains("verifyLaterHint"),
        "the pack wizard must point to the verifier flow, not offer a self-tick"
    );
    assert!(
        WIZARD.contains("wiz.mode === \"consignment\""),
        "the verifier self-tick may remain only on the importer's consignment form"
    );
    assert!(
        WIZARD.contains("function verifyPack"),
        "the Verifier role must be able to mark a pack verified"
    );
    for marker in ["packVerifyPending", "reviewVerifyBtn", "verifyStatus"] {
        assert!(
            WIZARD.contains(marker),
            "verification-lifecycle copy must exist: {marker:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 7. The JSON API integration pass (/api/...) — the wizard ↔ core contract:
//    every endpoint against a real migrated SQLite DB
// ---------------------------------------------------------------------------

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
    let app =
        kaimeter_core::http::router(kaimeter_core::state::AppState::new(i18n, storage.clone()));
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
    assert_eq!(exposure["price"]["eur"], 80.5);
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
    assert_eq!(exposure["price"]["eur"], 75.36);
    assert_eq!(exposure["price"]["manual"], true);

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
