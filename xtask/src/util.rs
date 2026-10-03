use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The repository root (the parent of `xtask/`).
///
/// `cargo run` sets CARGO_MANIFEST_DIR at run time; prefer it over the value
/// baked in at compile time, which goes stale if the binary is reused from
/// another checkout sharing the target directory.
pub fn root() -> PathBuf {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from);
    manifest_dir
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// The cargo that launched us, so `cargo xtask` uses the pinned toolchain.
pub fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

/// Runs git in the repository root and returns its stdout.
pub fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root())
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git output is not UTF-8")
}

/// Prints problems and fails if there are any.
pub fn report(what: &str, problems: &[String]) -> Result<()> {
    if problems.is_empty() {
        println!("{what}: ok");
        return Ok(());
    }
    for problem in problems {
        eprintln!("  - {problem}");
    }
    bail!("{what}: {} problem(s)", problems.len())
}
