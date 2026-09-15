//! Repository automation used by the `xtask` binary.

mod comments;
mod scan;

use std::process::ExitCode;

/// Runs the subcommand named by the process arguments.
pub fn run() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("check-comments") => scan::check_comments(),
        Some(other) => unknown_subcommand(other),
        None => usage(),
    }
}

fn unknown_subcommand(name: &str) -> ExitCode {
    eprintln!("xtask: unknown subcommand `{name}`");
    usage()
}

fn usage() -> ExitCode {
    eprintln!("usage: cargo run -p xtask -- check-comments");
    ExitCode::FAILURE
}
