//! UUID generator: random (v4) or time-ordered (v7) UUIDs, one or many.

use navaja_core::{
    ActionMeta, Category, Choice, Control, Ctx, ErrorCode, GeneratorSpec, OptionSpec, OutputKind,
    OutputSpec, SPEC_VERSION, Tool, ToolError, ToolId, ToolMeta, UiSpec, Value, typed,
};
use serde::Deserialize;
use serde_json::json;

pub(crate) const TOOL: UuidGenerator = UuidGenerator;

pub(crate) struct UuidGenerator;

const MAX_COUNT: u32 = 10_000;
const COUNT_OUT_OF_RANGE: ErrorCode = ErrorCode::from_static("uuid.count_out_of_range");

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Version {
    #[default]
    V4,
    V7,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Format {
    #[default]
    Hyphenated,
    Simple,
    Braced,
    Urn,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(default)]
    version: Version,
    #[serde(default = "one")]
    count: u32,
    #[serde(default)]
    format: Format,
    #[serde(default)]
    uppercase: bool,
}

fn one() -> u32 {
    1
}

impl Tool for UuidGenerator {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            spec_version: SPEC_VERSION,
            id: ToolId::from_static("uuid"),
            name: "UUID generator".into(),
            description: "Generates random (v4) or time-ordered (v7) UUIDs, one or many.".into(),
            category: Category::GENERATORS,
            keywords: ["uuid", "guid", "unique", "identifier", "random", "v4", "v7"]
                .map(String::from)
                .to_vec(),
            icon: include_str!("icon.svg").into(),
            capabilities: vec![],
            actions: vec![ActionMeta::new("generate", "Generate")],
            tray: false,
            ui: UiSpec::Generator(GeneratorSpec {
                action: "generate".into(),
                options: vec![
                    OptionSpec {
                        key: "version".into(),
                        label: "Version".into(),
                        control: Control::Choice {
                            choices: vec![
                                Choice::new("v4", "Random (v4)"),
                                Choice::new("v7", "Time-ordered (v7)"),
                            ],
                            default: "v4".into(),
                        },
                        modes: vec![],
                    },
                    OptionSpec {
                        key: "count".into(),
                        label: "How many".into(),
                        control: Control::Integer {
                            min: 1,
                            max: i64::from(MAX_COUNT),
                            default: 1,
                        },
                        modes: vec![],
                    },
                    OptionSpec {
                        key: "format".into(),
                        label: "Format".into(),
                        control: Control::Choice {
                            choices: vec![
                                Choice::new("hyphenated", "Hyphenated"),
                                Choice::new("simple", "No hyphens"),
                                Choice::new("braced", "Braced"),
                                Choice::new("urn", "URN"),
                            ],
                            default: "hyphenated".into(),
                        },
                        modes: vec![],
                    },
                    OptionSpec {
                        key: "uppercase".into(),
                        label: "Upper case".into(),
                        control: Control::Toggle { default: false },
                        modes: vec![],
                    },
                ],
                outputs: vec![OutputSpec {
                    key: "uuids".into(),
                    label: "UUIDs".into(),
                    format: OutputKind::Text,
                }],
                run_on_open: true,
            }),
        }
    }

    fn invoke(&self, _action: &str, input: Value, ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        let input: Input = typed(input)?;
        if !(1..=MAX_COUNT).contains(&input.count) {
            return Err(
                ToolError::new(COUNT_OUT_OF_RANGE, "Choose between 1 and 10,000 UUIDs.")
                    .with_details(json!({ "min": 1, "max": MAX_COUNT })),
            );
        }
        ctx.check()?;
        Ok(json!({ "uuids": generate(&input, ctx)? }))
    }
}

fn generate(input: &Input, ctx: &Ctx<'_>) -> Result<String, ToolError> {
    let mut buffer = ::uuid::Uuid::encode_buffer();
    let mut out = String::with_capacity(input.count as usize * 46);
    for i in 0..input.count {
        if i % 1024 == 0 {
            ctx.check()?;
        }
        let id = match input.version {
            Version::V4 => ::uuid::Uuid::new_v4(),
            // now_v7 shares one process-wide context, so v7 UUIDs stay strictly
            // increasing across batches and runs, even within a millisecond.
            Version::V7 => ::uuid::Uuid::now_v7(),
        };
        let text = match (input.format, input.uppercase) {
            (Format::Hyphenated, false) => id.hyphenated().encode_lower(&mut buffer),
            (Format::Hyphenated, true) => id.hyphenated().encode_upper(&mut buffer),
            (Format::Simple, false) => id.simple().encode_lower(&mut buffer),
            (Format::Simple, true) => id.simple().encode_upper(&mut buffer),
            (Format::Braced, false) => id.braced().encode_lower(&mut buffer),
            (Format::Braced, true) => id.braced().encode_upper(&mut buffer),
            (Format::Urn, false) => id.urn().encode_lower(&mut buffer),
            (Format::Urn, true) => id.urn().encode_upper(&mut buffer),
        };
        if i > 0 {
            out.push('\n');
        }
        out.push_str(text);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
