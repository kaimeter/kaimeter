// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! In-memory lookups over the seeded reference tables.
//!
//! [`Lookup`] loads `cn_codes`, `default_values`, and `installations` from
//! storage once and serves subsequent reads from memory, with graceful
//! errors for missing CN codes and missing production routes.

use std::collections::BTreeMap;

use crate::db::Storage;
use crate::domain::errors::DomainError;
use crate::domain::markups::MarkupYear;
use crate::domain::types::{CnCode, DefaultValue, Installation, Sector};

/// In-memory snapshot of the CBAM reference tables.
#[derive(Debug, Clone, Default)]
pub struct Lookup {
    cn_codes: BTreeMap<String, CnCode>,
    /// (cn_code, production route) -> default value.
    defaults: BTreeMap<(String, String), DefaultValue>,
    installations: BTreeMap<String, Installation>,
}

impl Lookup {
    /// Load the reference tables from `storage` (must be migrated).
    ///
    /// # Errors
    ///
    /// [`DomainError::Storage`] when a query fails or a seeded row is
    /// malformed (bad CN code, unknown sector, non-numeric number).
    pub fn from_storage(storage: &dyn Storage) -> Result<Self, DomainError> {
        let mut lookup = Self::default();

        let rows = storage
            .query_rows("SELECT code, description, sector FROM cn_codes", &[])
            .map_err(storage_err)?;
        for row in rows {
            let code = cell(&row, 0)?;
            let description = cell(&row, 1)?;
            let sector = Sector::parse(&cell(&row, 2)?)?;
            let cn = CnCode::new(&code, &description, sector)?;
            lookup.cn_codes.insert(code, cn);
        }

        let rows = storage
            .query_rows(
                "SELECT cn_code, production_route, direct_tco2e_per_t, indirect_tco2e_per_t, \
                 markup_2026_percent, markup_2027_percent, markup_2028_percent \
                 FROM default_values",
                &[],
            )
            .map_err(storage_err)?;
        for row in rows {
            let cn = cell(&row, 0)?;
            let route = cell(&row, 1)?;
            let direct: f64 = cell(&row, 2)?.parse().map_err(|_| num_err(&row, 2))?;
            let indirect: f64 = cell(&row, 3)?.parse().map_err(|_| num_err(&row, 3))?;
            let mut markups = BTreeMap::new();
            for (col, bucket) in [
                (4, MarkupYear::Y2026),
                (5, MarkupYear::Y2027),
                (6, MarkupYear::Y2028Plus),
            ] {
                let pct: f64 = cell(&row, col)?.parse().map_err(|_| num_err(&row, col))?;
                markups.insert(bucket, pct);
            }
            let cn_code = lookup.cn_codes.get(&cn).ok_or_else(|| {
                DomainError::Storage(format!("default_values references unknown CN `{cn}`"))
            })?;
            let default = DefaultValue {
                cn_code: cn_code.clone(),
                production_route: route.clone(),
                direct_tco2e_per_t: direct,
                indirect_tco2e_per_t: indirect,
                markups,
            };
            lookup.defaults.insert((cn, route), default);
        }

        let rows = storage
            .query_rows(
                "SELECT id, name, address, production_routes FROM installations",
                &[],
            )
            .map_err(storage_err)?;
        for row in rows {
            let id = cell(&row, 0)?;
            let installation = Installation {
                id: id.clone(),
                name: cell(&row, 1)?,
                address: cell(&row, 2)?,
                production_routes: cell(&row, 3)?
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect(),
            };
            lookup.installations.insert(id, installation);
        }

        Ok(lookup)
    }

    /// The CN code record, if seeded.
    #[must_use]
    pub fn cn_code(&self, code: &str) -> Option<&CnCode> {
        self.cn_codes.get(code)
    }

    /// All seeded CN codes.
    #[must_use]
    pub fn cn_codes(&self) -> Vec<&CnCode> {
        self.cn_codes.values().collect()
    }

    /// All defaults registered for a CN code (possibly empty).
    #[must_use]
    pub fn defaults_for_cn(&self, code: &str) -> Vec<&DefaultValue> {
        self.defaults
            .values()
            .filter(|d| d.cn_code.code() == code)
            .collect()
    }

    /// The default value for one (CN code, production route) pair.
    ///
    /// # Errors
    ///
    /// [`DomainError::NoDefaultForCnCode`] when the CN code has no defaults
    /// at all; [`DomainError::NoDefaultForRoute`] when the code exists but
    /// has no default for `route`.
    pub fn default_for_route(&self, code: &str, route: &str) -> Result<&DefaultValue, DomainError> {
        if let Some(d) = self.defaults.get(&(code.to_string(), route.to_string())) {
            return Ok(d);
        }
        if self.defaults_for_cn(code).is_empty() {
            return Err(DomainError::NoDefaultForCnCode(code.to_string()));
        }
        Err(DomainError::NoDefaultForRoute {
            cn: code.to_string(),
            route: route.to_string(),
        })
    }

    /// The installation record, if seeded.
    #[must_use]
    pub fn installation(&self, id: &str) -> Option<&Installation> {
        self.installations.get(id)
    }

    /// Number of distinct CN codes in the snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cn_codes.len()
    }

    /// True when no reference data is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cn_codes.is_empty()
    }
}

fn storage_err(e: crate::db::StorageError) -> DomainError {
    DomainError::Storage(e.to_string())
}

fn cell(row: &[Option<String>], col: usize) -> Result<String, DomainError> {
    row.get(col)
        .and_then(|c| c.as_ref().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DomainError::Storage(format!("NULL/empty column {col} in reference row")))
}

fn num_err(row: &[Option<String>], col: usize) -> DomainError {
    DomainError::Storage(format!(
        "non-numeric value `{:?}` in column {col}",
        row.get(col).and_then(|c| c.as_deref())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_storage(f: impl Fn(&Lookup)) {
        let dir = std::env::temp_dir().join("kaimeter-lookup-unit");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let storage = crate::db::SqliteStorage::open(&dir.join("t.db")).expect("open");
        storage.migrate().expect("migrate");
        let lookup = Lookup::from_storage(&storage).expect("lookup");
        f(&lookup);
    }

    #[test]
    fn seeded_snapshot_serves_lookups() {
        with_storage(|lookup| {
            assert!(lookup.len() >= 3);
            assert!(!lookup.is_empty());
            assert!(lookup.cn_code("73181500").is_some());
            assert!(lookup.cn_code("00000000").is_none());
            assert!(lookup.installation("INST-DE-001").is_some());
            assert!(lookup.installation("nope").is_none());

            let all = lookup.defaults_for_cn("76041010");
            assert_eq!(all.len(), 2, "PRIMARY and RECYCLED seeded");
            assert!(lookup.default_for_route("73181500", "EF").is_ok());
            assert!(matches!(
                lookup.default_for_route("73181500", "BOGUS"),
                Err(DomainError::NoDefaultForRoute { cn, route })
                    if cn == "73181500" && route == "BOGUS"
            ));
            assert!(matches!(
                lookup.default_for_route("00000000", "EF"),
                Err(DomainError::NoDefaultForCnCode(c)) if c == "00000000"
            ));
        });
    }
}
