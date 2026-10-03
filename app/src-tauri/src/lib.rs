//! The Navaja desktop app: Tauri shell around the tool registry.
//!
//! See `docs/architecture.md` §5.
// Production code never unwraps or panics; tests may.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::sync::Arc;

use navaja_core::Registry;
use tauri::Manager;

mod clipboard;
mod commands;
mod crash;
mod logging;
mod paths;
mod settings;
mod state;
mod window;

/// What `tests/privacy.rs` drives: the real panic hook, logger and run path.
/// Not an API.
#[doc(hidden)]
pub mod testing {
    pub use crate::commands::{RunEnvelope, execute};
    pub use crate::crash::install as install_crash_hook;
    pub use crate::logging::{LogGuard, init_with_directives as init_logging};
    pub use crate::settings::SettingsStore;
    pub use crate::state::AppState;
}

/// Builds and runs the app until the last window closes or the user quits,
/// then releases what it holds and returns the exit code for `main`.
pub fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let app_dir = paths::app_dir();
    if let Some(dir) = &app_dir {
        let _ = paths::ensure_private_dir(dir);
    }
    // First: no panic may reach the default hook, which prints the payload.
    crash::install(app_dir.clone());
    let logs = logging::init(app_dir.as_deref());

    // Tool metadata is validated by registry_test in CI; a failure here is a
    // build defect, reported once and fatal.
    let registry = Registry::new(navaja_tools::all()).map_err(|problems| {
        for problem in &problems {
            tracing::error!(%problem, "invalid tool");
        }
        std::io::Error::other("invalid tool registry")
    })?;
    let settings = settings::SettingsStore::load(app_dir.as_deref());
    let state = Arc::new(state::AppState::new(registry, settings));
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        tools = state.registry.len(),
        "starting"
    );

    // Tauri errors carry no user input, so they are logged in full. A setup
    // error becomes Tauri's panic, whose message the panic hook withholds.
    let app = tauri::Builder::default()
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::shell_ready,
            commands::list_tools,
            commands::search,
            commands::run_tool,
            commands::cancel_run,
            commands::copy_text,
            commands::settings_get,
            commands::settings_set,
        ])
        .setup(move |app| {
            let window = window::create_main(app.handle())
                .inspect_err(|error| tracing::error!(%error, "creating the main window failed"))?;
            // A new window already follows the OS theme.
            let theme = app.state::<Arc<state::AppState>>().settings.get().theme;
            if theme != settings::Theme::System {
                commands::apply_theme(&window, theme);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .inspect_err(|error| tracing::error!(%error, "starting the app failed"))?;

    // Unlike `run`, which exits the process from inside the event loop,
    // `run_return` comes back, so the clean-up below happens.
    let code = app.run_return(|_, _| {});
    // On X11 the clipboard handle serves copied text until it drops.
    state.clipboard.release();
    drop(state);
    // Flushes lines still queued for the log file.
    drop(logs);
    Ok(code)
}
