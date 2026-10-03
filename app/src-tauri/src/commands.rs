//! IPC commands. Each one is declared in build.rs and granted one by one in
//! capabilities/main.json.

use serde::Serialize;
use tauri::{Runtime, WebviewWindow};

/// Basic facts about this build, shown in About.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub spec_version: u16,
}

pub fn app_info_value() -> AppInfo {
    AppInfo {
        name: "Navaja",
        version: env!("CARGO_PKG_VERSION"),
        spec_version: navaja_core::SPEC_VERSION,
    }
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    app_info_value()
}

/// Called by the front end once it has rendered, so the window never shows
/// an empty frame. See `window::create_main` for the fallback.
#[tauri::command]
pub fn shell_ready<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    crate::window::reveal(&window).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_this_build() {
        let info = app_info_value();
        assert_eq!(info.name, "Navaja");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.spec_version, navaja_core::SPEC_VERSION);
    }
}
