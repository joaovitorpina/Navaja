use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use serde_path_to_error::Segment;

use crate::error::ToolError;
use crate::id::is_ident;

/// Parses a tool's input into `T`. Use `#[serde(deny_unknown_fields)]` on `T`.
///
/// Errors map to `core.invalid_input`. The message is generic and the details
/// carry only the path of the offending field, never the offending value, so
/// a pasted secret can't leak into an error (serde's own messages quote values).
///
/// Input types must not use maps keyed by user content (`HashMap<String, _>`):
/// such keys can't be told apart from schema field names.
pub fn typed<T: DeserializeOwned>(input: Value) -> Result<T, ToolError> {
    serde_path_to_error::deserialize::<_, T>(input).map_err(|error| {
        // For an unknown field the last path segment is the stray key itself,
        // which comes from the input; name only its parent.
        let unknown_field = error.inner().to_string().starts_with("unknown field");
        ToolError::invalid_input("The input does not have the expected shape.")
            .with_details(json!({ "path": safe_path(error.path(), unknown_field) }))
    })
}

/// Renders the path with field names from the tool's own schema; anything
/// that could come from the input (unusual map keys) becomes `*`.
fn safe_path(path: &serde_path_to_error::Path, mask_last: bool) -> String {
    let mut out = String::new();
    let count = path.iter().count();
    for (i, segment) in path.iter().enumerate() {
        let last = i + 1 == count;
        match segment {
            Segment::Seq { index } => out.push_str(&format!("[{index}]")),
            Segment::Map { key } | Segment::Enum { variant: key } => {
                if !out.is_empty() {
                    out.push('.');
                }
                let looks_like_schema = !(mask_last && last) && key.len() <= 32 && is_ident(key);
                out.push_str(if looks_like_schema { key } else { "*" });
            }
            Segment::Unknown => out.push_str(".?"),
        }
    }
    if out.is_empty() { ".".to_owned() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Input {
        count: u32,
        #[serde(default)]
        upper: bool,
    }

    #[test]
    fn parses_valid_input() {
        let input: Input = typed(json!({ "count": 3 })).unwrap();
        assert_eq!(
            input,
            Input {
                count: 3,
                upper: false
            }
        );
    }

    #[test]
    fn errors_never_echo_values() {
        let secret = "eyJhbGciOiJIUzI1NiJ9.SECRET";
        let error = typed::<Input>(json!({ "count": secret })).unwrap_err();
        assert_eq!(error.code.as_str(), "core.invalid_input");
        let rendered = serde_json::to_string(&error).unwrap();
        assert!(!rendered.contains("SECRET"), "{rendered}");
        assert_eq!(error.details.unwrap()["path"], "count");
    }

    #[test]
    fn unknown_keys_are_masked_in_paths() {
        let error =
            typed::<std::collections::HashMap<String, u32>>(json!({ "Bearer abc.def": "x" }))
                .unwrap_err();
        let rendered = serde_json::to_string(&error).unwrap();
        assert!(!rendered.contains("Bearer"), "{rendered}");
    }

    #[test]
    fn unknown_fields_are_rejected_without_naming_them() {
        // Ident-shaped, so only the unknown-field rule keeps it out.
        let error = typed::<Input>(json!({ "count": 1, "hunter2secret": 1 })).unwrap_err();
        let rendered = serde_json::to_string(&error).unwrap();
        assert!(!rendered.contains("hunter2secret"), "{rendered}");
    }
}
