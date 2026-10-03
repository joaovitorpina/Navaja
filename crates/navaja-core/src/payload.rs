//! Output payload types, one per `OutputKind`:
//!
//! | `OutputKind`  | JSON value              |
//! |---------------|-------------------------|
//! | `Text`        | string                  |
//! | `Code`        | string                  |
//! | `KeyValue`    | `Vec<KeyValueRow>`      |
//! | `Diagnostics` | `Vec<Diagnostic>`       |
//! | `Binary`      | `BinaryValue`           |
//!
//! Any declared output may also be `null` (nothing to show for this run).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::ErrorCode;
use crate::meta::OutputKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct KeyValueRow {
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub note: Option<String>,
    /// Masked in the UI until revealed; copied through the sensitive path.
    /// Always explicit, so a forgotten flag can't silently unmask a value.
    pub secret: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: ErrorCode,
    /// English fallback; must not echo the input.
    pub message: String,
    /// 1-based.
    pub line: Option<u32>,
    /// 1-based, in UTF-16 code units (what the editor counts).
    pub column: Option<u32>,
    pub details: Option<Value>,
}

/// Bytes that are not valid UTF-8, shown as hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BinaryValue {
    pub len: u64,
    pub hex: String,
}

impl BinaryValue {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            hex.push(char::from(HEX[usize::from(byte >> 4)]));
            hex.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self {
            len: bytes.len() as u64,
            hex,
        }
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Checks that `value` has exactly the JSON shape `kind` promises: it must
/// survive a round trip through the payload type unchanged, so missing,
/// extra or mistyped fields are all caught. Only used with debug assertions.
pub(crate) fn matches_kind(kind: &OutputKind, value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    match kind {
        OutputKind::Text | OutputKind::Code { .. } => value.is_string(),
        OutputKind::KeyValue => is_list_of::<KeyValueRow>(value),
        OutputKind::Diagnostics => is_list_of::<Diagnostic>(value),
        OutputKind::Binary => round_trips::<BinaryValue>(value).is_some_and(|b| {
            b.hex.len() as u64 == 2 * b.len
                && b.hex
                    .bytes()
                    .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
        }),
        OutputKind::Unsupported => false,
    }
}

fn round_trips<T: DeserializeOwned + Serialize>(value: &Value) -> Option<T> {
    let parsed: T = serde_json::from_value(value.clone()).ok()?;
    (serde_json::to_value(&parsed).ok().as_ref() == Some(value)).then_some(parsed)
}

fn is_list_of<T: DeserializeOwned + Serialize>(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().all(|item| round_trips::<T>(item).is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn binary_hex() {
        let value = BinaryValue::from_bytes(&[0x00, 0x7f, 0xff]);
        assert_eq!(value.len, 3);
        assert_eq!(value.hex, "007fff");
    }

    #[test]
    fn shapes() {
        assert!(matches_kind(&OutputKind::Text, &json!("x")));
        assert!(matches_kind(&OutputKind::Text, &Value::Null));
        assert!(!matches_kind(&OutputKind::Text, &json!(1)));
        assert!(matches_kind(
            &OutputKind::KeyValue,
            &json!([{ "key": "exp", "value": "1", "secret": false }])
        ));
        assert!(!matches_kind(
            &OutputKind::KeyValue,
            &json!([{ "key": "exp" }])
        ));
        assert!(matches_kind(
            &OutputKind::Diagnostics,
            &json!([{ "severity": "error", "code": "json.syntax", "message": "m",
                      "line": 1, "column": 2, "details": null }])
        ));
        assert!(matches_kind(
            &OutputKind::Binary,
            &json!({ "len": 1, "hex": "00" })
        ));
        assert!(!matches_kind(&OutputKind::Unsupported, &json!("x")));
    }

    #[test]
    fn shapes_are_exact() {
        // A forgotten or misspelled secret flag must not pass as `secret: false`.
        for row in [
            json!({ "key": "token", "value": "s3cr3t" }),
            json!({ "key": "token", "value": "s3cr3t", "isSecret": true }),
            json!({ "key": "token", "value": "s3cr3t", "secret": false, "isSecret": true }),
        ] {
            assert!(!matches_kind(&OutputKind::KeyValue, &json!([row])), "{row}");
        }
        // Optional diagnostic fields are present as null, never missing.
        assert!(!matches_kind(
            &OutputKind::Diagnostics,
            &json!([{ "severity": "error", "code": "json.syntax", "message": "m" }])
        ));
        // Binary hex must be lower-case and match the length.
        for bad in [
            json!({ "len": 2, "hex": "00" }),
            json!({ "len": 1, "hex": "0G" }),
            json!({ "len": 1, "hex": "FF" }),
            json!({ "len": 1, "hex": "ff", "extra": 1 }),
        ] {
            assert!(!matches_kind(&OutputKind::Binary, &bad), "{bad}");
        }
    }
}
