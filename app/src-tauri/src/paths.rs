//! Where Navaja keeps its own files: settings, logs and crash reports.
//!
//! The directory name is frozen in docs/adr/0001-stack.md and deliberately
//! not derived from the app identifier, so an identifier change never moves
//! user data.

use std::path::{Path, PathBuf};

/// Overrides the directory in debug builds only (tests, end-to-end runs).
#[cfg(any(debug_assertions, test))]
const OVERRIDE: &str = "NAVAJA_APP_DIR";

pub fn app_dir() -> Option<PathBuf> {
    #[cfg(any(debug_assertions, test))]
    if let Some(dir) = std::env::var_os(OVERRIDE).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let name = if cfg!(target_os = "linux") {
        "navaja"
    } else {
        "Navaja"
    };
    dirs::config_dir().map(|base| base.join(name))
}

/// Where the log files go: the logger writes there and `open_logs` shows it.
pub fn logs_dir(app_dir: &Path) -> PathBuf {
    app_dir.join("logs")
}

/// Creates `dir` (and parents) readable only by the user on Unix.
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
