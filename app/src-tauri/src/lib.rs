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

/// Builds and runs the app until the last window closes or the user quits.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app_dir = paths::app_dir();
    if let Some(dir) = &app_dir {
        let _ = paths::ensure_private_dir(dir);
    }
    // First: no panic may reach the default hook, which prints the payload.
    crash::install(app_dir.clone());
    let _logs = logging::init(app_dir.as_deref());

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

    tauri::Builder::default()
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
            let window = window::create_main(app.handle())?;
            let theme = app.state::<Arc<state::AppState>>().settings.get().theme;
            commands::apply_theme(&window, theme);
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
