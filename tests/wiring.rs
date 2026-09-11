// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Every declared module must be reachable from non-test code.
//!
//! A green unit test proves a function behaves; it does not prove anything
//! calls it. `src/vault.rs` shipped a correct Argon2id/AES-256-GCM
//! implementation with four passing tests and zero callers, so encryption at
//! rest was inert while the suite stayed green (see ROADMAP.md, 0.1.4 audit).
//!
//! This test closes that gap. A module declared in `lib.rs` or `main.rs` that
//! nothing in `src/` references by path is either dead or — worse — a feature
//! that looks implemented and is not. A module referenced only from `tests/`
//! is reported separately: it is exercised by test code, but no product path
//! reaches it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const ROOTS: [&str; 2] = ["src/lib.rs", "src/main.rs"];

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Source with doc comments and line comments removed, so a module mentioned
/// only in prose (e.g. `[`crate::vault`]` in a doc comment) is not mistaken
/// for a call site. This is a line-level approximation: it drops whole lines
/// that are comments and strips trailing `// ...` from code lines.
fn code_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_block = false;
    for line in text.lines() {
        let mut line = line.to_string();
        if in_block {
            match line.find("*/") {
                Some(i) => {
                    line = line[i + 2..].to_string();
                    in_block = false;
                }
                None => continue,
            }
        }
        if let Some(i) = line.find("/*") {
            if line[i..].contains("*/") {
                let after = line[i..]
                    .find("*/")
                    .map(|j| i + j + 2)
                    .unwrap_or(line.len());
                line = format!("{}{}", &line[..i], &line[after..]);
            } else {
                line.truncate(i);
                in_block = true;
            }
        }
        // Drop `///`, `//!`, and `//` lines entirely; strip trailing comments.
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        if let Some(i) = line.find("//") {
            line.truncate(i);
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn declared_modules(root: &Path) -> BTreeSet<String> {
    let mut mods = BTreeSet::new();
    for rel in ROOTS {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            let Some(rest) = line
                .strip_prefix("pub mod ")
                .or_else(|| line.strip_prefix("mod "))
            else {
                continue;
            };
            if let Some(name) = rest.strip_suffix(';') {
                mods.insert(name.trim().to_string());
            }
        }
    }
    mods
}

/// True when `path` *is* part of the named module, rather than a consumer.
fn is_own_file(path: &Path, module: &str) -> bool {
    let stem_is_module = path.file_stem().is_some_and(|s| s == module);
    let in_module_dir = path
        .parent()
        .and_then(|d| d.file_name())
        .is_some_and(|d| d == module);
    stem_is_module || in_module_dir
}

/// True when `code` mentions the module as a path segment: `crate::vault::`,
/// `kaimeter_core::vault::`, or a bare `vault::` inside the crate. A module
/// *declaration* (`mod vault;`) is not followed by `::`, so it does not match,
/// and `std::sync::Arc` does not count as a use of a crate module `sync`.
fn references(code: &str, module: &str) -> bool {
    let needle = format!("{module}::");
    let mut from = 0;
    while let Some(i) = code[from..].find(&needle) {
        let at = from + i;
        from = at + 1;

        // The character immediately before the segment must not continue an
        // identifier, so `myhttp::` is not a use of `http`. Only the directly
        // adjacent character matters: whitespace separates path segments from
        // preceding syntax (`if dossier::x`).
        let continues_identifier = match code[..at].chars().next_back() {
            Some(c) => c.is_alphanumeric() || c == '_',
            None => false,
        };
        if continues_identifier {
            continue;
        }
        // `std::sync::Arc` is the standard library, never the crate module.
        if let Some(before_std) = code[..at].trim_end_matches(':').strip_suffix("std") {
            let boundary = match before_std.chars().next_back() {
                Some(c) => !(c.is_alphanumeric() || c == '_'),
                None => true,
            };
            if boundary {
                continue;
            }
        }
        return true;
    }
    false
}

/// Modules that are declared but not reachable from a product path today.
///
/// This is a ratchet, not an amnesty. The test fails if the set of unwired
/// modules *grows*, and it also fails if a module listed here becomes wired
/// and the entry is not removed — so fixing one is a one-line deletion, and
/// nothing can quietly join the list.
///
/// - `vault` — R22 encryption at rest. Real Argon2id/AES-256-GCM with tests,
///   no callers: `payload_sealed` is never written. See ROADMAP.md, 0.1.5/0.2.0.
/// - `liability`, `sync`, `verifier` — referenced only from `tests/`.
const KNOWN_UNWIRED: [&str; 4] = ["vault", "liability", "sync", "verifier"];

#[test]
fn every_declared_module_is_reachable_from_src() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let modules = declared_modules(root);
    assert!(
        modules.len() > 10,
        "expected the crate's modules to be discovered, found {}",
        modules.len()
    );

    let src = rust_files(&root.join("src"));
    let src_code: Vec<(&PathBuf, String)> = src
        .iter()
        .map(|p| (p, code_only(&fs::read_to_string(p).unwrap_or_default())))
        .collect();

    let mut unreachable = Vec::new();

    for module in &modules {
        let used_in_src = src_code
            .iter()
            .any(|(path, code)| !is_own_file(path, module) && references(code, module));
        if used_in_src {
            continue;
        }
        let used_in_tests = rust_files(&root.join("tests")).iter().any(|p| {
            code_only(&fs::read_to_string(p).unwrap_or_default()).contains(&format!("::{module}"))
        });
        if !used_in_tests || !KNOWN_UNWIRED.contains(&module.as_str()) {
            unreachable.push(module.clone());
        }
    }

    let known: BTreeSet<String> = KNOWN_UNWIRED.iter().map(|s| (*s).to_string()).collect();
    let actual: BTreeSet<String> = modules.iter().cloned().collect();
    let stale: Vec<&String> = known.iter().filter(|m| !actual.contains(*m)).collect();
    assert!(
        stale.is_empty(),
        "KNOWN_UNWIRED names modules that no longer exist: {stale:?}"
    );

    // Anything not allow-listed must be wired.
    unreachable.retain(|m| !known.contains(m));
    assert!(
        unreachable.is_empty(),
        "declared but unreachable from any product path, and not in \
         KNOWN_UNWIRED: {unreachable:?}. Wire the module, or delete it. Do not \
         add to KNOWN_UNWIRED without a roadmap entry."
    );
}

/// The allow-list must not rot: a module that becomes wired has to leave it.
#[test]
fn known_unwired_list_has_no_fixed_entries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = rust_files(&root.join("src"));
    let src_code: Vec<(PathBuf, String)> = src
        .iter()
        .map(|p| {
            (
                p.clone(),
                code_only(&fs::read_to_string(p).unwrap_or_default()),
            )
        })
        .collect();

    let fixed: Vec<&str> = KNOWN_UNWIRED
        .iter()
        .copied()
        .filter(|module| {
            src_code
                .iter()
                .any(|(path, code)| !is_own_file(path, module) && references(code, module))
        })
        .collect();

    assert!(
        fixed.is_empty(),
        "{fixed:?} are now wired from src/ but still listed in KNOWN_UNWIRED. \
         Remove them from the list."
    );
}
