//! Thin binary entry point for the `xtask` library.

use std::process::ExitCode;

fn main() -> ExitCode {
    xtask::run()
}
