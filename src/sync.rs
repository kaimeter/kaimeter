// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Sync layer (R7/R11/R14/R36): ETS price cache with staleness flag,
//! localized data requests, and registry status refresh — every networked
//! feature degrades gracefully offline and NEVER blocks the math.

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;

// ---------------------------------------------------------------------------
// R7/R14 — ETS price cache
// ---------------------------------------------------------------------------

/// One ETS price observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EtsPrice {
    /// Euros per tCO2e.
    pub eur_per_tco2e: f64,
    /// Observation date, ISO `YYYY-MM-DD`.
    pub as_of_iso: String,
}

/// A cached price plus its staleness state (offline mode uses the cached
/// value with a visible staleness flag, or a manual entry — R7/R22).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedPrice {
    /// The price to use in projections.
    pub price: EtsPrice,
    /// True when the cache came from manual entry.
    pub manual: bool,
    /// True when a refresh was attempted after this price was cached and
    /// the network was unavailable.
    pub stale: bool,
}

impl CachedPrice {
    /// Construct a cache entry that is fresh by definition (just synced).
    #[must_use]
    pub fn fresh(price: EtsPrice) -> Self {
        Self {
            price,
            manual: false,
            stale: false,
        }
    }
}

/// The price cache. `None` until the first sync or manual entry —
/// projections then surface "no price" instead of blocking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EtsPriceCache {
    entry: Option<CachedPrice>,
}

impl EtsPriceCache {
    /// An empty cache (nothing synced yet).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt a refresh with the given fetch closure. Offline (closure
    /// returns `None`), the cached price is kept and flagged stale. The
    /// call never blocks and never errors on network absence (R22).
    pub fn refresh(&mut self, fetch: impl FnOnce() -> Option<EtsPrice>) {
        match fetch() {
            Some(price) => {
                self.entry = Some(CachedPrice {
                    price,
                    manual: false,
                    stale: false,
                });
            }
            None => {
                if let Some(entry) = &mut self.entry {
                    entry.stale = true;
                }
            }
        }
    }

    /// Manual entry (R7/R22 fallback). Marks the entry manual and fresh —
    /// the user is the source of truth for a hand-entered price.
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidEtsPrice`] for a negative or non-finite price.
    pub fn manual_entry(&mut self, eur_per_tco2e: f64, as_of_iso: &str) -> Result<(), DomainError> {
        if !(eur_per_tco2e.is_finite() && eur_per_tco2e >= 0.0) {
            return Err(DomainError::InvalidEtsPrice(eur_per_tco2e));
        }
        self.entry = Some(CachedPrice {
            price: EtsPrice {
                eur_per_tco2e,
                as_of_iso: as_of_iso.to_string(),
            },
            manual: true,
            stale: false,
        });
        Ok(())
    }

    /// The price to project with, when one exists.
    #[must_use]
    pub fn current(&self) -> Option<CachedPrice> {
        self.entry.clone()
    }
}

// ---------------------------------------------------------------------------
// R11 — localized data requests
// ---------------------------------------------------------------------------

/// The data a declarant needs from a mill (R11): process route, electricity
/// mix, supplier inputs, mass per CN code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataRequest {
    /// Request identifier.
    pub id: String,
    /// Locale the request renders in (`en`, `zh-CN` at launch).
    pub locale: String,
    /// The mill/supplier this request goes to.
    pub recipient: String,
    /// CN codes the request covers.
    pub cn_codes: Vec<String>,
    /// Outbox state.
    pub queued: bool,
}

/// Build a localized data request: English and Simplified Chinese at
/// launch; other languages come later (unknown locales fall back to `en`).
#[must_use]
pub fn data_request(id: &str, locale: &str, recipient: &str, cn_codes: &[String]) -> DataRequest {
    let locale = match locale {
        "zh-CN" => "zh-CN".to_string(),
        _ => "en".to_string(),
    };
    DataRequest {
        id: id.to_string(),
        locale,
        recipient: recipient.to_string(),
        cn_codes: cn_codes.to_vec(),
        queued: true,
    }
}

/// The localized template strings for a request (subject + item keys).
///
/// # Errors
///
/// [`DomainError::Storage`] when the request locale is unknown.
pub fn request_template(
    request: &DataRequest,
) -> Result<(&'static str, &'static str), DomainError> {
    match request.locale.as_str() {
        "en" => Ok((
            "CBAM data request",
            "Please provide: process route, electricity mix, supplier inputs, mass per CN code.",
        )),
        "zh-CN" => Ok((
            "CBAM 数据请求",
            "请提供：工艺路线、用电结构、上游供应商投入、各 CN 编码质量。",
        )),
        other => Err(DomainError::Storage(format!(
            "unknown request locale `{other}`"
        ))),
    }
}

/// Drain the request outbox with the given send closure. Offline (closure
/// returns `false`), requests stay queued — nothing is lost, nothing
/// blocks. Returns the ids that were sent.
pub fn drain_outbox(
    requests: &mut [DataRequest],
    send: impl FnMut(&DataRequest) -> bool,
) -> Vec<String> {
    let _ = (requests, send);
    todo!("0.6.0: outbox drain (owned; used by the web layer)")
}

// ---------------------------------------------------------------------------
// R36 — registry status refresh
// ---------------------------------------------------------------------------

/// A registry status snapshot (operator registration or declarant
/// authorisation) with its refresh state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegistryStatus {
    /// Subject of the status (operator id or EORI).
    pub subject: String,
    /// Status token as last seen.
    pub status: String,
    /// Last successful refresh, ISO `YYYY-MM-DD`.
    pub refreshed_iso: String,
    /// True when a refresh attempt after this date could not reach the
    /// registry (offline-first: the cached status remains usable).
    pub stale: bool,
}

/// Attempt a registry status refresh; offline keeps the cached status and
/// flags it stale. Never blocks (R36 × R22).
pub fn refresh_registry_status(
    current: &mut RegistryStatus,
    fetch: impl FnOnce() -> Option<(String, String)>,
) {
    match fetch() {
        Some((subject, status)) => {
            *current = RegistryStatus {
                subject,
                status,
                refreshed_iso: current.refreshed_iso.clone(),
                stale: false,
            };
        }
        None => current.stale = true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REGULATORY PIN (R22): the math never blocks on the network — offline
    /// refresh keeps the cached price with a visible staleness flag.
    #[test]
    fn offline_refresh_flags_stale_and_keeps_cache() {
        let mut cache = EtsPriceCache::new();
        assert!(cache.current().is_none(), "empty until first sync");

        // Seed with the first published certificate price (2026-04-07).
        cache
            .manual_entry(75.36, "2026-04-07")
            .expect("manual entry");
        let entry = cache.current().expect("entry");
        assert!(entry.manual);
        assert!(!entry.stale);

        // Offline refresh: closure yields nothing.
        cache.refresh(|| None);
        let entry = cache.current().expect("cache survives offline refresh");
        assert!(entry.stale, "visible staleness flag");
        assert!((entry.price.eur_per_tco2e - 75.36).abs() < 1e-12);

        // A later successful sync clears the flag.
        cache.refresh(|| {
            Some(EtsPrice {
                eur_per_tco2e: 80.0,
                as_of_iso: "2027-01-04".into(),
            })
        });
        let entry = cache.current().expect("entry");
        assert!(!entry.stale);
        assert!((entry.price.eur_per_tco2e - 80.0).abs() < 1e-12);
    }

    #[test]
    fn manual_price_rejects_garbage() {
        let mut cache = EtsPriceCache::new();
        assert!(matches!(
            cache.manual_entry(-1.0, "2026-04-07"),
            Err(DomainError::InvalidEtsPrice(_))
        ));
        assert!(cache.manual_entry(f64::NAN, "2026-04-07").is_err());
    }

    /// REGULATORY PIN (R11): requests localize to en + zh-CN at launch and
    /// fall back to English otherwise.
    #[test]
    fn data_requests_localize_with_fallback() {
        let codes = vec!["73181500".to_string()];
        let zh = data_request("r1", "zh-CN", "某钢铁厂", &codes);
        let (subject, _) = request_template(&zh).expect("zh template");
        assert_eq!(subject, "CBAM 数据请求");

        let en = data_request("r2", "en", "mill", &codes);
        assert_eq!(request_template(&en).expect("en").0, "CBAM data request");

        // Unknown locale falls back to en at construction.
        let fallback = data_request("r3", "it", "mill", &codes);
        assert_eq!(fallback.locale, "en");
    }

    #[test]
    fn registry_status_degrades_offline() {
        let mut status = RegistryStatus {
            subject: "OP-1".into(),
            status: "REGISTERED".into(),
            refreshed_iso: "2026-09-01".into(),
            stale: false,
        };
        refresh_registry_status(&mut status, || None);
        assert!(status.stale);
        assert_eq!(status.status, "REGISTERED", "cached status stays usable");
    }
}
