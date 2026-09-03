// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Trade-flow isolation (R25/R26/R46): ICR workspaces with per-EORI
//! PHYSICAL tenant segregation, group/multi-installation hierarchies, and
//! joint-and-several liability tagging.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// R25 — ICR workspaces with physical tenant isolation
// ---------------------------------------------------------------------------

/// Resolve the physical database path for one EORI's workspace:
/// `db/tenants/{eori}.db` under the data directory. Per-EORI physical
/// segregation prevents cross-tenant leaks at rest — stronger than
/// `tenant_id` WHERE clauses.
///
/// # Errors
///
/// [`DomainError::Storage`] when the EORI fails format validation (see
/// `registry::validate_eori`) — a bad EORI must never become a filename.
pub fn workspace_db_path(data_dir: &Path, eori: &str) -> Result<PathBuf, DomainError> {
    // R25: validation runs BEFORE any path is built — a bad EORI must never
    // become a filename on disk.
    crate::registry::validate_eori(eori).map_err(|e| DomainError::Storage(e.to_string()))?;
    Ok(data_dir.join("tenants").join(format!("{eori}.db")))
}

/// Open (and migrate) the tenant database for one EORI. The returned storage
/// is the ONLY handle that can see that declarant's records.
///
/// # Errors
///
/// Propagates path validation and SQLite open/migration errors.
pub fn open_workspace(
    data_dir: &Path,
    eori: &str,
) -> Result<crate::db::SqliteStorage, DomainError> {
    let db_path = workspace_db_path(data_dir, eori)?;
    let tenants_dir = data_dir.join("tenants");
    std::fs::create_dir_all(&tenants_dir).map_err(|e| {
        DomainError::Storage(format!("create tenants dir {}: {e}", tenants_dir.display()))
    })?;
    let storage = crate::db::SqliteStorage::open(&db_path)
        .map_err(|e| DomainError::Storage(e.to_string()))?;
    storage
        .migrate()
        .map_err(|e| DomainError::Storage(e.to_string()))?;
    Ok(storage)
}

/// Rollup of one declarant's year-to-date position (the 50 t line is watched
/// once across ALL connected ICR workspaces, not per broker — R25).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclarantRollup {
    /// The declarant EORI this rollup belongs to.
    pub eori: String,
    /// Year-to-date net mass across all CBAM goods, kg.
    pub ytd_net_mass_kg: f64,
    /// Dossiers open for this declarant.
    pub open_dossiers: u32,
}

/// Roll up positions across workspaces so a master-importer view sees one
/// aggregate.
#[must_use]
pub fn rollup_workspaces(rolls: &[DeclarantRollup]) -> f64 {
    // R25: the master view watches the 50 t de-minimis line ONCE across ALL
    // connected ICR workspaces, not per broker.
    rolls.iter().map(|roll| roll.ytd_net_mass_kg).sum()
}

// ---------------------------------------------------------------------------
// R26 — group & multi-installation hierarchy
// ---------------------------------------------------------------------------

/// One production site in a group (each carries its distinct CBAM
/// installation ID; site dossiers stay separable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteRecord {
    /// CBAM installation identifier (the verifier-facing unit).
    pub installation_id: String,
    /// Site display name.
    pub name: String,
    /// ISO-3166 alpha-2 country of the site.
    pub country: String,
}

/// A parent-company account over multiple production sites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupHierarchy {
    /// Group identifier.
    pub group_id: String,
    /// Sites under the group.
    pub sites: Vec<SiteRecord>,
}

/// Roll a metric (e.g. embedded emissions) up across a group's sites while
/// site-level boundaries stay separable — group rollups never blur site
/// attribution, so the pairing is returned per site.
///
/// # Errors
///
/// [`DomainError::Storage`] when a site id appears twice.
pub fn group_rollup<'a>(
    group: &'a GroupHierarchy,
    per_site_values: &[(&'a str, f64)],
) -> Result<Vec<(&'a str, f64)>, DomainError> {
    // R26: each installation ID is the verifier-facing unit; rollups never
    // blur site boundaries.
    let known: std::collections::HashSet<&str> = group
        .sites
        .iter()
        .map(|site| site.installation_id.as_str())
        .collect();

    for (site_id, _) in per_site_values {
        if !known.contains(site_id) {
            return Err(DomainError::Storage(format!(
                "group rollup `{}`: unknown site `{site_id}`",
                group.group_id
            )));
        }
    }

    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (site_id, _) in per_site_values {
        if !seen.insert(site_id) {
            return Err(DomainError::Storage(format!(
                "group rollup `{}`: duplicate site `{site_id}`",
                group.group_id
            )));
        }
    }

    let mut paired = Vec::with_capacity(group.sites.len());
    for site in &group.sites {
        let site_id = site.installation_id.as_str();
        let value = per_site_values
            .iter()
            .find(|(id, _)| *id == site_id)
            .map(|(_, v)| *v)
            .ok_or_else(|| {
                DomainError::Storage(format!(
                    "group rollup `{}`: missing value for site `{site_id}`",
                    group.group_id
                ))
            })?;
        paired.push((site_id, value));
    }
    Ok(paired)
}

// ---------------------------------------------------------------------------
// R46 — ICR joint-and-several liability tagging
// ---------------------------------------------------------------------------

/// Liability tag state for an ICR-filed consignment/workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiabilityTag {
    /// The represented importer holds authorised-declarant status.
    None,
    /// The represented importer LACKS status: the ICR carries joint and
    /// several liability for the resulting obligations (Art 5(2) & 26).
    JointAndSeveral,
}

/// Tag an ICR-filed record based on the represented importer's status.
#[must_use]
pub fn liability_tag(importer_has_authorised_status: bool) -> LiabilityTag {
    // R46 (Art 5(2) & 26): liability attaches only where the represented
    // importer lacks authorised-declarant status.
    if importer_has_authorised_status {
        LiabilityTag::None
    } else {
        LiabilityTag::JointAndSeveral
    }
}

/// Aggregate exposure surfaced on the ICR's dashboard: sum of the masses
/// carrying joint-and-several liability, kg.
#[must_use]
pub fn aggregate_liability_mass_kg(masses_kg: &[(LiabilityTag, f64)]) -> f64 {
    // R46: the dashboard exposure counts ONLY joint-and-several mass.
    masses_kg
        .iter()
        .filter(|(tag, _)| *tag == LiabilityTag::JointAndSeveral)
        .map(|(_, mass)| mass)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollup_of_empty_slice_is_zero() {
        assert_eq!(rollup_workspaces(&[]), 0.0);
    }

    #[test]
    fn group_rollup_rejects_empty_values_when_sites_exist() {
        let group = GroupHierarchy {
            group_id: "G".to_string(),
            sites: vec![SiteRecord {
                installation_id: "INST-1".to_string(),
                name: "one".to_string(),
                country: "CN".to_string(),
            }],
        };
        let err = group_rollup(&group, &[]).expect_err("missing site value");
        match err {
            DomainError::Storage(msg) => assert!(msg.contains("INST-1")),
            other => panic!("expected Storage error, got {other:?}"),
        }
    }
}
