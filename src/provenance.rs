// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Provenance: hash-chained audit trail (R10), attachment records (R16),
//! and the retention/purge scheduler (R27).
//!
//! The SHA-256 chain below is frozen and tested; attachments, retention and
//! export formats build on it.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// Hash-chained audit trail (R10) — frozen core
// ---------------------------------------------------------------------------

/// One append-only audit event. `hash` covers the full event including the
/// previous event's hash, so any post-hoc edit breaks the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Zero-based position in the chain.
    pub seq: u64,
    /// UTC timestamp, ISO-8601 `YYYY-MM-DDTHH:MM:SSZ`.
    pub ts_utc: String,
    /// Actor (user, role, or system component).
    pub actor: String,
    /// Action verb, e.g. `override`, `extraction_confirmed`, `default_fallback`.
    pub action: String,
    /// Subject the action applies to (record/dossier id).
    pub subject: String,
    /// SHA-256 hex digest of the action payload (the what-changed record).
    pub payload_hash: String,
    /// SHA-256 hex digest of the previous event (`64` zeros for the genesis).
    pub prev_hash: String,
    /// SHA-256 hex digest of this event (the chain link).
    pub hash: String,
}

/// Compute the chain hash of an event body: SHA-256 over
/// `seq|ts|actor|action|subject|payload_hash|prev_hash`.
#[must_use]
pub fn chain_hash(
    seq: u64,
    ts_utc: &str,
    actor: &str,
    action: &str,
    subject: &str,
    payload_hash: &str,
    prev_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seq.to_string().as_bytes());
    for part in [ts_utc, actor, action, subject, payload_hash, prev_hash] {
        hasher.update(b"|");
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// The genesis previous-hash (all zeros).
pub const GENESIS_PREV_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// An append-only, hash-linked audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuditChain {
    events: Vec<AuditEvent>,
}

impl AuditChain {
    /// An empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event to the chain. The hash link is computed here —
    /// callers cannot forge links.
    pub fn append(
        &mut self,
        ts_utc: &str,
        actor: &str,
        action: &str,
        subject: &str,
        payload_hash: &str,
    ) -> &AuditEvent {
        let seq = self.events.len() as u64;
        let prev_hash = self
            .events
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| GENESIS_PREV_HASH.to_string());
        let hash = chain_hash(
            seq,
            ts_utc,
            actor,
            action,
            subject,
            payload_hash,
            &prev_hash,
        );
        let event = AuditEvent {
            seq,
            ts_utc: ts_utc.to_string(),
            actor: actor.to_string(),
            action: action.to_string(),
            subject: subject.to_string(),
            payload_hash: payload_hash.to_string(),
            prev_hash,
            hash,
        };
        self.events.push(event);
        self.events.last().expect("just pushed")
    }

    /// All events in order.
    #[must_use]
    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    /// The chain root: the hash of the last event (`GENESIS_PREV_HASH` when
    /// empty). Ships with the dossier so a verifier can prove non-tampering.
    #[must_use]
    pub fn chain_root(&self) -> &str {
        self.events
            .last()
            .map(|e| e.hash.as_str())
            .unwrap_or(GENESIS_PREV_HASH)
    }

    /// Verify the whole chain end-to-end. Returns `Ok(())` when every link
    /// recomputes correctly; [`DomainError::ChainBroken`] at the first bad
    /// sequence number otherwise.
    pub fn verify(&self) -> Result<(), DomainError> {
        let mut prev = GENESIS_PREV_HASH.to_string();
        for event in &self.events {
            let expected = chain_hash(
                event.seq,
                &event.ts_utc,
                &event.actor,
                &event.action,
                &event.subject,
                &event.payload_hash,
                &prev,
            );
            if event.prev_hash != prev || event.hash != expected {
                return Err(DomainError::ChainBroken(event.seq));
            }
            prev = event.hash.clone();
        }
        Ok(())
    }
}

/// SHA-256 hex digest of arbitrary bytes (attachment/document hashing).
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Attachments (R16) — agent-owned
// ---------------------------------------------------------------------------

/// A source document attached to a record as provenance. The document never
/// leaves the device (R16/R22); only its hash and metadata are stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Stable attachment identifier.
    pub id: String,
    /// Original file name.
    pub filename: String,
    /// MIME type (e.g. `application/pdf`, `image/jpeg`).
    pub mime_type: String,
    /// SHA-256 hex digest of the file bytes.
    pub sha256: String,
    /// True once the end-user verified prefilled fields against this source.
    pub verified_by_human: bool,
    /// Free-text note (which fields were verified, by whom).
    pub verification_note: String,
}

/// Create a verified attachment record, hashing the file bytes.
///
/// # Errors
///
/// [`DomainError::HumanVerificationRequired`] when `verified_by_human` is
/// false — R16: the verifying human is the author of the record; nothing is
/// retained as provenance without human verification.
pub fn new_attachment(
    id: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
    verified_by_human: bool,
    verification_note: &str,
) -> Result<Attachment, DomainError> {
    // R16: the end-user must verify each entry against the attached source
    // before anything is saved; the verifying human is the author of the
    // record. No human verification, no retained provenance.
    if !verified_by_human {
        return Err(DomainError::HumanVerificationRequired(id.to_string()));
    }
    // Only the hash and metadata are stored — the document never leaves the
    // device (R16/R22).
    Ok(Attachment {
        id: id.to_string(),
        filename: filename.to_string(),
        mime_type: mime_type.to_string(),
        sha256: sha256_hex(bytes),
        verified_by_human: true,
        verification_note: verification_note.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Retention & purge (R27) — agent-owned
// ---------------------------------------------------------------------------

/// Retention status of one record under the Art 14 four-year rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionStatus {
    /// The declaration (filing) year the record belongs to.
    pub declaration_year: i32,
    /// ISO date the record may be purged: December 31 of
    /// `declaration_year + 4`.
    pub purge_after_iso: String,
}

/// Compute the retention horizon for a declaration year (Art 14 CBAM Reg:
/// records kept for 4 years after the declaration year).
#[must_use]
pub fn retention_expiry(declaration_year: i32) -> RetentionStatus {
    // Art 14 CBAM Reg (R27): 4 calendar years AFTER the declaration year —
    // the record is retained through December 31 of `declaration_year + 4`.
    RetentionStatus {
        declaration_year,
        purge_after_iso: format!("{:04}-12-31", declaration_year + 4),
    }
}

/// The purge decision for one record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurgeDecision {
    /// Still inside the retention window.
    Retain,
    /// Past the window: hard-delete personal data.
    Purge,
}

/// Decide whether a record may be purged on `today_iso`.
///
/// The purge fires only when `today_iso` is strictly AFTER
/// `status.purge_after_iso`: the record is retained THROUGH December 31 of
/// `declaration_year + 4` (Art 14 CBAM Reg, R27). Dates are compared as day
/// counts via the calendar's proleptic Gregorian day numbers, so the
/// comparison is calendar-exact.
///
/// # Errors
///
/// [`DomainError::InvalidImportDate`] when `today_iso` does not parse.
pub fn purge_decision(
    status: &RetentionStatus,
    today_iso: &str,
) -> Result<PurgeDecision, DomainError> {
    let (ty, tm, td) = crate::calendar::parse_iso(today_iso)?;
    let today = crate::calendar::days_from_epoch(ty, tm, td);
    // `purge_after_iso` is generated by `retention_expiry` and always parses;
    // a hand-edited status must still fail loudly rather than purge early.
    let (py, pm, pd) = crate::calendar::parse_iso(&status.purge_after_iso)?;
    let purge_after = crate::calendar::days_from_epoch(py, pm, pd);
    if today > purge_after {
        Ok(PurgeDecision::Purge)
    } else {
        Ok(PurgeDecision::Retain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev_payload(n: u64) -> String {
        sha256_hex(n.to_string().as_bytes())
    }

    /// REGULATORY PIN: the chain is append-only, links bind to the previous
    /// hash, and any tampering is detected (R10).
    #[test]
    fn hash_chain_detects_tampering() {
        let mut chain = AuditChain::new();
        chain.append(
            "2026-09-01T10:00:00Z",
            "mill:user",
            "extraction_confirmed",
            "cons-1",
            &ev_payload(1),
        );
        chain.append(
            "2026-09-01T10:05:00Z",
            "mill:user",
            "override",
            "cons-1",
            &ev_payload(2),
        );
        chain.append(
            "2026-09-01T10:07:00Z",
            "core",
            "default_fallback",
            "cons-2",
            &ev_payload(3),
        );
        assert_eq!(chain.events().len(), 3);
        assert!(chain.verify().is_ok());

        // The root is the last event's hash.
        let root = chain.chain_root().to_string();

        // Tamper with a mid-chain event's payload: verification must fail
        // exactly there.
        let mut tampered = chain.clone();
        tampered.events[1].payload_hash = ev_payload(999);
        assert!(matches!(
            tampered.verify(),
            Err(DomainError::ChainBroken(seq)) if seq == 1
        ));

        // Removing the tail changes the root (dossier ships the root).
        let mut truncated = chain.clone();
        truncated.events.pop();
        assert_ne!(truncated.chain_root(), root);
        assert!(truncated.verify().is_ok(), "a valid prefix still verifies");

        // A forged first event (wrong genesis link) is rejected.
        let mut forged = AuditChain::new();
        forged.append(
            "2026-09-01T10:00:00Z",
            "attacker",
            "override",
            "cons-1",
            &ev_payload(1),
        );
        forged.events[0].prev_hash = "ff".repeat(32);
        assert!(matches!(forged.verify(), Err(DomainError::ChainBroken(0))));
    }

    /// The chain is deterministic: same inputs, same hash links.
    #[test]
    fn chain_hash_is_deterministic_and_sensitive() {
        let a = chain_hash(0, "t", "actor", "action", "subject", "p", GENESIS_PREV_HASH);
        let b = chain_hash(0, "t", "actor", "action", "subject", "p", GENESIS_PREV_HASH);
        assert_eq!(a, b);
        assert_ne!(
            chain_hash(1, "t", "actor", "action", "subject", "p", GENESIS_PREV_HASH),
            a,
            "sequence number is part of the hash"
        );
        assert_eq!(a.len(), 64, "sha256 hex");
    }

    #[test]
    fn sha256_hex_pins_known_vector() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// REGULATORY PIN: Art 14 CBAM Reg (R27) — retention runs through
    /// December 31 of `declaration_year + 4`, leap year or not.
    #[test]
    fn retention_expiry_is_dec31_of_year_plus_four() {
        for (year, expected) in [
            (2024, "2028-12-31"), // 2028 is a leap year; Dec 31 still valid
            (2025, "2029-12-31"),
            (2026, "2030-12-31"),
            (2027, "2031-12-31"),
        ] {
            let status = retention_expiry(year);
            assert_eq!(status.declaration_year, year);
            assert_eq!(status.purge_after_iso, expected);
            // The generated horizon must itself be a real ISO date.
            assert!(crate::calendar::parse_iso(&status.purge_after_iso).is_ok());
        }
    }

    /// The purge boundary is strict for every pinned year: Retain ON the
    /// horizon date, Purge the day after (R27 / Art 14).
    #[test]
    fn purge_is_strict_across_years() {
        for year in [2024, 2025, 2026, 2027] {
            let status = retention_expiry(year);
            let on = purge_decision(&status, &status.purge_after_iso).expect("valid date");
            assert_eq!(
                on,
                PurgeDecision::Retain,
                "year {year}: retained through the horizon"
            );
            let (y, m, d) = crate::calendar::parse_iso(&status.purge_after_iso).expect("valid");
            let (ny, nm, nd) =
                crate::calendar::civil_from_days(crate::calendar::days_from_epoch(y, m, d) + 1);
            let next = format!("{ny:04}-{nm:02}-{nd:02}");
            let after = purge_decision(&status, &next).expect("valid date");
            assert_eq!(after, PurgeDecision::Purge, "year {year}: purged on {next}");
        }
    }
}
