// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Kaimeter core server: an open-core CBAM compliance toolkit.
//!
//! Single-process monolith: axum HTTP, embedded SQLite behind a storage
//! abstraction, i18n-first locale loading with a compliance termbase.

pub mod calendar;
pub mod compliance;
pub mod config;
pub mod customs;
pub mod db;
pub mod domain;
pub mod dossier;
pub mod export;
pub mod http;
pub mod i18n;
pub mod liability;
pub mod math;
pub mod provenance;
pub mod registry;
pub mod roles;
pub mod state;
pub mod store;
pub mod sync;
pub mod validate;
pub mod vault;
pub mod verifier;

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use axum::serve;
use state::AppState;
use tokio::net::TcpListener;

use crate::config::Config;
use crate::db::{SqliteStorage, Storage};
use crate::i18n::I18n;

/// Run the server end-to-end: load config, prepare data dir + database,
/// run migrations, load locales, bind, and serve until SIGINT/SIGTERM.
pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("load configuration")?;
    let state = bootstrap(&cfg)
        .await
        .context("bootstrap application state")?;
    serve_until_shutdown(cfg.addr, state).await
}

/// Prepare the data directory, SQLite database, migrations, and locales.
pub async fn bootstrap(cfg: &Config) -> anyhow::Result<AppState> {
    std::fs::create_dir_all(&cfg.data_dir)
        .with_context(|| format!("create data dir {}", cfg.data_dir.display()))?;

    let db_path = cfg.data_dir.join("kaimeter.db");
    let storage = SqliteStorage::open(&db_path)
        .map_err(|e| anyhow::anyhow!(e))
        .with_context(|| format!("open SQLite at {}", db_path.display()))?;
    storage
        .migrate()
        .map_err(|e| anyhow::anyhow!(e))
        .context("run database migrations")?;
    tracing::info!(path = %db_path.display(), version = storage.schema_version().unwrap_or(0), "database ready");

    let i18n = I18n::load(&cfg.locales_dir)
        .map_err(|e| anyhow::anyhow!(e))
        .with_context(|| format!("load locales from {}", cfg.locales_dir.display()))?;
    for code in i18n.locale_codes() {
        tracing::info!(locale = %code, welcome = %i18n.t_or_en(&code, "welcome"), "locale loaded");
    }

    Ok(AppState::new(i18n, Arc::new(storage)))
}

/// Bind `addr` and serve until SIGINT or SIGTERM arrives.
pub async fn serve_until_shutdown(addr: String, state: AppState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    let app = http::router(state);
    tracing::info!(%addr, "kaimeter listening");
    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")
}

/// Resolve when SIGINT (Ctrl+C) or SIGTERM is received.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install SIGINT handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received; draining connections");
}

/// Ensure the locales directory contains the required files (used by tests and
/// by tooling that needs to verify a deployment's locale assets).
pub fn locales_present(dir: &Path) -> bool {
    dir.join("en.json").is_file()
        && dir.join("zh-CN.json").is_file()
        && dir.join("termbase.json").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kaimeter-lib-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn bootstrap_creates_db_and_runs_migrations_and_loads_locales() {
        let root = scratch("bootstrap");
        std::fs::create_dir_all(&root).unwrap();
        let locales = root.join("locales");
        std::fs::create_dir_all(&locales).unwrap();
        std::fs::write(
            locales.join("en.json"),
            r#"{"welcome":"Welcome to Kaimeter"}"#,
        )
        .unwrap();
        std::fs::write(
            locales.join("zh-CN.json"),
            r#"{"welcome":"欢迎使用 Kaimeter"}"#,
        )
        .unwrap();
        std::fs::write(
            locales.join("termbase.json"),
            r#"{"terms":{"embedded emissions":{"zh-CN":"隐含排放"}}}"#,
        )
        .unwrap();

        let cfg = Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: root.join("data"),
            locales_dir: locales.clone(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let state = rt.block_on(bootstrap(&cfg)).expect("bootstrap");

        // DB exists, migration applied.
        assert!(cfg.data_dir.join("kaimeter.db").is_file());
        // i18n resolves both locales.
        assert_eq!(
            state.i18n().t("en", "welcome").unwrap(),
            "Welcome to Kaimeter"
        );
        assert_eq!(
            state.i18n().t("zh-CN", "welcome").unwrap(),
            "欢迎使用 Kaimeter"
        );
        assert!(locales_present(&locales));
    }

    #[test]
    fn bootstrap_fails_when_locales_missing() {
        let root = scratch("nolocales");
        std::fs::create_dir_all(&root).unwrap();
        let cfg = Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: root.join("data"),
            locales_dir: root.join("no-such-locales"),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(bootstrap(&cfg)).is_err());
    }
}
