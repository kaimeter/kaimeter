// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Developer tool: regenerate the wizard's generated dictionary block in
//! `web/wizard.html` from the committed `locales/*.json`. Run it after any
//! locale edit; `cargo test` (wizard freshness test) fails otherwise.
//!
//! ```text
//! cargo run --bin regen-wizard
//! ```

fn main() -> anyhow::Result<()> {
    let i18n = kaimeter_core::i18n::I18n::embedded()?;
    let rendered = kaimeter_core::wizard::render_embedded(&i18n);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/wizard.html");
    std::fs::write(&path, rendered)?;
    println!("regenerated {}", path.display());
    Ok(())
}
