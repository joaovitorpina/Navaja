//! Main window lifecycle. The window is declared in tauri.conf.json with
//! `"create": false` and built here, so every hardening option lives in one
//! place (docs/architecture.md §5).

use std::time::Duration;

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow, WebviewWindowBuilder};

pub const MAIN: &str = "main";

/// How long to wait for the front end's `shell_ready` before showing anyway.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Builds the main window. `initial_tool` (already checked against the
/// registry) becomes the first route.
pub fn create_main<R: Runtime>(
    app: &AppHandle<R>,
    initial_tool: Option<&str>,
) -> tauri::Result<WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;

    let mut builder = WebviewWindowBuilder::from_config(app, &config)?
        // No pop-ups: links and window.open never create windows.
        .on_new_window(|_url, _features| NewWindowResponse::Deny);
    if let Some(id) = initial_tool {
        builder = builder.initialization_script(route_script(id));
    }
    let window = builder.build()?;

    // Fallback: if the front end never reports ready, show the window anyway
    // rather than leaving an invisible process behind.
    let handle = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_TIMEOUT);
        if !handle.is_visible().unwrap_or(true) {
            tracing::warn!("front end did not report ready; showing the window anyway");
            let _ = reveal(&handle);
        }
    });
    Ok(window)
}

/// Sets the first route before the page's own scripts run. `id` is a
/// registered tool id (`[a-z][a-z0-9_]*`), so it is safe inside the string.
fn route_script(id: &str) -> String {
    format!("if (!location.hash || location.hash === '#/') location.hash = '#/tool/{id}';")
}

pub fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window(MAIN)
}

pub fn reveal<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    window.show()?;
    window.unminimize()?;
    window.set_focus()
}

/// Hides the window if it is shown and focused; otherwise shows it.
pub fn toggle<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    let visible = window.is_visible().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    if visible && focused && !minimized {
        window.hide()
    } else {
        reveal(window)
    }
}

/// Navigates to a tool. `id` must be a registered tool id.
pub fn open_tool<R: Runtime>(window: &WebviewWindow<R>, id: &str) -> tauri::Result<()> {
    window.eval(format!("window.location.hash = '#/tool/{id}';"))
}

/// Opens the command palette.
pub fn open_palette<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    window.eval("window.dispatchEvent(new Event('navaja:palette'));")
}
