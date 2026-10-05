//! The Navaja desktop app: Tauri shell around the tool registry.
//!
//! See `docs/architecture.md` §5.
// Production code never unwraps or panics; tests may.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::fmt;
use std::sync::Arc;

use navaja_core::Registry;
use tauri::{AppHandle, Runtime};

mod args;
mod clipboard;
mod commands;
mod crash;
mod guard;
mod logging;
mod opener;
mod paths;
mod platform;
mod settings;
mod state;
#[cfg(test)]
mod test_support;
mod tray;
mod window;

/// What the integration tests drive: the real panic hook, logger and run
/// path (`tests/privacy.rs`), and the clipboard behind `copy_text`, as
/// `AppState::clipboard` (`tests/clipboard`). Not an API.
#[doc(hidden)]
pub mod testing {
    pub use crate::commands::{RunEnvelope, execute};
    pub use crate::crash::install as install_crash_hook;
    pub use crate::logging::{LogGuard, init_with_directives as init_logging};
    pub use crate::settings::SettingsStore;
    pub use crate::state::AppState;
}

/// Why the app did not start. std prints an error returned from `main` with
/// `Debug`, so `Debug` prints the plain message rather than a quoted string.
pub struct StartError(String);

impl<E: std::error::Error> From<E> for StartError {
    fn from(error: E) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Builds and runs the app until the last window closes or the user quits,
/// then releases what it holds and returns the exit code for `main`.
pub fn run() -> Result<i32, StartError> {
    let app_dir = paths::app_dir();
    // First: no panic may reach the default hook, which prints the payload.
    crash::install(app_dir.clone());

    let args = args::parse(std::env::args_os());
    // Root on any Unix. An elevated Windows start only gets the banner.
    let is_root = cfg!(unix) && platform::is_elevated();
    if let Some(refusal) = args::root_refusal(&args, is_root) {
        return Err(StartError(refusal.to_owned()));
    }

    if let Some(dir) = &app_dir {
        let _ = paths::ensure_private_dir(dir);
    }
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
    let state = Arc::new(state::AppState::new(registry, settings, app_dir));

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
    let builder = {
        let builder = builder.plugin(tauri_plugin_wdio::init());
        // The embedded WebDriver server listens on localhost with no
        // authentication, so it starts only when the test harness launched
        // the app, never when someone runs a leftover e2e build by hand.
        let webdriver = e2e_harness_launched();
        tracing::info!(webdriver, "end-to-end test build");
        if webdriver {
            builder.plugin(tauri_plugin_wdio_webdriver::init())
        } else {
            builder
        }
    };

    // Tauri errors carry no user input, so they are logged in full. A setup
    // error becomes Tauri's panic, whose message the panic hook withholds.
    let app = builder
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
            commands::open_logs,
            commands::open_url,
            commands::quit,
        ])
        .setup({
            let state = Arc::clone(&state);
            move |app| {
                // Only the primary instance gets here; a second launch has
                // already handed over its arguments and exited.
                tracing::info!(
                    version = env!("CARGO_PKG_VERSION"),
                    tools = state.registry.len(),
                    "starting"
                );
                let window = window::create_main(app.handle(), &state, initial_tool.as_deref())
                    .inspect_err(
                        |error| tracing::error!(%error, "creating the main window failed"),
                    )?;
                // A new window already follows the OS theme.
                let theme = state.settings.get().theme;
                if theme != settings::Theme::System {
                    commands::apply_theme(&window, theme);
                }
                // Some Linux desktops have no tray host or no appindicator
                // library; the app works without a tray.
                if !platform::tray_available() {
                    tracing::warn!("no tray icon: no appindicator library");
                } else if let Err(error) = tray::create(app.handle(), &state, &tray_tools) {
                    tracing::warn!(%error, "no tray icon");
                }
                Ok(())
            }
        })
        .build(tauri::generate_context!())
        .inspect_err(|error| tracing::error!(%error, "starting the app failed"))?;

    // Some quits end the process inside the event loop even with
    // `run_return` (macOS's Quit menu item, a Windows logoff), but every quit
    // emits `Exit` first, so the clean-up happens there.
    let mut logs = Some(logs);
    let code = app.run_return(move |_, event| {
        if let tauri::RunEvent::Exit = event {
            // On X11 the clipboard handle serves copied text until it drops.
            state.clipboard.release();
            // Flushes lines still queued for the log file.
            drop(logs.take());
        }
    });
    Ok(code)
}

/// What asked Navaja to quit, for the one log line a quit writes. Fixed
/// values, so nothing from the webview reaches the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuitFrom {
    Tray,
    Command,
}

impl QuitFrom {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tray => "tray",
            Self::Command => "command",
        }
    }
}

/// Quits the app. The tray's Quit and the `quit` command both come here, so
/// both end the event loop the same way: `exit` emits `RunEvent::Exit`,
/// where `run` releases the clipboard and flushes the log.
fn quit<R: Runtime>(app: &AppHandle<R>, from: QuitFrom) {
    tracing::info!(from = from.as_str(), "quit");
    app.exit(0);
}

/// Whether the end-to-end harness started this process: `@wdio/tauri-service`
/// sets `WDIO_EMBEDDED_SERVER` when it launches the app, and a harness that
/// launches it another way sets `NAVAJA_E2E_WEBDRIVER`.
#[cfg(feature = "e2e")]
fn e2e_harness_launched() -> bool {
    ["WDIO_EMBEDDED_SERVER", "NAVAJA_E2E_WEBDRIVER"]
        .into_iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// A second `navaja` launch: show (or toggle) the running window and open
/// the requested tool. Arguments never run a tool action. Before the front
/// end is ready, the request waits for it (see `window::ready`).
fn on_second_launch<R: Runtime>(app: &AppHandle<R>, state: &state::AppState, argv: Vec<String>) {
    let args = args::parse(argv);
    let request = window::Request {
        tool: args.tool.filter(|id| state.registry.meta(id).is_some()),
        palette: false,
    };
    let result = if args.toggle {
        window::toggle(app, &state.window, &request)
    } else {
        window::open(app, &state.window, &request)
    };
    if let Err(error) = result {
        tracing::warn!(%error, "could not handle a second launch");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_errors_print_as_plain_text() {
        let error = StartError::from(std::io::Error::other("no \"quotes\" added"));
        assert_eq!(format!("{error:?}"), r#"no "quotes" added"#);
        assert_eq!(error.to_string(), r#"no "quotes" added"#);
    }
}
