//! IPC commands. Each one is declared in build.rs and granted one by one in
//! capabilities/main.json. Inputs from the webview are validated here.

use std::sync::Arc;
use std::time::Instant;

use navaja_core::{CategoryInfo, Progress, RunEnv, SearchHit, ToolError, ToolMeta, Value};
use serde::Serialize;
use tauri::ipc::{Channel, Response};
use tauri::{Runtime, State, WebviewWindow};

use crate::settings::{Settings, Theme};
use crate::state::AppState;

/// Basic facts about this build, shown in About.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub spec_version: u16,
    /// Running as administrator or root, which Navaja never needs.
    pub elevated: bool,
}

pub fn app_info_value() -> AppInfo {
    AppInfo {
        name: "Navaja".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        spec_version: navaja_core::SPEC_VERSION,
        elevated: crate::platform::is_elevated(),
    }
}

/// Everything the shell needs to build navigation and search.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Catalog {
    pub tools: Vec<ToolMeta>,
    pub categories: Vec<CategoryInfo>,
}

/// The body of a `run_tool` response, sent as raw JSON bytes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum RunEnvelope {
    Ok(Value),
    Err(ToolError),
}

const MAX_QUERY_CHARS: usize = 200;
const MAX_RUN_ID_LEN: usize = 64;

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

#[tauri::command]
pub fn list_tools(state: State<'_, Arc<AppState>>) -> Catalog {
    Catalog {
        tools: state.registry.metas().cloned().collect(),
        categories: state.registry.categories(),
    }
}

#[tauri::command]
pub fn search(state: State<'_, Arc<AppState>>, query: String) -> Vec<SearchHit> {
    let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
    state.registry.search(&query)
}

fn valid_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id.len() <= MAX_RUN_ID_LEN
        && run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Runs a tool action off the main thread. Progress goes over `progress`;
/// the result is a `RunEnvelope` as raw JSON, so large outputs aren't
/// re-encoded by the IPC layer.
#[tauri::command]
pub async fn run_tool(
    state: State<'_, Arc<AppState>>,
    tool: String,
    action: String,
    run_id: String,
    input: Value,
    progress: Channel<Progress>,
) -> Result<Response, String> {
    if !valid_run_id(&run_id) {
        return Err("invalid run id".to_owned());
    }
    let cancel = state
        .begin_run(&run_id)
        .ok_or_else(|| "duplicate run id".to_owned())?;

    let app = Arc::clone(state.inner());
    let (tool_id, action_id) = (tool.clone(), action.clone());
    let started = Instant::now();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        let report = |p: Progress| {
            let _ = progress.send(p);
        };
        let env = RunEnv {
            cancel: &cancel,
            progress: &report,
            services: &app.services,
        };
        app.registry.run(&tool, &action, input, &env)
    })
    .await;
    state.end_run(&run_id);

    let envelope = match joined {
        Ok(Ok(value)) => RunEnvelope::Ok(value),
        Ok(Err(error)) => RunEnvelope::Err(error),
        // The registry already contains tool panics; this is the worker itself.
        Err(_) => RunEnvelope::Err(ToolError::panicked()),
    };
    let code = match &envelope {
        RunEnvelope::Ok(_) => "ok",
        RunEnvelope::Err(error) => error.code.as_str(),
    };
    // Never log inputs or outputs.
    tracing::info!(
        tool = %tool_id,
        action = %action_id,
        ms = started.elapsed().as_millis() as u64,
        code,
        "run"
    );
    serde_json::to_vec(&envelope)
        .map(Response::new)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cancel_run(state: State<'_, Arc<AppState>>, run_id: String) -> bool {
    state.cancel_run(&run_id)
}

/// Copies through Rust; `sensitive` keeps it out of clipboard history and sync.
#[tauri::command]
pub fn copy_text(
    state: State<'_, Arc<AppState>>,
    text: String,
    sensitive: bool,
) -> Result<(), String> {
    state.clipboard.copy(text, sensitive)
}

#[tauri::command]
pub fn settings_get(state: State<'_, Arc<AppState>>) -> Settings {
    state.settings.get()
}

#[tauri::command]
pub fn settings_set<R: Runtime>(
    state: State<'_, Arc<AppState>>,
    window: WebviewWindow<R>,
    settings: Settings,
) -> Result<(), String> {
    let theme = settings.theme;
    state.settings.set(settings)?;
    apply_theme(&window, theme);
    Ok(())
}

pub fn apply_theme<R: Runtime>(window: &WebviewWindow<R>, theme: Theme) {
    let native = match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    let _ = window.set_theme(native);
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

    #[test]
    fn run_ids() {
        assert!(valid_run_id("3f2b1c9e-0a4d-4f1e-9a7b-2c8d5e6f7a8b"));
        for bad in ["", "a b", "x/../y", &"a".repeat(65), "id\n"] {
            assert!(!valid_run_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn envelope_wire_format() {
        let ok =
            serde_json::to_value(RunEnvelope::Ok(serde_json::json!({ "uuids": "x" }))).unwrap();
        assert_eq!(ok, serde_json::json!({ "ok": { "uuids": "x" } }));
        let err = serde_json::to_value(RunEnvelope::Err(ToolError::cancelled())).unwrap();
        assert_eq!(err["err"]["code"], "core.cancelled");
    }
}
