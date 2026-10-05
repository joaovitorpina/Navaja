//! Checks that keep "one folder plus one registration line" honest
//! (docs/architecture.md §3, §4).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use navaja_core::{
    Control, Ctx, ErrorCode, OptionSpec, Registry, Services, ToolMeta, UiSpec, Value,
    with_detached_env,
};
use serde_json::{Map, json};

use crate::{MODULES, all};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn registry() -> Registry {
    Registry::new(all()).unwrap_or_else(|problems| {
        let list: Vec<String> = problems.iter().map(ToString::to_string).collect();
        panic!("invalid tool metadata:\n{}", list.join("\n"));
    })
}

#[test]
fn every_tool_folder_is_registered_and_the_list_is_sorted() {
    let mut folders: Vec<String> = std::fs::read_dir(root())
        .expect("read tools/")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("mod.rs").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    folders.sort();
    let mut listed = MODULES.to_vec();
    listed.sort_unstable();
    assert_eq!(
        folders, listed,
        "every tools/<id>/mod.rs needs exactly one line in register_tools! (tools/lib.rs)"
    );
    assert_eq!(
        MODULES,
        listed.as_slice(),
        "keep register_tools! sorted, one id per line"
    );
}

#[test]
fn registry_accepts_every_tool() {
    assert_eq!(registry().len(), MODULES.len());
}

/// Roadmap M2a exit: searching "uid" ranks the UUID generator first, on the
/// registry the app ships rather than a fixture. A new tool that outranks it
/// fails here.
#[test]
fn uid_finds_uuid_first() {
    let hits = registry().search("uid");
    assert_eq!(
        hits.first().map(|hit| hit.id.as_str()),
        Some("uuid"),
        "search(\"uid\") returned {hits:?}"
    );
}

#[test]
fn ids_equal_folder_names() {
    for (tool, module) in all().iter().zip(MODULES) {
        assert_eq!(tool.meta().id.as_str(), *module);
    }
}

#[test]
fn custom_views_exist_exactly_for_custom_tools() {
    for tool in all() {
        let meta = tool.meta();
        let ui = root().join(meta.id.as_str()).join("ui");
        match meta.ui {
            UiSpec::Custom { .. } => {
                assert!(
                    ui.join("View.svelte").is_file(),
                    "{}: missing ui/View.svelte",
                    meta.id
                );
                assert!(
                    ui.join("i18n").join("en.ts").is_file(),
                    "{}: missing ui/i18n/en.ts",
                    meta.id
                );
            }
            _ => assert!(
                !ui.exists(),
                "{}: generic tools have no ui/ folder; use UiSpec::Custom for a custom view",
                meta.id
            ),
        }
    }
}

/// Every declared action, given a pre-cancelled context and a junk input,
/// returns `core.invalid_input` or `core.cancelled`. Tools must parse with
/// `typed()` and call `ctx.check()` before any side effect; this probe catches
/// tools that skip both.
#[test]
fn actions_refuse_junk_input_and_cancellation() {
    let cancel = AtomicBool::new(true);
    let services = Services::new();
    for tool in all() {
        let meta = tool.meta();
        let ctx = Ctx::new(&cancel, &|_| {}, &services, &meta.capabilities);
        for action in &meta.actions {
            let error = tool
                .invoke(&action.id, json!({ "__navaja_probe__": true }), &ctx)
                .expect_err("a probe input must be refused");
            assert!(
                error.code == ErrorCode::INVALID_INPUT || error.code == ErrorCode::CANCELLED,
                "{}.{}: refused with {} instead of core.invalid_input or core.cancelled",
                meta.id,
                action.id,
                error.code
            );
        }
    }
}

/// The object the generic view sends: every applicable option at its default.
fn default_input(meta: &ToolMeta, mode: &str) -> Map<String, Value> {
    let (options, is_transform): (&[OptionSpec], bool) = match &meta.ui {
        UiSpec::Transform(spec) => (&spec.options, true),
        UiSpec::Generator(spec) => (&spec.options, false),
        _ => (&[], false),
    };
    let mut input = Map::new();
    if is_transform {
        input.insert("input".into(), json!(""));
    }
    for option in options {
        if !option.modes.is_empty() && !option.modes.iter().any(|m| m == mode) {
            continue;
        }
        let value = match &option.control {
            Control::Toggle { default } => json!(default),
            Control::Choice { default, .. } => json!(default),
            Control::Integer { default, .. } => json!(default),
            Control::Text { default, .. } => json!(default),
            Control::Unsupported => continue,
        };
        input.insert(option.key.clone(), value);
    }
    input
}

/// Every value the generic view can send for one option.
fn option_values(option: &OptionSpec) -> Vec<Value> {
    match &option.control {
        Control::Toggle { .. } => vec![json!(false), json!(true)],
        Control::Choice { choices, .. } => choices.iter().map(|c| json!(c.value)).collect(),
        Control::Integer { min, max, .. } => vec![json!(min), json!(max)],
        Control::Text { .. } | Control::Unsupported => vec![],
    }
}

/// The generic views' contract with the tool's input type: defaults, every
/// choice and both integer bounds are accepted, and outputs pass the debug
/// checks.
#[test]
fn generic_views_inputs_are_accepted() {
    let registry = registry();
    for meta in registry.metas() {
        let (modes, options): (Vec<String>, Vec<OptionSpec>) = match &meta.ui {
            UiSpec::Generator(spec) => (vec![spec.action.clone()], spec.options.clone()),
            UiSpec::Transform(spec) => (spec.modes.clone(), spec.options.clone()),
            _ => continue,
        };
        for mode in &modes {
            let run = |input: Map<String, Value>| {
                with_detached_env(|env| {
                    registry.run(meta.id.as_str(), mode, Value::Object(input), env)
                })
            };
            let base = default_input(meta, mode);
            match (&meta.ui, run(base.clone())) {
                // Pure generators must succeed with their defaults.
                (UiSpec::Generator(_), Err(error)) if meta.capabilities.is_empty() => {
                    panic!("{}.{mode} with defaults: {error}", meta.id);
                }
                (_, Err(error)) => assert!(
                    !matches!(
                        error.code.as_str(),
                        "core.invalid_input" | "core.invalid_output" | "core.panicked"
                    ),
                    "{}.{mode} with defaults: {error}",
                    meta.id
                ),
                _ => {}
            }
            for option in options
                .iter()
                .filter(|o| o.modes.is_empty() || o.modes.contains(mode))
            {
                for value in option_values(option) {
                    let mut input = base.clone();
                    input.insert(option.key.clone(), value.clone());
                    if let Err(error) = run(input) {
                        assert_ne!(
                            error.code,
                            ErrorCode::INVALID_INPUT,
                            "{}.{mode}: option {}={value} rejected",
                            meta.id,
                            option.key
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn undeclared_actions_are_rejected() {
    let registry = registry();
    for meta in registry.metas() {
        let error = with_detached_env(|env| {
            registry.run(meta.id.as_str(), "no_such_action", json!({}), env)
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UNKNOWN_ACTION);
    }
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out
}

/// Every `ErrorCode::from_static("…")` in a tool's folder is `<id>.<snake>`,
/// whichever code paths the tests happen to exercise.
#[test]
fn error_code_literals_use_the_tool_namespace() {
    const NEEDLE: &str = "ErrorCode::from_static(\"";
    for module in MODULES {
        for file in rust_files(&root().join(module)) {
            let source = std::fs::read_to_string(&file).expect("read tool source");
            for (offset, _) in source.match_indices(NEEDLE) {
                let rest = &source[offset + NEEDLE.len()..];
                let code = rest.split('"').next().unwrap_or("");
                assert!(
                    ErrorCode::is_valid(code) && code.split('.').next() == Some(*module),
                    "{}: error code {code:?} must be `{module}.<snake_case>`",
                    file.display()
                );
            }
        }
    }
}
