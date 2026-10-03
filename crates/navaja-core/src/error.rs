use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::ErrorCode;

/// A tool's failure. `code` is stable and translated by the front end;
/// `message` is an English fallback and never echoes the input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ToolError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<Value>,
}

impl ToolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn unknown_tool(id: &str) -> Self {
        Self::new(ErrorCode::UNKNOWN_TOOL, "No such tool.")
            .with_details(serde_json::json!({ "tool": id }))
    }

    pub fn unknown_action(tool: &str, action: &str) -> Self {
        Self::new(ErrorCode::UNKNOWN_ACTION, "This tool has no such action.")
            .with_details(serde_json::json!({ "tool": tool, "action": action }))
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::INVALID_INPUT, message)
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorCode::CANCELLED, "Cancelled.")
    }

    pub fn panicked() -> Self {
        Self::new(ErrorCode::PANICKED, "The tool failed unexpectedly.")
    }

    pub fn invalid_output(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::INVALID_OUTPUT, reason)
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ToolError {}
