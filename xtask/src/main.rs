//! Repository tasks, run as `cargo xtask <command>` (alias in .cargo/config.toml).
//!
//! - `check`: crate edges, dependency closures, Tauri version parity and the
//!   security-relevant parts of the Tauri config (docs/roadmap.md §2).
//! - `bindings [--check]`: regenerate the ts-rs bindings, or fail on drift.
//! - `tool-gate <base>`: a PR that adds a tool touches only what
//!   docs/architecture.md §4 allows.
//! - `licenses`: every installed npm package, dev dependencies included, is
//!   under a licence deny.toml allows, or has an entry in js-licenses.toml.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool reports to the terminal"
)]
#![allow(
    clippy::disallowed_methods,
    reason = "xtask runs cargo, git and pnpm as child processes"
)]

use std::process::ExitCode;

mod bindings;
mod check;
mod licenses;
mod spdx;
mod tool_gate;
mod util;

const USAGE: &str =
    "usage: cargo xtask <check | bindings [--check] | tool-gate <base-ref> | licenses>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("check") => check::run(),
        Some("bindings") => bindings::run(args.iter().any(|a| a == "--check")),
        Some("tool-gate") => match args.get(1) {
            Some(base) => tool_gate::run(base),
            None => Err(anyhow::anyhow!(USAGE)),
        },
        Some("licenses") => licenses::run(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
