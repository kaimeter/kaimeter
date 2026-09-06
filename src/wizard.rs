// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! The wizard web asset and its locale injection.
//!
//! `web/wizard.html` is a single zero-dependency file that must also run
//! standalone from `file://`, so its translation dictionaries physically live
//! inside it — but they are GENERATED from `locales/*.json` (the `ui.*` keys),
//! never authored by hand. The generated block sits between the markers below
//! and is refreshed by `cargo run --bin regen-wizard`; a freshness test here
//! fails the build when the committed block drifts from the locale files.
//!
//! When the binary serves the page, [`render`] re-injects the *loaded* locales
//! into the same region — so a `locales/` directory on disk re-localizes the
//! wizard UI without a rebuild, exactly as it does for backend messages.

use crate::i18n::I18n;

/// The wizard asset embedded at compile time — one artifact, two delivery
/// modes (served at `/`, or opened directly from `file://`).
pub const WIZARD_TEMPLATE: &str = include_str!("../web/wizard.html");

/// Region markers around the generated dictionary block (`const L = …`).
const LOCALES_START: &str = "/*kaimeter-locales-start*/";
const LOCALES_END: &str = "/*kaimeter-locales-end*/";

/// Render the wizard for the loaded i18n state: the template with its
/// dictionary region replaced by the state's `ui.*` dictionaries.
pub fn render(i18n: &I18n) -> String {
    let json = serde_json::to_string_pretty(&i18n.ui_dictionaries()).expect("serializable maps");
    inject(WIZARD_TEMPLATE, &json)
}

/// Splice `json` between the locale markers, replacing whatever the region
/// currently carries. Panics when the markers are missing or duplicated —
/// both mean the template itself is broken, which is a build-time bug.
fn inject(html: &str, json: &str) -> String {
    let (start, end) = region(html).expect("wizard template carries exactly one locale region");
    let mut out = String::with_capacity(html.len() + json.len());
    out.push_str(&html[..start + LOCALES_START.len()]);
    out.push_str(json);
    out.push_str(&html[end..]);
    out
}

/// Byte offsets of the dictionary region: the start-marker's index and the
/// end-marker's index. `None` unless both markers occur exactly once and in
/// order.
fn region(html: &str) -> Option<(usize, usize)> {
    if html.matches(LOCALES_START).count() != 1 || html.matches(LOCALES_END).count() != 1 {
        return None;
    }
    let start = html.find(LOCALES_START)?;
    let end = html.find(LOCALES_END)?;
    (start < end).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_carries_exactly_one_locale_region() {
        assert!(region(WIZARD_TEMPLATE).is_some());
    }

    #[test]
    fn committed_wizard_matches_the_locale_files() {
        // The freshness guard: the generated block in web/wizard.html must be
        // exactly what the committed locale files produce. Drift means
        // someone edited one side without regenerating.
        let i18n = I18n::embedded().expect("embedded locales");
        assert_eq!(
            render(&i18n),
            WIZARD_TEMPLATE,
            "web/wizard.html is stale — edit locales/*.json, then run \
             `cargo run --bin regen-wizard`"
        );
    }

    #[test]
    fn disk_locales_replace_the_generated_dictionaries() {
        let dir = std::env::temp_dir().join("kaimeter-wizard-inject-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("en.json"), r#"{"ui.dash":"Cockpit"}"#).expect("en");
        std::fs::write(dir.join("zh-CN.json"), r#"{"ui.dash":"驾驶舱"}"#).expect("zh");
        std::fs::write(dir.join("termbase.json"), r#"{"terms":{}}"#).expect("termbase");
        let i18n = I18n::load(&dir).expect("load");
        let served = render(&i18n);
        assert!(served.contains("Cockpit"), "disk ui override is served");
        assert!(served.contains("驾驶舱"));
        assert_ne!(served, WIZARD_TEMPLATE);
    }

    #[test]
    fn injection_is_idempotent_and_marker_preserving() {
        let html = format!("a {LOCALES_START} old {LOCALES_END} b");
        let out = inject(&html, r#"{"en":{}}"#);
        assert_eq!(
            out,
            format!("a {LOCALES_START}{{\"en\":{{}}}}{LOCALES_END} b")
        );
        // Re-injecting over an injected region replaces, not nests.
        let again = inject(&out, r#"{"en":{"x":"y"}}"#);
        assert_eq!(
            again,
            format!("a {LOCALES_START}{{\"en\":{{\"x\":\"y\"}}}}{LOCALES_END} b")
        );
    }
}
