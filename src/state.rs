// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Shared application state handed to every handler.

use std::sync::Arc;

use crate::i18n::I18n;

/// State shared across handlers. Storage is behind the `Storage` trait via
/// `Arc`, so tests can inject an in-memory database.
#[derive(Clone)]
pub struct AppState {
    i18n: Arc<I18n>,
    #[allow(dead_code)] // seam for the first real data endpoints
    storage: Arc<dyn crate::db::Storage>,
}

impl AppState {
    /// Production constructor.
    pub fn new(i18n: I18n, storage: Arc<dyn crate::db::Storage>) -> Self {
        Self {
            i18n: Arc::new(i18n),
            storage,
        }
    }

    /// Access the i18n layer.
    pub fn i18n(&self) -> &I18n {
        &self.i18n
    }

    /// Access the storage backend through the abstraction seam.
    #[allow(dead_code)] // seam for the first real data endpoints
    pub fn storage(&self) -> &Arc<dyn crate::db::Storage> {
        &self.storage
    }

    /// Test constructor with a throwaway storage backend, unique per `tag`.
    #[cfg(test)]
    pub fn new_for_tests(i18n: I18n, tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("kaimeter-state-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let db = crate::db::SqliteStorage::open(&dir.join("test.db")).expect("open db");
        db.migrate().expect("migrate");
        Self {
            i18n: Arc::new(i18n),
            storage: Arc::new(db),
        }
    }
}
