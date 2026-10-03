//! Registry behaviour with small fake tools.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::*;

const ICON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M4 12h16"/></svg>"#;

fn meta(id: &'static str, ui: UiSpec, actions: Vec<ActionMeta>) -> ToolMeta {
    ToolMeta {
        spec_version: SPEC_VERSION,
        id: ToolId::from_static(id),
        name: format!("{id} tool"),
        description: "A test tool.".into(),
        category: Category::ENCODERS,
        keywords: vec!["test".into()],
        icon: ICON.into(),
        capabilities: vec![],
        actions,
        tray: false,
        ui,
    }
}

fn text_output() -> OutputSpec {
    OutputSpec {
        key: "text".into(),
        label: "Text".into(),
        format: OutputKind::Text,
    }
}

fn transform(modes: &[&str]) -> UiSpec {
    UiSpec::Transform(TransformSpec {
        modes: modes.iter().map(|m| (*m).to_owned()).collect(),
        input: InputSpec::default(),
        options: vec![OptionSpec {
            key: "upper".into(),
            label: "Upper case".into(),
            control: Control::Toggle { default: false },
            modes: vec![],
        }],
        outputs: vec![text_output()],
        live: true,
    })
}

fn generator(action: &str) -> UiSpec {
    UiSpec::Generator(GeneratorSpec {
        action: action.into(),
        options: vec![],
        outputs: vec![text_output()],
        run_on_open: true,
    })
}

/// Echoes its input, optionally upper-cased.
struct Echo;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EchoInput {
    input: String,
    #[serde(default)]
    upper: bool,
}

impl Tool for Echo {
    fn meta(&self) -> ToolMeta {
        meta(
            "echo",
            transform(&["echo"]),
            vec![ActionMeta::new("echo", "Echo")],
        )
    }

    fn invoke(&self, _action: &str, input: Value, ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        let input: EchoInput = typed(input)?;
        ctx.check()?;
        let text = if input.upper {
            input.input.to_uppercase()
        } else {
            input.input
        };
        Ok(json!({ "text": text }))
    }
}

/// Misbehaves in the way its action name says.
struct Rogue;

const ROGUE_ACTIONS: &[&str] = &[
    "panic",
    "extra",
    "missing",
    "wrong_shape",
    "foreign_code",
    "host_code",
];

impl Tool for Rogue {
    fn meta(&self) -> ToolMeta {
        let actions: Vec<ActionMeta> = ROGUE_ACTIONS
            .iter()
            .map(|a| ActionMeta::new(a, a))
            .collect();
        meta("rogue", transform(ROGUE_ACTIONS), actions)
    }

    fn invoke(&self, action: &str, _input: Value, _ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        match action {
            "panic" => panic!("secret input in a panic message"),
            "extra" => Ok(json!({ "text": "x", "surprise": 1 })),
            "missing" => Ok(json!({})),
            "wrong_shape" => Ok(json!({ "text": 42 })),
            "foreign_code" => Err(ToolError::new(
                ErrorCode::from_static("echo.borrowed"),
                "nope",
            )),
            _ => Err(ToolError::new(ErrorCode::UNKNOWN_TOOL, "pretending")),
        }
    }
}

fn registry() -> Registry {
    Registry::new(vec![Arc::new(Echo), Arc::new(Rogue)]).expect("valid tools")
}

fn run(registry: &Registry, id: &str, action: &str, input: Value) -> Result<Value, ToolError> {
    with_detached_env(|env| registry.run(id, action, input, env))
}

#[test]
fn runs_a_declared_action() {
    let registry = registry();
    let out = run(
        &registry,
        "echo",
        "echo",
        json!({ "input": "hi", "upper": true }),
    )
    .unwrap();
    assert_eq!(out, json!({ "text": "HI" }));
}

#[test]
fn run_single_applies_the_registry_checks() {
    let out = run_single(Echo, "echo", json!({ "input": "x" })).unwrap();
    assert_eq!(out, json!({ "text": "x" }));
    let err = run_single(Echo, "nope", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::UNKNOWN_ACTION);
}

#[test]
fn unknown_tool_and_action() {
    let registry = registry();
    let err = run(&registry, "nope", "echo", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::UNKNOWN_TOOL);
    let err = run(&registry, "echo", "shout", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::UNKNOWN_ACTION);
}

#[test]
fn invalid_input_never_reaches_the_tool_logic() {
    let registry = registry();
    let err = run(&registry, "echo", "echo", json!({ "inptu": "typo" })).unwrap_err();
    assert_eq!(err.code, ErrorCode::INVALID_INPUT);
}

#[test]
fn cancellation_is_reported() {
    let registry = registry();
    let cancel = AtomicBool::new(true);
    let services = Services::new();
    let env = RunEnv {
        cancel: &cancel,
        progress: &|_| {},
        services: &services,
    };
    let err = registry
        .run("echo", "echo", json!({ "input": "x" }), &env)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::CANCELLED);
    assert!(cancel.load(Ordering::Relaxed));
}

#[test]
fn panics_become_errors_without_the_payload() {
    let registry = registry();
    let err = run(&registry, "rogue", "panic", json!({})).unwrap_err();
    // Code, message and details are exactly the generic ones.
    assert_eq!(err, ToolError::panicked());
    // The registry keeps serving after a panic.
    assert!(run(&registry, "echo", "echo", json!({ "input": "ok" })).is_ok());
}

#[cfg(debug_assertions)]
#[test]
fn outputs_and_codes_are_checked_with_debug_assertions() {
    let registry = registry();
    for action in [
        "extra",
        "missing",
        "wrong_shape",
        "foreign_code",
        "host_code",
    ] {
        let err = run(&registry, "rogue", action, json!({})).unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_OUTPUT, "{action}");
    }
}

#[cfg(not(debug_assertions))]
#[test]
fn outputs_pass_through_unchecked_in_release_builds() {
    let registry = registry();
    let out = run(&registry, "rogue", "extra", json!({})).unwrap();
    assert_eq!(out, json!({ "text": "x", "surprise": 1 }));
    let err = run(&registry, "rogue", "foreign_code", json!({})).unwrap_err();
    assert_eq!(err.code.as_str(), "echo.borrowed");
}

#[test]
fn search_and_categories_go_through_the_registry() {
    let registry = registry();
    let hits = registry.search("echo");
    assert_eq!(hits.first().map(|h| h.id.as_str()), Some("echo"));
    let categories = registry.categories();
    assert_eq!(categories.len(), 1);
    assert_eq!(categories[0].id, Category::ENCODERS);
    assert_eq!(categories[0].label, "Encoders");
}

#[derive(Debug)]
struct Engine;

/// Reports whether it can reach the `Engine` service.
struct ServiceProbe(&'static str, Vec<Capability>);

impl Tool for ServiceProbe {
    fn meta(&self) -> ToolMeta {
        let mut m = meta(
            self.0,
            transform(&["go"]),
            vec![ActionMeta::new("go", "Go")],
        );
        m.capabilities.clone_from(&self.1);
        m
    }
    fn invoke(&self, _: &str, _: Value, ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        Ok(json!({ "text": ctx.service::<Engine>().is_some().to_string() }))
    }
}

#[test]
fn run_grants_exactly_the_declared_capabilities() {
    let registry = Registry::new(vec![
        Arc::new(ServiceProbe("with", vec![Capability::CONTAINER_ENGINE])),
        Arc::new(ServiceProbe("without", vec![])),
    ])
    .unwrap();
    let cancel = AtomicBool::new(false);
    let mut services = Services::new();
    services
        .insert(Capability::CONTAINER_ENGINE, Engine)
        .unwrap();
    let env = RunEnv {
        cancel: &cancel,
        progress: &|_| {},
        services: &services,
    };
    let with = registry.run("with", "go", json!({}), &env).unwrap();
    let without = registry.run("without", "go", json!({}), &env).unwrap();
    assert_eq!(with["text"], "true");
    assert_eq!(without["text"], "false");
}

struct Custom<F: Fn() -> ToolMeta + Send + Sync + 'static>(F);

impl<F: Fn() -> ToolMeta + Send + Sync + 'static> Tool for Custom<F> {
    fn meta(&self) -> ToolMeta {
        (self.0)()
    }
    fn invoke(&self, _: &str, _: Value, _: &Ctx<'_>) -> Result<Value, ToolError> {
        Ok(Value::Null)
    }
}

fn problems_for(make: impl Fn() -> ToolMeta + Send + Sync + 'static) -> Vec<String> {
    match Registry::new(vec![Arc::new(Custom(make))]) {
        Ok(_) => vec![],
        Err(errors) => errors.into_iter().map(|e| e.problem).collect(),
    }
}

fn valid() -> ToolMeta {
    meta(
        "good",
        transform(&["go"]),
        vec![ActionMeta::new("go", "Go")],
    )
}

fn with_transform(f: impl Fn(&mut TransformSpec) + Send + Sync + 'static) -> Mutation {
    Box::new(move |m| {
        if let UiSpec::Transform(spec) = &mut m.ui {
            f(spec);
        }
    })
}

fn with_option(f: impl Fn(&mut OptionSpec) + Send + Sync + 'static) -> Mutation {
    with_transform(move |spec| f(&mut spec.options[0]))
}

/// Breaks one aspect of otherwise valid metadata.
type Mutation = Box<dyn Fn(&mut ToolMeta) + Send + Sync>;

#[test]
fn rejects_malformed_metadata_with_a_specific_problem() {
    assert!(problems_for(valid).is_empty());

    let cases: Vec<(&str, Mutation)> = vec![
        (
            "id must match",
            Box::new(|m| m.id = ToolId::from_static("Bad-Id")),
        ),
        (
            "reserved for extensions",
            Box::new(|m| m.id = ToolId::from_static("acme__good")),
        ),
        (
            "reserved for the host",
            Box::new(|m| m.id = ToolId::from_static("core")),
        ),
        ("spec_version", Box::new(|m| m.spec_version = 99)),
        ("name is empty", Box::new(|m| m.name = " ".into())),
        (
            "description is empty",
            Box::new(|m| m.description = String::new()),
        ),
        (
            "category",
            Box::new(|m| m.category = Category::from_static("Bad Category")),
        ),
        ("keyword", Box::new(|m| m.keywords = vec!["Test".into()])),
        ("icon", Box::new(|m| m.icon = "<svg/>".into())),
        (
            "capability",
            Box::new(|m| m.capabilities = vec![Capability::from_static("x")]),
        ),
        (
            "capability",
            Box::new(|m| m.capabilities = vec![Capability::PROCESS_KILL, Capability::PROCESS_KILL]),
        ),
        (
            "not reachable",
            Box::new(|m| m.actions.push(ActionMeta::new("hidden", "Hidden"))),
        ),
        (
            "repeated",
            Box::new(|m| m.actions.push(ActionMeta::new("go", "Again"))),
        ),
        (
            "has no label",
            Box::new(|m| m.actions[0].label = String::new()),
        ),
        (
            "is not a declared action",
            Box::new(|m| {
                m.ui = transform(&["go", "nope"]);
            }),
        ),
        (
            "cannot be destructive",
            Box::new(|m| m.actions[0] = ActionMeta::new("go", "Go").destructive()),
        ),
        ("generator action", Box::new(|m| m.ui = generator("nope"))),
        (
            "generator action `go` cannot be destructive",
            Box::new(|m| {
                m.ui = generator("go");
                m.actions[0] = ActionMeta::new("go", "Go").destructive();
            }),
        ),
        (
            "generator action `go` cannot be destructive",
            Box::new(|m| {
                m.ui = generator("go");
                if let UiSpec::Generator(spec) = &mut m.ui {
                    spec.run_on_open = false;
                }
                m.actions[0] = ActionMeta::new("go", "Go").destructive();
            }),
        ),
        (
            "action `a",
            Box::new(|m| {
                let long = "a".repeat(ActionMeta::MAX_ID_LEN + 1);
                m.actions[0] = ActionMeta::new(&long, "Go");
                m.ui = transform(&[&long]);
            }),
        ),
        (
            "must equal the tool id",
            Box::new(|m| {
                m.ui = UiSpec::Custom {
                    view: "other".into(),
                }
            }),
        ),
        ("not supported", Box::new(|m| m.ui = UiSpec::Unsupported)),
        ("reserved", with_option(|o| o.key = "input".into())),
        ("reserved", with_option(|o| o.key = "file".into())),
        (
            "unknown mode",
            with_option(|o| o.modes = vec!["nope".into()]),
        ),
        (
            "choices",
            with_option(|o| {
                o.control = Control::Choice {
                    choices: vec![Choice::new("a", "A")],
                    default: "b".into(),
                };
            }),
        ),
        (
            "choices",
            with_option(|o| {
                o.control = Control::Choice {
                    choices: vec![Choice::new("a", "A"), Choice::new("a", "Again")],
                    default: "a".into(),
                };
            }),
        ),
        (
            "min <= default <= max",
            with_option(|o| {
                o.control = Control::Integer {
                    min: 5,
                    max: 1,
                    default: 3,
                };
            }),
        ),
        (
            "2^53",
            with_option(|o| {
                o.control = Control::Integer {
                    min: 0,
                    max: i64::MAX,
                    default: 0,
                };
            }),
        ),
        (
            "2^53",
            with_option(|o| {
                o.control = Control::Integer {
                    min: i64::MIN,
                    max: 0,
                    default: 0,
                };
            }),
        ),
        (
            "longer than its limit",
            with_option(|o| {
                o.control = Control::Text {
                    default: "abcdef".into(),
                    limit: 3,
                };
            }),
        ),
        (
            "control kind",
            with_option(|o| o.control = Control::Unsupported),
        ),
        ("no outputs", with_transform(|spec| spec.outputs.clear())),
        (
            "malformed or repeated",
            with_transform(|spec| spec.outputs.push(text_output())),
        ),
        (
            "needs a lang",
            with_transform(|spec| {
                spec.outputs[0].format = OutputKind::Code { lang: " ".into() };
            }),
        ),
        (
            "format not supported",
            with_transform(|spec| {
                spec.outputs[0].format = OutputKind::Unsupported;
            }),
        ),
    ];
    for (expected, mutate) in cases {
        let mutate = Arc::new(mutate);
        let problems = problems_for(move || {
            let mut m = valid();
            mutate(&mut m);
            m
        });
        assert!(
            problems.iter().any(|p| p.contains(expected)),
            "expected a problem mentioning {expected:?}, got {problems:?}"
        );
    }
}

#[test]
fn reports_every_problem_of_every_tool() {
    let broken = |id: &'static str| {
        Custom(move || {
            let mut m = meta(id, transform(&["go"]), vec![ActionMeta::new("go", "Go")]);
            m.name = String::new();
            m.keywords = vec!["UPPER".into()];
            m
        })
    };
    let errors = Registry::new(vec![Arc::new(broken("one")), Arc::new(broken("two"))]).unwrap_err();
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert_eq!(errors.iter().filter(|e| e.tool == "one").count(), 2);
}

#[test]
fn rejects_duplicate_ids() {
    let errors = Registry::new(vec![Arc::new(Custom(valid)), Arc::new(Custom(valid))]).unwrap_err();
    assert!(errors.iter().any(|e| e.problem.contains("duplicate")));
}
