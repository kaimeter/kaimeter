// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Build the frontend and hand it to `include_str!`.
//!
//! `web/` is the source of truth for the wizard UI — React, Tailwind, shadcn,
//! bundled by Vite into ONE self-contained file that opens from `file://` with
//! no network. `src/wizard.rs` embeds that file with `include_str!`, so it must
//! exist before rustc compiles the crate. This script produces it.
//!
//! That is what makes `cargo build` and `cargo test` always embed a UI built
//! from `web/src/`: neither the artifact nor the generated dictionaries are
//! committed, so there is no copy left to go stale.
//!
//! Node is therefore a build prerequisite (see README). One escape exists for
//! environments that build the UI elsewhere:
//!
//! * `KAIMETER_SKIP_FRONTEND=1` — reuse a prebuilt `web/wizard.html` as-is. The
//!   container build does this: Node builds the UI in its own stage, so the
//!   Rust stage needs no Node toolchain at all.
//!
//! Rust-only edits do not pay for the frontend: cargo re-runs this script only
//! when one of the watched inputs below changes.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// The single-file artifact the crate embeds, written into `OUT_DIR`.
const ARTIFACT: &str = "wizard.html";

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let web = manifest.join("web");
    let artifact = out_dir.join(ARTIFACT);

    // Re-run when a frontend input changes. Cargo walks watched directories
    // recursively, so `src` and `scripts` cover every module. `web` itself is
    // deliberately NOT watched: `node_modules` lives there and changes on every
    // install, which would rebuild the UI on every cargo invocation.
    for input in [
        "src",
        "scripts",
        "index.html",
        "vite.config.mjs",
        "package.json",
        "package-lock.json",
    ] {
        watch(&web.join(input));
    }
    // The bundled fallback dictionaries are generated from the repo's locale
    // files, so they are an input too.
    watch(&manifest.join("locales"));
    println!("cargo:rerun-if-env-changed=KAIMETER_SKIP_FRONTEND");

    if truthy(env::var("KAIMETER_SKIP_FRONTEND").ok().as_deref()) {
        let prebuilt = web.join(ARTIFACT);
        assert!(
            prebuilt.is_file(),
            "KAIMETER_SKIP_FRONTEND=1 but {} does not exist — build the frontend \
             first (`npm --prefix web run build`), or unset it and let this \
             script do it.",
            prebuilt.display()
        );
        copy(&prebuilt, &artifact);
        return;
    }

    install_dependencies(&web);
    build(&web, &artifact);
}

/// Watch `path` when it exists. The container's Rust stage receives only the
/// prebuilt artifact, so the frontend sources are absent there and cargo would
/// otherwise warn about each missing watch path.
fn watch(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn truthy(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1" | "true" | "yes"))
}

/// npm ships as a shell shim on Unix and a `.cmd` on Windows.
fn npm() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

/// `npm ci` when the tree is missing or the lockfile is newer than the install.
/// `npm ci` wipes `node_modules`, so it is worth not paying for it every build.
fn install_dependencies(web: &Path) {
    let lock = web.join("package-lock.json");
    let stamp = web.join("node_modules").join(".package-lock.json");
    if lock.is_file() && stamp.is_file() && modified(&lock) <= modified(&stamp) {
        return;
    }
    let status = spawn(npm(), &["ci"], web);
    assert!(
        status.success(),
        "`npm ci` failed in {}: the frontend dependencies could not be installed",
        web.display()
    );
}

fn build(web: &Path, artifact: &Path) {
    // Vite writes the bundle straight into OUT_DIR, so the build never creates
    // an output file inside the source tree.
    let status = match Command::new(npm())
        .args(["run", "build"])
        .current_dir(web)
        .env("KAIMETER_WIZARD_OUT", artifact)
        .status()
    {
        Ok(status) => status,
        Err(err) => missing_node(err),
    };
    assert!(
        status.success(),
        "`npm run build` failed: the wizard was not built"
    );
    assert!(
        artifact.is_file(),
        "the frontend build produced no {} — is KAIMETER_WIZARD_OUT honoured?",
        artifact.display()
    );
}

fn spawn(program: &str, args: &[&str], dir: &Path) -> std::process::ExitStatus {
    match Command::new(program).args(args).current_dir(dir).status() {
        Ok(status) => status,
        Err(err) => missing_node(err),
    }
}

fn missing_node(err: std::io::Error) -> ! {
    panic!(
        "could not run npm ({err}).\n\
         Node.js is required to build the wizard in web/. Install Node 22+ and \
         re-run (see README). To build the UI elsewhere and reuse it instead, \
         set KAIMETER_SKIP_FRONTEND=1 with a prebuilt web/wizard.html."
    )
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn copy(from: &Path, to: &Path) {
    fs::copy(from, to)
        .unwrap_or_else(|err| panic!("copy {} -> {}: {err}", from.display(), to.display()));
}
