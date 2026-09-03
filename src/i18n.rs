// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! i18n layer: JSON locales loaded from a directory, plus a **termbase** —
//! locked compliance terminology that translations must use verbatim.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

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
        let locales = load_locale(dir, "en")?;
        let zh = load_locale(dir, "zh-CN")?;
        let termbase = load_termbase(dir)?;
        let i18n = Self {
            locales: BTreeMap::from([(locales.code.clone(), locales), (zh.code.clone(), zh)]),
            termbase,
        };
        // Validate: every termbase locale must exist among loaded locales.
        let loaded: Vec<Locale> = i18n.locales.values().cloned().collect();
        i18n.termbase.validate(&loaded)?;
        // Validate: every locale must carry the same message keys as `en`.
        // `en` is canonical — a missing key in any other locale is a hard
        // error, so a half-translated locale can never ship.
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
}
