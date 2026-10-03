//! Main window lifecycle. The window is declared in tauri.conf.json with
//! `"create": false` and built here, so every hardening option lives in one
//! place (docs/architecture.md §5).

use std::time::Duration;

use tauri::{AppHandle, Runtime, WebviewWindow, WebviewWindowBuilder};

pub const MAIN: &str = "main";

/// How long to wait for the front end's `shell_ready` before showing anyway.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

pub fn create_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;

    let window = WebviewWindowBuilder::from_config(app, &config)?.build()?;

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

pub fn reveal<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    window.show()?;
    window.unminimize()?;
    window.set_focus()
}
