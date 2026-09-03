// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Role-based interface (R47): one product, four personas. The role
//! determines the rendered workflow, never the deployment; roles are
//! resettable and overlapping. ICR filing (R25) is a mode of the importer/
//! declarant interface, not a separate persona.

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;

/// The four personas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    /// Exporter / mill / producer — the data originator (produces the packs
    /// their counterparties need).
    Exporter,
    /// Trading house — optional hop: aggregate, mask per field, pass packs
    /// through, per-buyer coverage.
    TradingHouse,
    /// Importer / declarant — the obligated party (ICR filing is a mode).
    ImporterDeclarant,
    /// Verifier — accredited reviewer, sits outside the trade loop.
    Verifier,
}

impl Role {
    /// All four roles in first-run display order.
    pub const ALL: [Role; 4] = [
        Role::Exporter,
        Role::TradingHouse,
        Role::ImporterDeclarant,
        Role::Verifier,
    ];

    /// Canonical persisted string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Exporter => "EXPORTER",
            Role::TradingHouse => "TRADING_HOUSE",
            Role::ImporterDeclarant => "IMPORTER_DECLARANT",
            Role::Verifier => "VERIFIER",
        }
    }

    /// Parse the canonical string.
    ///
    /// # Errors
    ///
    /// [`DomainError::Storage`] for an unknown role token.
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s.trim() {
            "EXPORTER" => Ok(Role::Exporter),
            "TRADING_HOUSE" => Ok(Role::TradingHouse),
            "IMPORTER_DECLARANT" => Ok(Role::ImporterDeclarant),
            "VERIFIER" => Ok(Role::Verifier),
            other => Err(DomainError::Storage(format!("unknown role `{other}`"))),
        }
    }

    /// The workflow keys this role renders (in nav order). Overlapping
    /// roles union their workflows.
    #[must_use]
    pub fn workflows(self) -> Vec<&'static str> {
        match self {
            Role::Exporter => vec![
                "workflow.mill.profile",
                "workflow.pack.attach_verify",
                "workflow.pack.preview_export",
                "workflow.pack.reusable",
            ],
            Role::TradingHouse => vec![
                "workflow.trader.suppliers",
                "workflow.trader.data_requests",
                "workflow.trader.pack_passthrough",
                "workflow.trader.buyer_coverage",
            ],
            Role::ImporterDeclarant => vec![
                "workflow.importer.consignments",
                "workflow.importer.deminimis_watch",
                "workflow.importer.exposure",
                "workflow.importer.dossier_assembly",
                "workflow.importer.icr_mode",
                "workflow.importer.declaration_export",
            ],
            Role::Verifier => vec![
                "workflow.verifier.dossier_review",
                "workflow.verifier.chain_check",
                "workflow.verifier.findings",
                "workflow.verifier.attestation",
            ],
        }
    }
}

/// A user's role configuration: one or more overlapping roles, one active.
/// Persisted locally (settings table), resettable at any time — switching
/// roles never requires a reinstall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSelection {
    roles: Vec<Role>,
    active: Role,
}

impl RoleSelection {
    /// First-run selection: the user picks who they are.
    #[must_use]
    pub fn first_run(active: Role) -> Self {
        Self {
            roles: vec![active],
            active,
        }
    }

    /// Add an overlapping role and make it active (idempotent).
    pub fn add_role(&mut self, role: Role) {
        if !self.roles.contains(&role) {
            self.roles.push(role);
        }
        self.active = role;
    }

    /// Switch the active role.
    ///
    /// # Errors
    ///
    /// [`DomainError::Storage`] when the role is not among the configured
    /// roles — add it first via [`RoleSelection::add_role`].
    pub fn switch_active(&mut self, role: Role) -> Result<(), DomainError> {
        if !self.roles.contains(&role) {
            return Err(DomainError::Storage(format!(
                "role `{}` not configured",
                role.as_str()
            )));
        }
        self.active = role;
        Ok(())
    }

    /// The active role.
    #[must_use]
    pub fn active(&self) -> Role {
        self.active
    }

    /// All configured roles (insertion order).
    #[must_use]
    pub fn roles(&self) -> &[Role] {
        &self.roles
    }

    /// The union of workflows across configured roles, active role first.
    #[must_use]
    pub fn rendered_workflows(&self) -> Vec<&'static str> {
        let mut out = self.active.workflows();
        for role in &self.roles {
            if *role != self.active {
                for wf in role.workflows() {
                    if !out.contains(&wf) {
                        out.push(wf);
                    }
                }
            }
        }
        out
    }
}

/// Serialize the selection for the settings table (canonical JSON).
#[must_use]
pub fn persist(selection: &RoleSelection) -> String {
    serde_json::to_string(selection).unwrap_or_else(|_| {
        format!(
            "{{\"roles\":[\"{}\"],\"active\":\"{}\"}}",
            selection.active.as_str(),
            selection.active.as_str()
        )
    })
}

/// Restore a selection from its persisted form.
///
/// # Errors
///
/// [`DomainError::Storage`] on malformed JSON or unknown role tokens.
pub fn restore(json: &str) -> Result<RoleSelection, DomainError> {
    serde_json::from_str(json)
        .map_err(|e| DomainError::Storage(format!("corrupt role selection: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REGULATORY PIN (R47): first-run role selection renders the right
    /// workflow per persona; overlapping roles switch without reinstall.
    #[test]
    fn first_run_selection_renders_role_workflow() {
        let mill = RoleSelection::first_run(Role::Exporter);
        let wf = mill.rendered_workflows();
        assert!(wf.contains(&"workflow.pack.attach_verify"));
        assert!(wf.contains(&"workflow.pack.preview_export"));
        assert!(!wf.contains(&"workflow.importer.declaration_export"));

        let importer = RoleSelection::first_run(Role::ImporterDeclarant);
        let wf = importer.rendered_workflows();
        assert!(wf.contains(&"workflow.importer.consignments"));
        assert!(
            wf.contains(&"workflow.importer.icr_mode"),
            "ICR is a mode of importer"
        );
        assert!(!wf.contains(&"workflow.verifier.findings"));
    }

    #[test]
    fn overlapping_roles_switch_without_reinstall() {
        let mut sel = RoleSelection::first_run(Role::TradingHouse);
        sel.add_role(Role::ImporterDeclarant);
        assert_eq!(sel.active(), Role::ImporterDeclarant);
        sel.switch_active(Role::TradingHouse).expect("switch");
        assert_eq!(sel.active(), Role::TradingHouse);

        let wf = sel.rendered_workflows();
        assert!(wf.contains(&"workflow.trader.pack_passthrough"));
        assert!(wf.contains(&"workflow.importer.exposure"));

        // A role that was never added cannot be activated.
        assert!(sel.switch_active(Role::Verifier).is_err());
    }

    #[test]
    fn role_selection_persists_and_restores() {
        let mut sel = RoleSelection::first_run(Role::Verifier);
        sel.add_role(Role::Exporter);
        let json = persist(&sel);
        let back = restore(&json).expect("restore");
        assert_eq!(back, sel);
        assert!(restore("{\"roles\":[\"WIZARD\"]}",).is_err());
        assert_eq!(
            Role::parse("TRADING_HOUSE").expect("parse"),
            Role::TradingHouse
        );
    }
}
