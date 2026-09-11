// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! HTTP layer: routes and handlers.

pub mod api;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::state::AppState;

#[cfg(test)]
use crate::wizard::WIZARD_TEMPLATE;

/// Build the application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(wizard))
        .route("/wizard.html", get(wizard))
        .route("/healthz", get(healthz))
        .route("/i18n/welcome", get(welcome))
        // The wizard's JSON API (wizard ↔ core integration contract).
        .nest("/api", api::router())
        .with_state(state)
}

/// `GET /` and `GET /wizard.html` — the offline demo wizard. Served from the
/// state's pre-rendered copy so the page's dictionaries always match the
/// loaded locales: with embedded locales the bytes equal the repo artifact;
/// with a `locales/` directory on disk the wizard is re-localized too.
async fn wizard(State(state): State<AppState>) -> Html<String> {
    Html(state.wizard_html().as_ref().clone())
}

/// `GET /healthz` — liveness probe.
async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// `GET /i18n/welcome` — proves the locale layer: resolves the `welcome` key
/// in `en` and `zh-CN`, plus the locked termbase term for "embedded emissions".
async fn welcome(State(state): State<AppState>) -> Response {
    let i18n = state.i18n();
    let en = i18n.t_or_en("en", "welcome");
    let zh = i18n.t_or_en("zh-CN", "welcome");
    (
        StatusCode::OK,
        Json(json!({
            "en": en,
            "zh-CN": zh,
            "terms": {
                "embedded emissions": {
                    "en": i18n.term("en", "embedded emissions"),
                    "zh-CN": i18n.term("zh-CN", "embedded emissions"),
                }
            },
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state(tag: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!("kaimeter-http-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("en.json"), r#"{"welcome":"Welcome to Kaimeter"}"#).unwrap();
        std::fs::write(dir.join("zh-CN.json"), r#"{"welcome":"欢迎使用 Kaimeter"}"#).unwrap();
        std::fs::write(
            dir.join("termbase.json"),
            r#"{"terms":{"embedded emissions":{"zh-CN":"隐含排放"}}}"#,
        )
        .unwrap();
        let i18n = crate::i18n::I18n::load(&dir).expect("i18n load");
        AppState::new_for_tests(i18n, tag)
    }

    /// A state built on the compiled-in locales — the single-file deployment
    /// path, where the served wizard must equal the repo artifact byte for
    /// byte.
    fn embedded_state(tag: &str) -> AppState {
        let i18n = crate::i18n::I18n::embedded().expect("embedded locales");
        AppState::new_for_tests(i18n, tag)
    }

    #[tokio::test]
    async fn healthz_returns_ok_json() {
        let app = router(test_state("healthz"));
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["status"], "ok");
    }

    #[tokio::test]
    async fn root_serves_embedded_wizard() {
        let app = router(embedded_state("wizard"));
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(ct.starts_with("text/html"), "content-type was {ct}");
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let html = std::str::from_utf8(&body).expect("wizard is utf-8");
        // The document may open with an HTML comment (the SPDX header); a
        // valid HTML5 doctype may be preceded by comments.
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Kaimeter"));
        // With embedded locales the served asset is byte-identical to the
        // repo file — one artifact, two delivery modes (file:// and `/`).
        assert_eq!(html, WIZARD_TEMPLATE);
    }

    #[tokio::test]
    async fn disk_locales_relocalize_the_served_wizard() {
        // A configured locales directory re-localizes the wizard UI too, not
        // just backend messages: its ui.* dictionaries replace the generated
        // block in the served page.
        let dir = std::env::temp_dir().join("kaimeter-http-test-wizard-disk");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join("en.json"),
            r#"{"welcome":"hi","ui.docTitle":"Override Title"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("zh-CN.json"),
            r#"{"welcome":"你好","ui.docTitle":"覆盖标题"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("termbase.json"), r#"{"terms":{}}"#).unwrap();
        let i18n = crate::i18n::I18n::load(&dir).expect("i18n load");
        let state = AppState::new_for_tests(i18n, "wizard-disk");

        let app = router(state);
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/wizard.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let html = std::str::from_utf8(&body).expect("wizard is utf-8");
        assert!(html.contains("Override Title"), "ui override served");
        assert_ne!(html, WIZARD_TEMPLATE);
    }

    #[tokio::test]
    async fn welcome_resolves_both_locales_and_termbase() {
        let app = router(test_state("welcome"));
        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/i18n/welcome")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["en"], "Welcome to Kaimeter");
        assert_eq!(v["zh-CN"], "欢迎使用 Kaimeter");
        assert_eq!(v["terms"]["embedded emissions"]["zh-CN"], "隐含排放");
    }
}
