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
use tauri::{AppHandle, Manager, Runtime};

mod args;
mod clipboard;
mod commands;
mod crash;
mod guard;
mod logging;
mod paths;
mod platform;
mod settings;
mod state;
mod tray;
mod window;

/// Builds and runs the app until the last window closes or the user quits.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = args::parse(std::env::args());
    // A GUI run as root breaks on Wayland and leaves root-owned files in the
    // user's config. Navaja never needs it.
    #[cfg(target_os = "linux")]
    if platform::is_elevated() && !args.allow_root {
        return Err("Navaja does not need root and will not run as root.                     Start it as your own user, or pass --allow-root."
            .into());
    }

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

    let initial_tool = args.tool.filter(|id| state.registry.meta(id).is_some());
    let tray_tools: Vec<(String, String)> = state
        .registry
        .metas()
        .filter(|meta| meta.tray)
        .map(|meta| (meta.id.to_string(), meta.name.clone()))
        .collect();

    let builder = tauri::Builder::default()
        // First, so a second launch hands its arguments over and exits.
        .plugin(tauri_plugin_single_instance::init({
            let state = Arc::clone(&state);
            move |app, argv, _cwd| on_second_launch(app, &state, argv)
        }))
        .plugin(guard::init());
    // End-to-end test builds only (`--features e2e`, docs/roadmap.md §2).
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
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
            // Only the primary instance gets here; a second launch has
            // already handed over its arguments and exited.
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                tools = app.state::<Arc<state::AppState>>().registry.len(),
                "starting"
            );
            let window = window::create_main(app.handle(), initial_tool.as_deref())?;
            let theme = app.state::<Arc<state::AppState>>().settings.get().theme;
            commands::apply_theme(&window, theme);
            // Some Linux desktops have no tray host; the app works without it.
            if let Err(error) = tray::create(app.handle(), &tray_tools) {
                tracing::warn!(%error, "no tray icon");
            }
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}

/// A second `navaja` launch: show (or toggle) the running window and open
/// the requested tool. Arguments never run a tool action.
fn on_second_launch<R: Runtime>(app: &AppHandle<R>, state: &state::AppState, argv: Vec<String>) {
    let args = args::parse(argv);
    let Some(main) = window::main_window(app) else {
        return;
    };
    let shown = if args.toggle {
        window::toggle(&main)
    } else {
        window::reveal(&main)
    };
    let opened = match args.tool.filter(|id| state.registry.meta(id).is_some()) {
        Some(id) => window::open_tool(&main, &id),
        None => Ok(()),
    };
    if let Err(error) = shown.and(opened) {
        tracing::warn!(%error, "could not handle a second launch");
    }
}
