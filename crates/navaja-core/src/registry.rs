//! The tool registry: validates every tool's metadata once at startup and
//! runs actions behind `catch_unwind`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use serde_json::Value;

use crate::ctx::{Ctx, RunEnv};
use crate::error::ToolError;
use crate::icon::validate_icon;
use crate::id::{Capability, Category, CategoryInfo, ErrorCode, ToolId, is_ident};
use crate::meta::{Control, OptionSpec, OutputKind, OutputSpec, ToolMeta, UiSpec};
use crate::payload::{Diagnostic, matches_kind};
use crate::search::{SearchHit, rank};
use crate::tool::Tool;

// Tool panics are contained with catch_unwind; aborting would take the whole
// app down with the first buggy tool (docs/architecture.md §3).
#[cfg(panic = "abort")]
compile_error!(
    "navaja-core requires panic = \"unwind\": Registry::run contains tool panics with catch_unwind"
);

/// Option keys the generic views reserve for themselves.
const RESERVED_OPTION_KEYS: &[&str] = &["input", "file"];

/// Tool ids that would collide with host namespaces (error codes, i18n keys).
const RESERVED_TOOL_IDS: &[&str] = &["core"];

/// `core.*` codes a tool itself may return; the others are the host's.
const TOOL_CORE_CODES: &[ErrorCode] = &[
    ErrorCode::INVALID_INPUT,
    ErrorCode::CANCELLED,
    ErrorCode::PANICKED,
];

/// Integer options travel as JavaScript numbers.
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryError {
    pub tool: String,
    pub problem: String,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tool `{}`: {}", self.tool, self.problem)
    }
}

impl std::error::Error for RegistryError {}

struct Entry {
    meta: ToolMeta,
    tool: Arc<dyn Tool>,
}

pub struct Registry {
    entries: Vec<Entry>,
    index: HashMap<ToolId, usize>,
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.index.keys()).finish()
    }
}

impl Registry {
    /// Validates every tool and fails with all problems found, not just the first.
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Result<Self, Vec<RegistryError>> {
        let mut problems = Vec::new();
        let mut entries = Vec::with_capacity(tools.len());
        let mut index = HashMap::new();

        for tool in tools {
            let meta = tool.meta();
            let mut found = validate_meta(&meta);
            if index.contains_key(&meta.id) {
                found.push("duplicate tool id".to_owned());
            }
            if found.is_empty() {
                index.insert(meta.id.clone(), entries.len());
                entries.push(Entry { meta, tool });
            } else {
                problems.extend(found.into_iter().map(|problem| RegistryError {
                    tool: meta.id.to_string(),
                    problem,
                }));
            }
        }

        if problems.is_empty() {
            Ok(Self { entries, index })
        } else {
            Err(problems)
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every tool's metadata, in registration order.
    pub fn metas(&self) -> impl Iterator<Item = &ToolMeta> {
        self.entries.iter().map(|entry| &entry.meta)
    }

    pub fn meta(&self, id: &str) -> Option<&ToolMeta> {
        self.entry(id).map(|entry| &entry.meta)
    }

    /// The categories in use, in sidebar order.
    pub fn categories(&self) -> Vec<CategoryInfo> {
        let used: BTreeMap<&Category, CategoryInfo> = self
            .metas()
            .map(|meta| (&meta.category, meta.category.info()))
            .collect();
        let mut infos: Vec<CategoryInfo> = used.into_values().collect();
        infos.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        infos
    }

    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        rank(query, self.metas())
    }

    /// Runs a declared action. Panics become `core.panicked`; the panic
    /// payload is never returned, because it may contain input. The host must
    /// also install a panic hook that never prints the payload: the default
    /// hook writes it to stderr before `catch_unwind` recovers.
    ///
    /// In builds with debug assertions (the default for `cargo test`) the
    /// result is also checked against the declared outputs and the tool's
    /// error-code namespace, failing with `core.invalid_output`.
    pub fn run(
        &self,
        id: &str,
        action: &str,
        input: Value,
        env: &RunEnv<'_>,
    ) -> Result<Value, ToolError> {
        let entry = self.entry(id).ok_or_else(|| ToolError::unknown_tool(id))?;
        if entry.meta.action(action).is_none() {
            return Err(ToolError::unknown_action(id, action));
        }
        let ctx = Ctx::new(
            env.cancel,
            env.progress,
            env.services,
            &entry.meta.capabilities,
        );
        let result = catch_unwind(AssertUnwindSafe(|| entry.tool.invoke(action, input, &ctx)))
            .unwrap_or_else(|_| Err(ToolError::panicked()));

        if cfg!(debug_assertions) {
            match &result {
                Ok(value) => check_output(&entry.meta, value)?,
                Err(error) => check_error_code(&entry.meta, &error.code)?,
            }
        }
        result
    }

    fn entry(&self, id: &str) -> Option<&Entry> {
        self.index.get(id).and_then(|&i| self.entries.get(i))
    }
}

/// Builds a one-tool registry and runs `action`, with every debug check the
/// app applies. Use it in tool tests instead of calling `invoke` directly.
///
/// # Panics
/// If the tool's metadata is invalid.
pub fn run_single(tool: impl Tool, action: &str, input: Value) -> Result<Value, ToolError> {
    let registry = match Registry::new(vec![Arc::new(tool)]) {
        Ok(registry) => registry,
        Err(problems) => {
            let list: Vec<String> = problems.iter().map(ToString::to_string).collect();
            panic!("invalid tool metadata:\n{}", list.join("\n"));
        }
    };
    let id = registry
        .metas()
        .next()
        .map(|m| m.id.to_string())
        .unwrap_or_default();
    crate::ctx::with_detached_env(|env| registry.run(&id, action, input, env))
}

fn validate_meta(meta: &ToolMeta) -> Vec<String> {
    let mut problems = Vec::new();
    let mut problem = |p: String| problems.push(p);

    if meta.spec_version != crate::SPEC_VERSION {
        problem(format!(
            "spec_version {} differs from this build's {}",
            meta.spec_version,
            crate::SPEC_VERSION
        ));
    }
    if !ToolId::is_valid(meta.id.as_str()) {
        problem("id must match [a-z][a-z0-9_]* and be at most 64 bytes".to_owned());
    }
    if meta.id.is_extension_namespace() {
        problem("ids containing `__` are reserved for extensions".to_owned());
    }
    if RESERVED_TOOL_IDS.contains(&meta.id.as_str()) {
        problem(format!("id `{}` is reserved for the host", meta.id));
    }
    if meta.name.trim().is_empty() {
        problem("name is empty".to_owned());
    }
    if meta.description.trim().is_empty() {
        problem("description is empty".to_owned());
    }
    if !Category::is_valid(meta.category.as_str()) {
        problem(format!("category `{}` is malformed", meta.category));
    }
    for keyword in &meta.keywords {
        if keyword.trim().is_empty() || *keyword != keyword.to_lowercase() {
            problem(format!("keyword {keyword:?} must be non-empty lower case"));
        }
    }
    if let Err(error) = validate_icon(&meta.icon) {
        problem(format!("icon: {error}"));
    }
    let mut seen_caps = HashSet::new();
    for capability in &meta.capabilities {
        if !Capability::is_valid(capability.as_str()) || !seen_caps.insert(capability.as_str()) {
            problem(format!(
                "capability `{capability}` is malformed or repeated"
            ));
        }
    }

    if meta.actions.is_empty() {
        problem("declares no actions".to_owned());
    }
    let mut actions = HashSet::new();
    for action in &meta.actions {
        if !is_ident(&action.id) || !actions.insert(action.id.as_str()) {
            problem(format!("action `{}` is malformed or repeated", action.id));
        }
        if action.label.trim().is_empty() {
            problem(format!("action `{}` has no label", action.id));
        }
    }
    let destructive = |id: &str| meta.action(id).is_some_and(|a| a.destructive);

    let mut reachable = HashSet::new();
    match &meta.ui {
        UiSpec::Transform(spec) => {
            if spec.modes.is_empty() {
                problem("transform declares no modes".to_owned());
            }
            for mode in &spec.modes {
                if !actions.contains(mode.as_str()) || !reachable.insert(mode.as_str()) {
                    problem(format!(
                        "mode `{mode}` is not a declared action or is repeated"
                    ));
                }
                if spec.live && destructive(mode) {
                    problem(format!(
                        "live transform mode `{mode}` cannot be destructive"
                    ));
                }
            }
            check_options(&spec.options, Some(&spec.modes), &mut problem);
            check_outputs(&spec.outputs, &mut problem);
        }
        UiSpec::Generator(spec) => {
            if !actions.contains(spec.action.as_str()) {
                problem(format!(
                    "generator action `{}` is not declared",
                    spec.action
                ));
            }
            if spec.run_on_open && destructive(&spec.action) {
                problem("a run_on_open generator cannot be destructive".to_owned());
            }
            reachable.insert(spec.action.as_str());
            check_options(&spec.options, None, &mut problem);
            check_outputs(&spec.outputs, &mut problem);
        }
        UiSpec::Custom { view } => {
            if view != meta.id.as_str() {
                problem(format!("custom view `{view}` must equal the tool id"));
            }
            reachable.extend(actions.iter().copied());
        }
        UiSpec::Unsupported => problem("ui spec kind is not supported".to_owned()),
    }
    for action in &actions {
        if !reachable.contains(action) {
            problem(format!("action `{action}` is not reachable from the UI"));
        }
    }
    problems
}

fn check_options(
    options: &[OptionSpec],
    modes: Option<&Vec<String>>,
    problem: &mut impl FnMut(String),
) {
    let mut keys = HashSet::new();
    for option in options {
        let key = option.key.as_str();
        if !is_ident(key) {
            problem(format!("option `{key}` is malformed"));
        } else if RESERVED_OPTION_KEYS.contains(&key) {
            problem(format!("option `{key}` is reserved"));
        } else if !keys.insert(key) {
            problem(format!("option `{key}` is repeated"));
        }
        if option.label.trim().is_empty() {
            problem(format!("option `{key}` has no label"));
        }
        match (modes, option.modes.is_empty()) {
            (None, false) => problem(format!("option `{key}` lists modes but the spec has none")),
            (Some(modes), false) => {
                for mode in &option.modes {
                    if !modes.contains(mode) {
                        problem(format!("option `{key}` names unknown mode `{mode}`"));
                    }
                }
            }
            _ => {}
        }
        match &option.control {
            Control::Toggle { .. } => {}
            Control::Choice { choices, default } => {
                let mut values = HashSet::new();
                if choices.is_empty()
                    || !choices.iter().all(|c| values.insert(c.value.as_str()))
                    || !values.contains(default.as_str())
                {
                    problem(format!(
                        "option `{key}`: choices must be non-empty, unique and contain the default"
                    ));
                }
            }
            Control::Integer { min, max, default } => {
                if !(min <= default && default <= max) {
                    problem(format!("option `{key}`: needs min <= default <= max"));
                }
                if [min, max, default]
                    .iter()
                    .any(|v| v.unsigned_abs() > MAX_SAFE_INTEGER)
                {
                    problem(format!(
                        "option `{key}`: integer bounds must be within ±(2^53-1)"
                    ));
                }
            }
            Control::Text { default, limit } => {
                if default.chars().count() > *limit as usize {
                    problem(format!("option `{key}`: default is longer than its limit"));
                }
            }
            Control::Unsupported => problem(format!("option `{key}`: control kind not supported")),
        }
    }
}

fn check_outputs(outputs: &[OutputSpec], problem: &mut impl FnMut(String)) {
    if outputs.is_empty() {
        problem("declares no outputs".to_owned());
    }
    let mut keys = HashSet::new();
    for output in outputs {
        if !is_ident(&output.key) || !keys.insert(output.key.as_str()) {
            problem(format!("output `{}` is malformed or repeated", output.key));
        }
        if output.label.trim().is_empty() {
            problem(format!("output `{}` has no label", output.key));
        }
        match &output.format {
            OutputKind::Unsupported => {
                problem(format!("output `{}`: format not supported", output.key));
            }
            OutputKind::Code { lang } if lang.trim().is_empty() => {
                problem(format!("output `{}`: code output needs a lang", output.key));
            }
            _ => {}
        }
    }
}

fn declared_outputs(meta: &ToolMeta) -> Option<&[OutputSpec]> {
    match &meta.ui {
        UiSpec::Transform(spec) => Some(&spec.outputs),
        UiSpec::Generator(spec) => Some(&spec.outputs),
        UiSpec::Custom { .. } | UiSpec::Unsupported => None,
    }
}

fn check_output(meta: &ToolMeta, value: &Value) -> Result<(), ToolError> {
    let Some(outputs) = declared_outputs(meta) else {
        return Ok(());
    };
    let Some(object) = value.as_object() else {
        return Err(ToolError::invalid_output(
            "result must be an object keyed by output",
        ));
    };
    if let Some(extra) = object
        .keys()
        .find(|k| !outputs.iter().any(|o| &o.key == *k))
    {
        return Err(ToolError::invalid_output(format!(
            "undeclared output `{extra}`"
        )));
    }
    for output in outputs {
        let Some(item) = object.get(&output.key) else {
            return Err(ToolError::invalid_output(format!(
                "output `{}` is missing (use null for nothing)",
                output.key
            )));
        };
        if !matches_kind(&output.format, item) {
            return Err(ToolError::invalid_output(format!(
                "output `{}` does not match its format",
                output.key
            )));
        }
        if output.format == OutputKind::Diagnostics {
            let diagnostics: Vec<Diagnostic> =
                serde_json::from_value(item.clone()).unwrap_or_default();
            for diagnostic in &diagnostics {
                if diagnostic.code.namespace() != meta.id.as_str()
                    || !ErrorCode::is_valid(diagnostic.code.as_str())
                {
                    return Err(ToolError::invalid_output(format!(
                        "diagnostic code `{}` must be `{}.*`",
                        diagnostic.code, meta.id
                    )));
                }
            }
        }
    }
    Ok(())
}

fn check_error_code(meta: &ToolMeta, code: &ErrorCode) -> Result<(), ToolError> {
    let own = ErrorCode::is_valid(code.as_str()) && code.namespace() == meta.id.as_str();
    if own || TOOL_CORE_CODES.contains(code) {
        Ok(())
    } else {
        Err(ToolError::invalid_output(format!(
            "error code `{code}` must be `{}.*` or one of core.invalid_input, core.cancelled",
            meta.id
        )))
    }
}
