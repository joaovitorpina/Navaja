//! Tool metadata and the declarative UI description (`UiSpec`).
//!
//! The generic view renders `Transform` and `Generator` specs, so a text tool
//! needs no front-end code. `Custom` names a view in `tools/<id>/ui/`.

use serde::{Deserialize, Serialize};

use crate::id::{Capability, Category, ToolId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ToolMeta {
    /// `crate::SPEC_VERSION` for built-in tools.
    pub spec_version: u16,
    pub id: ToolId,
    /// English fallbacks; the front end translates `tool.<id>.name` etc.
    pub name: String,
    pub description: String,
    pub category: Category,
    /// Extra search terms, lower case.
    pub keywords: Vec<String>,
    /// SVG markup checked by `crate::validate_icon`.
    pub icon: String,
    pub capabilities: Vec<Capability>,
    pub actions: Vec<ActionMeta>,
    /// Listed in the tray menu.
    pub tray: bool,
    pub ui: UiSpec,
}

impl ToolMeta {
    pub fn action(&self, id: &str) -> Option<&ActionMeta> {
        self.actions.iter().find(|action| action.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ActionMeta {
    pub id: String,
    pub label: String,
    /// The shell asks for confirmation before running it.
    pub destructive: bool,
}

impl ActionMeta {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            destructive: false,
        }
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum UiSpec {
    /// Input on one side, outputs on the other; modes are action ids.
    Transform(TransformSpec),
    /// No input, options only (e.g. UUID).
    Generator(GeneratorSpec),
    /// A hand-written view in `tools/<view>/ui/View.svelte`; `view` equals the tool id.
    Custom { view: String },
    /// A kind from a newer spec. It deserializes so newer metadata still
    /// parses, but the registry rejects any tool that uses it.
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TransformSpec {
    /// Action ids offered as modes; the first is the default.
    pub modes: Vec<String>,
    pub input: InputSpec,
    pub options: Vec<OptionSpec>,
    pub outputs: Vec<OutputSpec>,
    /// Re-run (debounced) as the input changes.
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GeneratorSpec {
    pub action: String,
    pub options: Vec<OptionSpec>,
    pub outputs: Vec<OutputSpec>,
    /// Run once with default options when the tool opens.
    pub run_on_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InputSpec {
    /// Editor language hint, e.g. `json`; `None` for plain text.
    pub lang: Option<String>,
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OptionSpec {
    /// Key in the input object; never `input` or `file`.
    pub key: String,
    pub label: String,
    pub control: Control,
    /// Transform modes this option applies to; empty means all.
    #[serde(default)]
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Control {
    Toggle {
        default: bool,
    },
    Choice {
        choices: Vec<Choice>,
        default: String,
    },
    Integer {
        min: i64,
        max: i64,
        default: i64,
    },
    Text {
        default: String,
        /// Maximum length in characters.
        limit: u32,
    },
    /// A kind from a newer spec; the registry rejects tools that use it.
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl Choice {
    pub fn new(value: &str, label: &str) -> Self {
        Self {
            value: value.to_owned(),
            label: label.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OutputSpec {
    /// Key in the result object.
    pub key: String,
    pub label: String,
    pub format: OutputKind,
}

/// How an output value is shaped and rendered. See `crate::payload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum OutputKind {
    /// A string.
    Text,
    /// A string shown in the code view; `lang` is an open hint such as `json`.
    Code { lang: String },
    /// `Vec<KeyValueRow>`.
    KeyValue,
    /// `Vec<Diagnostic>`.
    Diagnostics,
    /// `BinaryValue`.
    Binary,
    /// A kind from a newer spec; the registry rejects tools that use it.
    #[serde(other)]
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ui_spec_wire_format() {
        let spec = UiSpec::Generator(GeneratorSpec {
            action: "generate".into(),
            options: vec![OptionSpec {
                key: "count".into(),
                label: "Count".into(),
                control: Control::Integer {
                    min: 1,
                    max: 10,
                    default: 1,
                },
                modes: vec![],
            }],
            outputs: vec![OutputSpec {
                key: "out".into(),
                label: "Out".into(),
                format: OutputKind::Code {
                    lang: "json".into(),
                },
            }],
            run_on_open: true,
        });
        assert_eq!(
            serde_json::to_value(&spec).unwrap(),
            json!({
                "kind": "generator",
                "action": "generate",
                "options": [{
                    "key": "count", "label": "Count",
                    "control": { "kind": "integer", "min": 1, "max": 10, "default": 1 },
                    "modes": []
                }],
                "outputs": [{ "key": "out", "label": "Out", "format": { "kind": "code", "lang": "json" } }],
                "runOnOpen": true
            })
        );
    }

    #[test]
    fn unknown_variants_from_newer_specs_deserialize_as_unsupported() {
        let ui: UiSpec = serde_json::from_value(json!({ "kind": "hologram" })).unwrap();
        assert_eq!(ui, UiSpec::Unsupported);
        let kind: OutputKind = serde_json::from_value(json!({ "kind": "chart" })).unwrap();
        assert_eq!(kind, OutputKind::Unsupported);
        let control: Control = serde_json::from_value(json!({ "kind": "slider" })).unwrap();
        assert_eq!(control, Control::Unsupported);
    }
}
