// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! i18n layer: JSON locales loaded from a directory, plus a **termbase** —
//! locked compliance terminology that translations must use verbatim.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Locale assets embedded at compile time: the release binary is
/// self-contained (same treatment as the wizard HTML). A `locales`
/// directory on disk — when present and complete — overrides these
/// without a rebuild.
const EMBEDDED_EN: &str = include_str!("../locales/en.json");
const EMBEDDED_ZH_CN: &str = include_str!("../locales/zh-CN.json");
const EMBEDDED_TERMBASE: &str = include_str!("../locales/termbase.json");

/// Where an [`I18n`] state's assets came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleSource {
    /// Loaded from the configured on-disk directory.
    Disk,
    /// Compiled-in assets, used when the configured directory is absent.
    Embedded,
}

impl std::fmt::Display for LocaleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disk => f.write_str("disk"),
            Self::Embedded => f.write_str("embedded"),
        }
    }
}

/// Library error type for locale/termbase handling.
#[derive(Debug, thiserror::Error)]
pub enum I18nError {
    /// A locale file was missing, unreadable, or invalid JSON.
    #[error("locale load failed: {1}")]
    Load(String, #[source] anyhow::Error),
    /// A termbase entry referenced a locale that was never loaded.
    #[error("termbase references unknown locale `{0}`")]
    UnknownLocaleInTermbase(String),
    /// A lookup key was absent from a locale.
    #[error("missing key `{0}` in locale `{1}`")]
    MissingKey(String, String),
}

/// A single loaded locale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Locale {
    /// BCP-47-ish code, e.g. `en`, `zh-CN`.
    pub code: String,
    /// Flat `key -> translated string` map.
    pub messages: BTreeMap<String, String>,
}

/// Locked compliance terminology. Terms here are normative: translations must
/// use the exact term (e.g. the Chinese rendering of "embedded emission" is
/// fixed and must not vary between screens or documents).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Termbase {
    /// term (canonical English) -> locale code -> locked rendering.
    pub terms: BTreeMap<String, BTreeMap<String, String>>,
}

impl Termbase {
    /// Look up the locked rendering of `term` for `locale`, if locked.
    pub fn term(&self, term: &str, locale: &str) -> Option<&str> {
        self.terms.get(term)?.get(locale).map(String::as_str)
    }

    /// Validate that every termbase locale reference is actually loaded.
    pub fn validate(&self, loaded: &[Locale]) -> Result<(), I18nError> {
        for (term, renderings) in &self.terms {
            for code in renderings.keys() {
                if !loaded.iter().any(|l| &l.code == code) {
                    return Err(I18nError::UnknownLocaleInTermbase(code.clone()));
                }
            }
            let _ = term;
        }
        Ok(())
    }
}

/// The whole i18n state: locales + termbase.
#[derive(Debug, Clone, Default)]
pub struct I18n {
    locales: BTreeMap<String, Locale>,
    termbase: Termbase,
}

impl I18n {
    /// Load `en.json`, `zh-CN.json`, and `termbase.json` from `dir`, validate
    /// termbase references, and return the ready i18n state.
    pub fn load(dir: &Path) -> Result<Self, I18nError> {
        let en = load_locale(dir, "en")?;
        let zh = load_locale(dir, "zh-CN")?;
        let termbase = load_termbase(dir)?;
        Self::build(en, zh, termbase)
    }

    /// Load from `dir` when it exists, else fall back to the compiled-in
    /// assets. The binary ships self-contained, so an absent locales
    /// directory is the normal single-file deployment, not an error. A
    /// directory that *exists* but is missing or has invalid files is a
    /// hard error: an operator who configured a locales directory gets
    /// strict behaviour, never a silent fallback to the embedded defaults.
    pub fn load_or_embedded(dir: &Path) -> Result<(Self, LocaleSource), I18nError> {
        if dir.exists() {
            Ok((Self::load(dir)?, LocaleSource::Disk))
        } else {
            let en = parse_locale("en", EMBEDDED_EN)?;
            let zh = parse_locale("zh-CN", EMBEDDED_ZH_CN)?;
            let termbase = parse_termbase(EMBEDDED_TERMBASE)?;
            Ok((Self::build(en, zh, termbase)?, LocaleSource::Embedded))
        }
    }

    /// Assemble and validate: every termbase locale must be loaded, and every
    /// locale must carry the same message keys as `en`. `en` is canonical —
    /// a missing key in any other locale is a hard error, so a
    /// half-translated locale can never ship.
    fn build(en: Locale, zh: Locale, termbase: Termbase) -> Result<Self, I18nError> {
        let i18n = Self {
            locales: BTreeMap::from([(en.code.clone(), en), (zh.code.clone(), zh)]),
            termbase,
        };
        let loaded: Vec<Locale> = i18n.locales.values().cloned().collect();
        i18n.termbase.validate(&loaded)?;
        if let Some(en) = i18n.locales.get("en") {
            let en_keys: BTreeSet<&String> = en.messages.keys().collect();
            for (code, locale) in &i18n.locales {
                if code == "en" {
                    continue;
                }
                for key in &en_keys {
                    if !locale.messages.contains_key(*key) {
                        return Err(I18nError::MissingKey((*key).clone(), code.clone()));
                    }
                }
            }
        }
        Ok(i18n)
    }

    /// Resolve a message key for a locale code.
    pub fn t(&self, locale: &str, key: &str) -> Result<&str, I18nError> {
        self.locales
            .get(locale)
            .ok_or_else(|| I18nError::MissingKey(key.to_string(), locale.to_string()))?
            .messages
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| I18nError::MissingKey(key.to_string(), locale.to_string()))
    }

    /// Resolve a message key, falling back to `en` when missing.
    pub fn t_or_en(&self, locale: &str, key: &str) -> String {
        self.t(locale, key)
            .or_else(|_| self.t("en", key))
            .unwrap_or(key)
            .to_string()
    }

    /// The locked compliance term for `term` in `locale`, falling back to the
    /// canonical (English) term.
    pub fn term(&self, locale: &str, term: &str) -> String {
        self.termbase.term(term, locale).unwrap_or(term).to_string()
    }

    /// Loaded locale codes, sorted.
    pub fn locale_codes(&self) -> Vec<String> {
        self.locales.keys().cloned().collect()
    }
}

fn read_json(path: &Path) -> anyhow::Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

fn load_locale(dir: &Path, code: &str) -> Result<Locale, I18nError> {
    let path = dir.join(format!("{code}.json"));
    let raw = read_json(&path).map_err(|e| {
        I18nError::Load(
            code.to_string(),
            anyhow::anyhow!("read {}: {e}", path.display()),
        )
    })?;
    let messages: BTreeMap<String, String> = serde_json::from_str(&raw).map_err(|e| {
        I18nError::Load(
            code.to_string(),
            anyhow::anyhow!("parse {}: {e}", path.display()),
        )
    })?;
    Ok(Locale {
        code: code.to_string(),
        messages,
    })
}

fn load_termbase(dir: &Path) -> Result<Termbase, I18nError> {
    let path = dir.join("termbase.json");
    let raw = read_json(&path).map_err(|e| {
        I18nError::Load(
            "termbase".to_string(),
            anyhow::anyhow!("read {}: {e}", path.display()),
        )
    })?;
    serde_json::from_str(&raw).map_err(|e| {
        I18nError::Load(
            "termbase".to_string(),
            anyhow::anyhow!("parse {}: {e}", path.display()),
        )
    })
}

fn parse_locale(code: &str, raw: &'static str) -> Result<Locale, I18nError> {
    let messages: BTreeMap<String, String> = serde_json::from_str(raw).map_err(|e| {
        I18nError::Load(
            code.to_string(),
            anyhow::anyhow!("parse embedded {code} locale: {e}"),
        )
    })?;
    Ok(Locale {
        code: code.to_string(),
        messages,
    })
}

fn parse_termbase(raw: &'static str) -> Result<Termbase, I18nError> {
    serde_json::from_str(raw).map_err(|e| {
        I18nError::Load(
            "termbase".to_string(),
            anyhow::anyhow!("parse embedded termbase: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kaimeter-i18n-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn loads_en_and_zh_cn_and_resolves_welcome() {
        let dir = fixture_dir("ok");
        std::fs::write(dir.join("en.json"), r#"{"welcome":"Welcome to Kaimeter"}"#).unwrap();
        std::fs::write(dir.join("zh-CN.json"), r#"{"welcome":"欢迎使用 Kaimeter"}"#).unwrap();
        std::fs::write(
            dir.join("termbase.json"),
            r#"{"terms":{"embedded emissions":{"zh-CN":"隐含排放"}}}"#,
        )
        .unwrap();

        let i18n = I18n::load(&dir).expect("load");
        assert_eq!(
            i18n.locale_codes(),
            vec!["en".to_string(), "zh-CN".to_string()]
        );
        assert_eq!(i18n.t("en", "welcome").unwrap(), "Welcome to Kaimeter");
        assert_eq!(i18n.t("zh-CN", "welcome").unwrap(), "欢迎使用 Kaimeter");
        // Termbase: locked term for zh-CN, canonical fallback for en.
        assert_eq!(i18n.term("zh-CN", "embedded emissions"), "隐含排放");
        assert_eq!(i18n.term("en", "embedded emissions"), "embedded emissions");
        // Fallback path.
        assert_eq!(i18n.t_or_en("fr", "welcome"), "Welcome to Kaimeter");
    }

    #[test]
    fn missing_locale_file_is_an_error() {
        let dir = fixture_dir("missing");
        std::fs::write(dir.join("en.json"), "{}").unwrap();
        // zh-CN.json missing.
        assert!(matches!(
            I18n::load(&dir),
            Err(I18nError::Load(code, _)) if code == "zh-CN"
        ));
    }

    #[test]
    fn termbase_referencing_unknown_locale_is_rejected() {
        let dir = fixture_dir("badterm");
        std::fs::write(dir.join("en.json"), "{}").unwrap();
        std::fs::write(dir.join("zh-CN.json"), "{}").unwrap();
        std::fs::write(
            dir.join("termbase.json"),
            r#"{"terms":{"x":{"fr":"unknown locale"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            I18n::load(&dir),
            Err(I18nError::UnknownLocaleInTermbase(code)) if code == "fr"
        ));
    }

    #[test]
    fn locale_missing_a_key_from_en_is_rejected() {
        let dir = fixture_dir("parity");
        std::fs::write(dir.join("en.json"), r#"{"a":"1","b":"2"}"#).unwrap();
        std::fs::write(dir.join("zh-CN.json"), r#"{"a":"一"}"#).unwrap();
        std::fs::write(dir.join("termbase.json"), r#"{"terms":{}}"#).unwrap();
        assert!(matches!(
            I18n::load(&dir),
            Err(I18nError::MissingKey(k, l)) if k == "b" && l == "zh-CN"
        ));
    }

    #[test]
    fn missing_key_is_an_error() {
        let dir = fixture_dir("missingkey");
        std::fs::write(dir.join("en.json"), r#"{"a":"1"}"#).unwrap();
        std::fs::write(dir.join("zh-CN.json"), r#"{"a":"1"}"#).unwrap();
        std::fs::write(dir.join("termbase.json"), r#"{"terms":{}}"#).unwrap();
        let i18n = I18n::load(&dir).expect("load");
        assert!(matches!(
            i18n.t("en", "nope"),
            Err(I18nError::MissingKey(k, l)) if k == "nope" && l == "en"
        ));
    }

    #[test]
    fn absent_dir_falls_back_to_embedded_assets() {
        let dir = std::env::temp_dir().join("kaimeter-i18n-embedded-absent");
        let _ = std::fs::remove_dir_all(&dir);
        let (i18n, source) = I18n::load_or_embedded(&dir).expect("embedded fallback");
        assert_eq!(source, LocaleSource::Embedded);
        assert_eq!(
            i18n.locale_codes(),
            vec!["en".to_string(), "zh-CN".to_string()]
        );
        // The embedded assets are the repo's own locale files.
        assert_eq!(i18n.t("en", "welcome").unwrap(), "Welcome to Kaimeter");
        assert_eq!(i18n.t("zh-CN", "welcome").unwrap(), "欢迎使用 Kaimeter");
        assert_eq!(i18n.term("zh-CN", "embedded emissions"), "隐含排放");
    }

    #[test]
    fn existing_but_incomplete_dir_is_an_error_not_a_fallback() {
        let dir = fixture_dir("fallback-strict");
        std::fs::write(dir.join("en.json"), r#"{"welcome":"hi"}"#).unwrap();
        // zh-CN.json is missing, and the directory itself exists: the
        // operator pointed kaimeter here, so this must stay a hard error.
        assert!(matches!(
            I18n::load_or_embedded(&dir),
            Err(I18nError::Load(code, _)) if code == "zh-CN"
        ));
    }

    #[test]
    fn complete_dir_overrides_embedded_assets() {
        let dir = fixture_dir("override");
        std::fs::write(dir.join("en.json"), r#"{"welcome":"From disk"}"#).unwrap();
        std::fs::write(dir.join("zh-CN.json"), r#"{"welcome":"来自磁盘"}"#).unwrap();
        std::fs::write(dir.join("termbase.json"), r#"{"terms":{}}"#).unwrap();
        let (i18n, source) = I18n::load_or_embedded(&dir).expect("disk override");
        assert_eq!(source, LocaleSource::Disk);
        assert_eq!(i18n.t("en", "welcome").unwrap(), "From disk");
        assert_eq!(i18n.t("zh-CN", "welcome").unwrap(), "来自磁盘");
    }
}
