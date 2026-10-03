//! The Navaja desktop app: Tauri shell around the tool registry.
//!
//! See `docs/architecture.md` §5.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod commands;
mod window;

/// Builds and runs the app until the last window closes or the user quits.
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::shell_ready
        ])
        .setup(|app| {
            window::create_main(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
}
