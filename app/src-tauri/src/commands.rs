//! IPC commands. Each one is declared in build.rs and granted one by one in
//! capabilities/main.json.
//!
//! What the host checks before any work: run ids, the shape of tool and
//! action ids, and the overall size of a run's input (string bytes and JSON
//! nodes). The search query is truncated and copied text is size-capped.
//! `open_url` opens only the exact URLs Navaja links to. Each tool validates
//! its own options (docs/architecture.md §3).

use std::sync::Arc;
use std::time::{Duration, Instant};

use navaja_core::{
    ActionMeta, CategoryInfo, ErrorCode, Progress, Registry, RunEnv, SearchHit, ToolError, ToolId,
    ToolMeta, Value,
};
use serde::Serialize;
use tauri::ipc::{Channel, Response};
use tauri::{AppHandle, Runtime, State, WebviewWindow};

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
/// Host-wide input limits for one run, whatever the tool: string bytes
/// (object keys included) and JSON nodes.
const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_INPUT_NODES: usize = 1_000_000;
/// Logged in place of a tool or action id the registry doesn't know, so
/// text from the webview never reaches the log.
const UNKNOWN_ID: &str = "<unknown>";
/// `open_url`'s answer to a URL it does not open. Never echoes the URL.
const OPEN_URL_REFUSED: &str = "Navaja opens only its own links.";

#[tauri::command]
pub fn app_info() -> AppInfo {
    app_info_value()
}

/// Called by the front end once it has rendered, so the window never shows
/// an empty frame. Also opens what a second launch or the tray asked for
/// meanwhile. See `window::create_main` for the fallback.
#[tauri::command]
pub fn shell_ready<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    crate::window::ready(&window, &state.window).map_err(|error| error.to_string())
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

/// Whether `input` stays within `max_bytes` of string data (object keys
/// included) and `max_nodes` JSON values. Stops at the first excess.
fn input_within(input: &Value, max_bytes: usize, max_nodes: usize) -> bool {
    let (mut bytes, mut nodes) = (0usize, 1usize);
    let mut pending = vec![input];
    while let Some(value) = pending.pop() {
        match value {
            Value::String(text) => bytes = bytes.saturating_add(text.len()),
            Value::Array(items) => {
                nodes = nodes.saturating_add(items.len());
                if nodes > max_nodes {
                    return false;
                }
                pending.extend(items);
            }
            Value::Object(map) => {
                nodes = nodes.saturating_add(map.len());
                bytes = map
                    .keys()
                    .fold(bytes, |sum, key| sum.saturating_add(key.len()));
                if nodes > max_nodes {
                    return false;
                }
                pending.extend(map.values());
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
        if bytes > max_bytes {
            return false;
        }
    }
    true
}

/// Refuses a run before it starts. Errors never echo what was sent.
fn precheck(tool: &str, action: &str, input: &Value) -> Result<(), ToolError> {
    if !ToolId::is_valid(tool) {
        return Err(ToolError::new(ErrorCode::UNKNOWN_TOOL, "No such tool."));
    }
    if !ActionMeta::is_valid_id(action) {
        return Err(ToolError::new(
            ErrorCode::UNKNOWN_ACTION,
            "This tool has no such action.",
        ));
    }
    if !input_within(input, MAX_INPUT_BYTES, MAX_INPUT_NODES) {
        return Err(ToolError::invalid_input("The input is too large."));
    }
    Ok(())
}

/// The one log line per run: ids the registry owns, duration and result
/// code. Never inputs or outputs.
fn log_run(
    registry: &Registry,
    tool: &str,
    action: &str,
    elapsed: Duration,
    envelope: &RunEnvelope,
) {
    let meta = registry.meta(tool);
    let tool = meta.map_or(UNKNOWN_ID, |meta| meta.id.as_str());
    let action = meta
        .and_then(|meta| meta.action(action))
        .map_or(UNKNOWN_ID, |action| action.id.as_str());
    let code = match envelope {
        RunEnvelope::Ok(_) => "ok",
        RunEnvelope::Err(error) => error.code.as_str(),
    };
    tracing::info!(
        tool = %tool,
        action = %action,
        ms = elapsed.as_millis() as u64,
        code,
        "run"
    );
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
    let report = move |p: Progress| {
        let _ = progress.send(p);
    };
    let envelope = execute(
        Arc::clone(state.inner()),
        tool,
        action,
        run_id,
        input,
        report,
    )
    .await?;
    serde_json::to_vec(&envelope)
        .map(Response::new)
        .map_err(|error| error.to_string())
}

/// `run_tool` without the Tauri types, so tests drive the same path: checks,
/// run registration, the tool call under `spawn_blocking`, and the log line.
/// `Err` is an IPC-level refusal (bad or duplicate run id); everything else,
/// tool errors included, is an envelope.
pub async fn execute(
    app: Arc<AppState>,
    tool: String,
    action: String,
    run_id: String,
    input: Value,
    report: impl Fn(Progress) + Send + Sync + 'static,
) -> Result<RunEnvelope, String> {
    if !valid_run_id(&run_id) {
        return Err("invalid run id".to_owned());
    }
    let started = Instant::now();
    let envelope = match precheck(&tool, &action, &input) {
        Err(error) => RunEnvelope::Err(error),
        Ok(()) => {
            let run = app
                .begin_run(&run_id)
                .ok_or_else(|| "duplicate run id".to_owned())?;
            let cancel = run.cancel_flag();
            let worker = Arc::clone(&app);
            let (tool, action) = (tool.clone(), action.clone());
            let joined = tauri::async_runtime::spawn_blocking(move || {
                let env = RunEnv {
                    cancel: &cancel,
                    progress: &report,
                    services: &worker.services,
                };
                worker.registry.run(&tool, &action, input, &env)
            })
            .await;
            drop(run);
            match joined {
                Ok(Ok(value)) => RunEnvelope::Ok(value),
                Ok(Err(error)) => RunEnvelope::Err(error),
                // The registry already contains tool panics; this is the worker itself.
                Err(_) => RunEnvelope::Err(ToolError::panicked()),
            }
        }
    };
    log_run(&app.registry, &tool, &action, started.elapsed(), &envelope);
    Ok(envelope)
}

/// True if a running run was signalled. Otherwise the id is remembered for a
/// while, so a run that registers it later starts cancelled.
#[tauri::command]
pub fn cancel_run(state: State<'_, Arc<AppState>>, run_id: String) -> bool {
    valid_run_id(&run_id) && state.cancel_run(&run_id)
}

/// Copies through Rust, off the main thread, always with the OS's clipboard
/// exclusion markers. clipboard.rs lists exactly what each OS gets.
#[tauri::command(async)]
pub fn copy_text(state: State<'_, Arc<AppState>>, text: String) -> Result<(), String> {
    state.clipboard.copy(text)
}

#[tauri::command]
pub fn settings_get(state: State<'_, Arc<AppState>>) -> Settings {
    state.settings.get()
}

/// Runs off the main thread: it writes the settings file.
#[tauri::command(async)]
pub fn settings_set<R: Runtime>(
    state: State<'_, Arc<AppState>>,
    window: WebviewWindow<R>,
    settings: Settings,
) -> Result<(), String> {
    // Async, so off the main thread: apply_theme may wait on it (Linux).
    state
        .settings
        .set(settings, |saved| apply_theme(&window, saved.theme))
}

/// Shows the log folder in the OS file manager, creating it if it is
/// missing. Off the main thread: it touches the disk and starts a program.
#[tauri::command(async)]
pub fn open_logs(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let Some(app_dir) = state.app_dir.as_deref() else {
        return Err("Navaja has no folder for its files on this system.".to_owned());
    };
    let logs = crate::paths::logs_dir(app_dir);
    crate::paths::ensure_private_dir(&logs).map_err(|error| {
        tracing::warn!(%error, "could not create the log folder");
        "Could not create the log folder.".to_owned()
    })?;
    crate::opener::open_folder(&logs)
}

/// Opens one of the URLs Navaja links to in the default browser; any other
/// URL is refused before anything starts. Off the main thread: it starts a
/// program.
#[tauri::command(async)]
pub fn open_url(url: String) -> Result<(), String> {
    if !crate::opener::is_allowed_url(&url) {
        // Not even the scheme: all of it comes from the webview.
        tracing::warn!("refused to open a URL Navaja does not link to");
        return Err(OPEN_URL_REFUSED.to_owned());
    }
    crate::opener::open_url(&url)
}

/// Quits through the same path as the tray's Quit, so the same clean-up
/// runs.
#[tauri::command]
pub fn quit<R: Runtime>(app: AppHandle<R>) {
    crate::quit(&app, crate::QuitFrom::Command);
}

pub fn apply_theme<R: Runtime>(window: &WebviewWindow<R>, theme: Theme) {
    let native = match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    let _ = window.set_theme(native);
    // On Linux, set_theme(None) turns GTK's prefer-dark setting off rather
    // than following the desktop, and WebKitGTK derives prefers-color-scheme
    // from that setting. With no preference set, theme() reads the desktop's
    // scheme from the XDG portal, so apply it explicitly.
    #[cfg(target_os = "linux")]
    if native.is_none()
        && let Ok(os) = window.theme()
    {
        let _ = window.set_theme(Some(os));
    }
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

    #[test]
    fn malformed_ids_are_refused_without_echo() {
        let input = serde_json::json!({});
        assert!(precheck("uuid", "generate", &input).is_ok());
        let long = "a".repeat(65);
        for tool in ["", "Uuid", "uuid\nforged line", "../uuid", long.as_str()] {
            let error = precheck(tool, "generate", &input).unwrap_err();
            assert_eq!(error.code, ErrorCode::UNKNOWN_TOOL, "{tool:?}");
            assert_eq!(error.details, None);
        }
        for action in ["", "Generate", "go\nforged line", "a-b", long.as_str()] {
            let error = precheck("uuid", action, &input).unwrap_err();
            assert_eq!(error.code, ErrorCode::UNKNOWN_ACTION, "{action:?}");
            assert_eq!(error.details, None);
        }
    }

    /// Only refused URLs: an accepted one would start a browser.
    #[test]
    fn open_url_refuses_what_navaja_does_not_link_to() {
        let repository = crate::opener::REPOSITORY;
        for url in [
            String::new(),
            format!("{repository}/issues"),
            format!("{repository}/raw/0123456789abcdef0123456789abcdef01234567/page.html"),
            format!("{repository}/archive/0123456789abcdef0123456789abcdef01234567.zip"),
            format!("{repository}#readme"),
            repository.to_lowercase(),
            "https://example.com/".to_owned(),
            "file:///etc/passwd".to_owned(),
        ] {
            assert_eq!(
                open_url(url.clone()),
                Err("Navaja opens only its own links.".to_owned()),
                "{url:?}"
            );
        }
    }

    /// At `trace` level, a refused URL reaches neither the log nor the
    /// error the webview gets back.
    #[test]
    fn a_refused_url_is_never_logged_or_echoed() {
        let canary = crate::test_support::canary();
        let urls = [
            canary.clone(),
            format!("https://{canary}.example/"),
            format!("{}/{canary}", crate::opener::REPOSITORY),
            format!("{}#{canary}", crate::opener::REPOSITORY),
            format!("javascript:{canary}"),
        ];
        let (errors, log): (Vec<String>, _) = crate::test_support::capture_log(|| {
            urls.into_iter()
                .map(|url| open_url(url).unwrap_err())
                .collect()
        });

        // The refusals were logged, so the capture works.
        assert_eq!(log.matches("refused to open a URL").count(), 5, "{log}");
        let canary = canary.to_ascii_lowercase();
        assert!(!log.to_ascii_lowercase().contains(&canary), "{log}");
        for error in errors {
            assert!(!error.to_ascii_lowercase().contains(&canary), "{error}");
        }
    }

    #[test]
    fn input_limits_count_string_bytes_and_nodes() {
        use serde_json::json;
        // 3 + 2 + 1 key bytes plus 3 + 1 value bytes; 6 nodes.
        let input = json!({ "abc": "xyz", "de": [1, { "f": "g" }] });
        assert!(input_within(&input, 10, 6));
        assert!(!input_within(&input, 9, 6));
        assert!(!input_within(&input, 10, 5));
        assert!(input_within(&json!(null), 0, 1));
        assert!(!input_within(&json!([null]), 0, 1));
    }

    #[test]
    fn input_limits_at_the_real_caps() {
        let at_cap = Value::String("a".repeat(MAX_INPUT_BYTES));
        assert!(precheck("uuid", "generate", &at_cap).is_ok());
        let too_long = Value::String("a".repeat(MAX_INPUT_BYTES + 1));
        let error = precheck("uuid", "generate", &too_long).unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_INPUT);
        assert_eq!(error.details, None);
        let too_many = Value::Array(vec![Value::Null; MAX_INPUT_NODES]);
        let error = precheck("uuid", "generate", &too_many).unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_INPUT);
    }

    /// Always panics.
    struct Boom;

    impl navaja_core::Tool for Boom {
        fn meta(&self) -> ToolMeta {
            use navaja_core::{Category, GeneratorSpec, OutputKind, OutputSpec, UiSpec};
            ToolMeta {
                spec_version: navaja_core::SPEC_VERSION,
                id: ToolId::from_static("boom"),
                name: "Boom".into(),
                description: "Always panics.".into(),
                category: Category::GENERATORS,
                keywords: vec![],
                icon: r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M4 12h16"/></svg>"#.into(),
                capabilities: vec![],
                actions: vec![ActionMeta::new("boom", "Boom")],
                tray: false,
                ui: UiSpec::Generator(GeneratorSpec {
                    action: "boom".into(),
                    options: vec![],
                    outputs: vec![OutputSpec {
                        key: "text".into(),
                        label: "Text".into(),
                        format: OutputKind::Text,
                    }],
                    run_on_open: false,
                }),
            }
        }

        fn invoke(&self, _: &str, _: Value, _: &navaja_core::Ctx<'_>) -> Result<Value, ToolError> {
            panic!("boom");
        }
    }

    fn app() -> Arc<AppState> {
        let mut tools = navaja_tools::all();
        tools.push(Arc::new(Boom));
        let registry = Registry::new(tools).unwrap();
        Arc::new(AppState::new(
            registry,
            crate::settings::SettingsStore::load(None),
            None,
        ))
    }

    fn run(
        app: &Arc<AppState>,
        tool: &str,
        action: &str,
        input: Value,
    ) -> Result<RunEnvelope, String> {
        tauri::async_runtime::block_on(execute(
            Arc::clone(app),
            tool.to_owned(),
            action.to_owned(),
            "run-1".to_owned(),
            input,
            |_| {},
        ))
    }

    fn code(envelope: &RunEnvelope) -> &str {
        match envelope {
            RunEnvelope::Ok(_) => "ok",
            RunEnvelope::Err(error) => error.code.as_str(),
        }
    }

    #[test]
    fn every_run_frees_its_id() {
        let app = app();
        let cases = [
            ("uuid", "generate", serde_json::json!({}), "ok"),
            (
                "uuid",
                "generate",
                serde_json::json!({ "count": 0 }),
                "uuid.count_out_of_range",
            ),
            (
                "uuid",
                "generate",
                serde_json::json!({ "x": 1 }),
                "core.invalid_input",
            ),
            ("uuid", "nope", serde_json::json!({}), "core.unknown_action"),
            (
                "Bad Id",
                "generate",
                serde_json::json!({}),
                "core.unknown_tool",
            ),
            ("boom", "boom", serde_json::json!({}), "core.panicked"),
        ];
        for (tool, action, input, expected) in cases {
            let envelope = run(&app, tool, action, input).unwrap();
            assert_eq!(code(&envelope), expected, "{tool}.{action}");
            assert!(
                app.begin_run("run-1").is_some(),
                "{tool}.{action} kept its id"
            );
        }
    }

    /// Roadmap M2a exit: a panicking tool returns `core.panicked` and the app
    /// keeps serving. The next run, on the same state and with the same run
    /// id, reaches a real tool and returns its output.
    #[test]
    fn a_panicking_tool_leaves_the_app_serving() {
        let app = app();
        for _ in 0..2 {
            let envelope = run(&app, "boom", "boom", serde_json::json!({})).unwrap();
            assert_eq!(code(&envelope), "core.panicked");
            let envelope = run(&app, "uuid", "generate", serde_json::json!({})).unwrap();
            let RunEnvelope::Ok(output) = envelope else {
                panic!("the run after a panic failed: {}", code(&envelope));
            };
            let groups: Vec<usize> = output["uuids"]
                .as_str()
                .unwrap_or_default()
                .split('-')
                .map(str::len)
                .collect();
            assert_eq!(groups, [8, 4, 4, 4, 12], "{output}");
        }
    }

    #[test]
    fn run_ids_are_checked_before_the_run() {
        let app = app();
        let held = app.begin_run("run-1").unwrap();
        let duplicate = run(&app, "uuid", "generate", serde_json::json!({}));
        assert_eq!(duplicate.unwrap_err(), "duplicate run id");
        drop(held);
        let invalid = tauri::async_runtime::block_on(execute(
            app,
            "uuid".to_owned(),
            "generate".to_owned(),
            "bad id".to_owned(),
            serde_json::json!({}),
            |_| {},
        ));
        assert_eq!(invalid.unwrap_err(), "invalid run id");
    }

    #[test]
    fn a_cancel_sent_before_the_run_stops_it() {
        let app = app();
        assert!(!app.cancel_run("run-1"));
        let envelope = run(&app, "uuid", "generate", serde_json::json!({})).unwrap();
        assert_eq!(code(&envelope), "core.cancelled");
    }
}
