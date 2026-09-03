// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Storage abstraction with an embedded SQLite backend.
//!
//! The [`Storage`] trait is the seam that lets tests swap the file database
//! for an in-memory one: everything above it only depends on the trait,
//! never on rusqlite types.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

/// Library error type.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The SQLite database could not be opened or initialized.
    #[error("storage open failed: {0}")]
    Open(#[source] anyhow::Error),
    /// A query or migration failed.
    #[error("storage operation failed: {0}")]
    Query(#[source] anyhow::Error),
}

/// Minimal storage contract. SQLite is the only backend; the trait keeps
/// file and in-memory databases interchangeable for tests.
pub trait Storage: Send + Sync {
    /// Execute a write statement, returning the number of affected rows.
    fn execute(&self, sql: &str, params: &[&str]) -> Result<usize, StorageError>;
    /// Run a query returning all rows as string cells (`NULL` -> `None`).
    fn query_rows(
        &self,
        sql: &str,
        params: &[&str],
    ) -> Result<Vec<Vec<Option<String>>>, StorageError>;
    /// Run a scalar query, returning the first column of the first row.
    fn query_scalar(&self, sql: &str, params: &[&str]) -> Result<Option<String>, StorageError>;
    /// Current schema version (max applied migration).
    fn schema_version(&self) -> Result<i64, StorageError>;
}

/// Embedded SQLite-backed [`Storage`].
pub struct SqliteStorage {
    conn: Mutex<Connection>,
}

impl SqliteStorage {
    /// Open (creating if needed) the database at `db_path` and bring the
    /// schema up to date by running all embedded migrations.
    pub fn open(db_path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open(db_path)
            .map_err(|e| StorageError::Open(anyhow::anyhow!("open {}: {e}", db_path.display())))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| StorageError::Open(anyhow::anyhow!("set WAL mode: {e}")))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| StorageError::Open(anyhow::anyhow!("enable foreign keys: {e}")))?;
        // 0.9.0 hardening (R20/R22 build note): avoid lockups on low-spec
        // factory machines during schema updates and concurrent access.
        conn.pragma_update(None, "busy_timeout", 5000)
            .map_err(|e| StorageError::Open(anyhow::anyhow!("set busy timeout: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Run all embedded migrations newer than the current schema version.
    pub fn migrate(&self) -> Result<(), StorageError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| StorageError::Query(anyhow::anyhow!("storage mutex poisoned")))?;
        let tx = conn
            .transaction()
            .map_err(|e| StorageError::Query(anyhow::anyhow!("begin migration tx: {e}")))?;

        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 version    INTEGER PRIMARY KEY,
                 applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             );",
        )
        .map_err(|e| StorageError::Query(anyhow::anyhow!("create schema_migrations: {e}")))?;

        let current: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .map_err(|e| StorageError::Query(anyhow::anyhow!("read schema version: {e}")))?;

        for (version, name, sql) in embedded_migrations() {
            if version <= current {
                continue;
            }
            tracing::info!(version, name, "applying migration");
            tx.execute_batch(sql).map_err(|e| {
                StorageError::Query(anyhow::anyhow!("migration {version} ({name}): {e}"))
            })?;
            tx.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            )
            .map_err(|e| StorageError::Query(anyhow::anyhow!("record migration {version}: {e}")))?;
        }
        tx.commit()
            .map_err(|e| StorageError::Query(anyhow::anyhow!("commit migrations: {e}")))
    }
}

impl Storage for SqliteStorage {
    fn execute(&self, sql: &str, params: &[&str]) -> Result<usize, StorageError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StorageError::Query(anyhow::anyhow!("storage mutex poisoned")))?;
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StorageError::Query(anyhow::anyhow!("prepare: {e}")))?;
        stmt.execute(rusqlite::params_from_iter(params.iter()))
            .map_err(|e| StorageError::Query(anyhow::anyhow!("execute: {e}")))
    }

    fn query_rows(
        &self,
        sql: &str,
        params: &[&str],
    ) -> Result<Vec<Vec<Option<String>>>, StorageError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StorageError::Query(anyhow::anyhow!("storage mutex poisoned")))?;
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StorageError::Query(anyhow::anyhow!("prepare: {e}")))?;
        let col_count = stmt.column_count();
        let mut rows = stmt
            .query(rusqlite::params_from_iter(params.iter()))
            .map_err(|e| StorageError::Query(anyhow::anyhow!("query: {e}")))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(|e| StorageError::Query(anyhow::anyhow!("next row: {e}")))?
        {
            let mut cells = Vec::with_capacity(col_count);
            for col in 0..col_count {
                let value = row
                    .get_ref(col)
                    .map_err(|e| StorageError::Query(anyhow::anyhow!("column {col}: {e}")))?;
                let text = match value {
                    rusqlite::types::ValueRef::Null => None,
                    rusqlite::types::ValueRef::Integer(i) => Some(i.to_string()),
                    rusqlite::types::ValueRef::Real(f) => Some(f.to_string()),
                    rusqlite::types::ValueRef::Text(t) => {
                        Some(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => {
                        Some(String::from_utf8_lossy(b).into_owned())
                    }
                };
                cells.push(text);
            }
            out.push(cells);
        }
        Ok(out)
    }

    fn query_scalar(&self, sql: &str, params: &[&str]) -> Result<Option<String>, StorageError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StorageError::Query(anyhow::anyhow!("storage mutex poisoned")))?;
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| StorageError::Query(anyhow::anyhow!("prepare: {e}")))?;
        let mut rows = stmt
            .query(rusqlite::params_from_iter(params.iter()))
            .map_err(|e| StorageError::Query(anyhow::anyhow!("query: {e}")))?;
        match rows
            .next()
            .map_err(|e| StorageError::Query(anyhow::anyhow!("next row: {e}")))?
        {
            Some(row) => row
                .get::<_, Option<String>>(0)
                .map_err(|e| StorageError::Query(anyhow::anyhow!("column 0: {e}"))),
            None => Ok(None),
        }
    }

    fn schema_version(&self) -> Result<i64, StorageError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| StorageError::Query(anyhow::anyhow!("storage mutex poisoned")))?;
        conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| StorageError::Query(anyhow::anyhow!("read schema version: {e}")))
    }
}

/// All migrations, versioned, embedded as SQL strings (`include_str!`).
///
/// Add new migrations by dropping a `.sql` file into `migrations/` and
/// appending to this list — versions are applied in ascending order, each
/// inside its own transaction, recorded in `schema_migrations`.
fn embedded_migrations() -> Vec<(i64, &'static str, &'static str)> {
    vec![
        (
            1,
            "0001_create_settings",
            include_str!("../migrations/0001_create_settings.sql"),
        ),
        (
            2,
            "0002_seed_tables",
            include_str!("../migrations/0002_seed_tables.sql"),
        ),
        (
            3,
            "0003_records",
            include_str!("../migrations/0003_records.sql"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kaimeter-db-test-{tag}"));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir.join("kaimeter.db")
    }

    #[test]
    fn migration_runner_creates_settings_table_and_records_version() {
        let path = temp_db("settings");
        let _ = std::fs::remove_file(&path);
        let storage = SqliteStorage::open(&path).expect("open");
        storage.migrate().expect("migrate");
        assert_eq!(storage.schema_version().expect("version"), 3);
        // Table exists and is usable through the trait seam.
        let affected = storage
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                &["greeting", "hello"],
            )
            .expect("insert");
        assert_eq!(affected, 1);
        assert_eq!(
            storage
                .query_scalar("SELECT value FROM settings WHERE key = 'greeting'", &[])
                .expect("select")
                .as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn migrations_are_idempotent() {
        let path = temp_db("idempotent");
        let _ = std::fs::remove_file(&path);
        let storage = SqliteStorage::open(&path).expect("open");
        storage.migrate().expect("first migrate");
        storage.migrate().expect("second migrate");
        assert_eq!(storage.schema_version().expect("version"), 3);
    }

    #[test]
    fn execute_error_surfaces_as_storage_error() {
        let path = temp_db("errors");
        let _ = std::fs::remove_file(&path);
        let storage = SqliteStorage::open(&path).expect("open");
        storage.migrate().expect("migrate");
        assert!(storage
            .execute("INSERT INTO missing_table VALUES (1)", &[])
            .is_err());
    }
}
