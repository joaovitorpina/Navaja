//! The privacy canary (docs/roadmap.md, "On every PR"): at `trace` level,
//! text sent to a run, including a tool's panic message and error message,
//! never reaches the log or crash files.
//!
//! Its own test binary, because the panic hook and the log subscriber are
//! process-wide.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use navaja_core::{
    ActionMeta, Category, Ctx, ErrorCode, InputSpec, OutputKind, OutputSpec, Registry, Tool,
    ToolError, ToolId, ToolMeta, TransformSpec, UiSpec, Value,
};
use navaja_lib::testing::{
    AppState, RunEnvelope, SettingsStore, execute, init_logging, install_crash_hook,
};
use serde_json::json;

const CANARY: &str = "NAVAJA-CANARY-3f9c";
/// The canary in the id grammar, so it passes the id shape checks.
const CANARY_ID: &str = "navaja_canary_3f9c";

const ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M4 12h16"/></svg>"#;

/// Echoes, fails with or panics with its input, by action.
struct Canary;

impl Tool for Canary {
    fn meta(&self) -> ToolMeta {
        let modes = ["echo", "fail", "boom"];
        ToolMeta {
            spec_version: navaja_core::SPEC_VERSION,
            id: ToolId::from_static("canary"),
            name: "Canary".into(),
            description: "Puts its input everywhere it can.".into(),
            category: Category::ENCODERS,
            keywords: vec![],
            icon: ICON.into(),
            capabilities: vec![],
            actions: modes.iter().map(|m| ActionMeta::new(m, m)).collect(),
            tray: false,
            ui: UiSpec::Transform(TransformSpec {
                modes: modes.map(String::from).to_vec(),
                input: InputSpec::default(),
                options: vec![],
                outputs: vec![OutputSpec {
                    key: "text".into(),
                    label: "Text".into(),
                    format: OutputKind::Text,
                }],
                live: false,
            }),
        }
    }

    fn invoke(&self, action: &str, input: Value, _ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        let text = input["input"].as_str().unwrap_or_default().to_owned();
        match action {
            "echo" => Ok(json!({ "text": text })),
            "fail" => Err(ToolError::new(
                ErrorCode::from_static("canary.failed"),
                format!("failed on {text}"),
            )),
            _ => panic!("panicked on {text}"),
        }
    }
}

fn run(app: &Arc<AppState>, tool: &str, action: &str, input: Value) -> RunEnvelope {
    tauri::async_runtime::block_on(execute(
        Arc::clone(app),
        tool.to_owned(),
        action.to_owned(),
        "canary-run".to_owned(),
        input,
        |_| {},
    ))
    .expect("the run id is valid and free")
}

fn code(envelope: &RunEnvelope) -> &str {
    match envelope {
        RunEnvelope::Ok(_) => "ok",
        RunEnvelope::Err(error) => error.code.as_str(),
    }
}

fn panic_on_this_thread() {
    panic!("thread panicked on {CANARY}");
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn canary_input_never_reaches_logs_or_crash_files() {
    let dir = std::env::temp_dir().join(format!("navaja-privacy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    install_crash_hook(Some(dir.clone()));
    let logs = init_logging(Some(&dir), "trace");

    let mut tools = navaja_tools::all();
    tools.push(Arc::new(Canary));
    let registry = Registry::new(tools).expect("valid tools");
    let app = Arc::new(AppState::new(registry, SettingsStore::load(None), None));

    let text = json!({ "input": CANARY });
    let cases = [
        // A run that succeeds with the canary in its input and output.
        ("canary", "echo", text.clone(), "ok"),
        // A tool error whose message quotes the input.
        ("canary", "fail", text.clone(), "canary.failed"),
        // A tool panic whose message quotes the input.
        ("canary", "boom", text.clone(), "core.panicked"),
        // Input the tool rejects: a bad value, then an unknown field.
        (
            "uuid",
            "generate",
            json!({ "format": CANARY }),
            "core.invalid_input",
        ),
        (
            "uuid",
            "generate",
            json!({ CANARY: true }),
            "core.invalid_input",
        ),
        // Ids that are not the registry's, malformed and well-formed.
        (CANARY, "generate", json!({}), "core.unknown_tool"),
        ("uuid", CANARY, json!({}), "core.unknown_action"),
        (CANARY_ID, "generate", json!({}), "core.unknown_tool"),
        ("uuid", CANARY_ID, json!({}), "core.unknown_action"),
    ];
    let mut outcomes = Vec::new();
    for (tool, action, input, expected) in cases {
        let envelope = run(&app, tool, action, input);
        outcomes.push((tool, action, code(&envelope).to_owned(), expected));
    }
    // A panic outside any tool, on a plain thread.
    let joined = std::thread::spawn(panic_on_this_thread).join();
    drop(logs);
    // Back to the default hook, so a failed assertion below prints its message.
    drop(std::panic::take_hook());

    for (tool, action, got, expected) in outcomes {
        assert_eq!(got, expected, "{tool}.{action}");
    }
    assert!(joined.is_err());

    let files = files_under(&dir);
    let crash_files = files
        .iter()
        .filter(|p| p.parent() == Some(dir.join("crash").as_path()))
        .count();
    assert!(crash_files >= 1, "no crash file in {files:?}");
    let log = files
        .iter()
        .filter(|p| p.parent() == Some(dir.join("logs").as_path()))
        .map(|p| std::fs::read_to_string(p).expect("read log file"))
        .collect::<String>();
    // The log saw the runs and the panics, without their text.
    assert!(log.contains(r#"code="canary.failed""#), "{log}");
    assert!(log.contains("tool=uuid action=<unknown>"), "{log}");
    assert!(log.contains("panic (payload withheld)"), "{log}");
    for file in &files {
        let bytes = std::fs::read(file).expect("read file");
        let content = String::from_utf8_lossy(&bytes).to_lowercase();
        for canary in [CANARY, CANARY_ID] {
            assert!(
                !content.contains(&canary.to_lowercase()),
                "{} contains {canary}",
                file.display()
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
