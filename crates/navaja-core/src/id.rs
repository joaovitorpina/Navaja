//! Identifier newtypes. All are plain strings on the wire.

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

/// `[a-z][a-z0-9_]*`: the grammar shared by tool ids, action ids, option keys
/// and the parts of error codes and capabilities.
pub(crate) fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

/// Stable tool identifier, equal to the tool's folder name.
///
/// Grammar: `[a-z][a-z0-9_]*`, at most 64 bytes. Ids containing `__` are
/// reserved for runtime extensions (`<publisher>__<name>`), and `core` is
/// reserved for the host's error codes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ToolId(Cow<'static, str>);

impl ToolId {
    pub const MAX_LEN: usize = 64;

    pub const fn from_static(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(id: &str) -> bool {
        id.len() <= Self::MAX_LEN && is_ident(id)
    }

    /// `<publisher>__<name>` ids belong to runtime extensions, never built-ins.
    pub fn is_extension_namespace(&self) -> bool {
        self.0.contains("__")
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for ToolId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

// Hash and Eq delegate to the inner string, so maps keyed by ToolId can be
// queried with a plain &str.
impl std::borrow::Borrow<str> for ToolId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Stable error code: `<namespace>.<name>`, where the namespace is `core` or
/// the tool id. The front end translates codes; messages are English fallbacks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ErrorCode(Cow<'static, str>);

impl ErrorCode {
    pub const UNKNOWN_TOOL: Self = Self::from_static("core.unknown_tool");
    pub const UNKNOWN_ACTION: Self = Self::from_static("core.unknown_action");
    pub const INVALID_INPUT: Self = Self::from_static("core.invalid_input");
    pub const INVALID_OUTPUT: Self = Self::from_static("core.invalid_output");
    pub const CANCELLED: Self = Self::from_static("core.cancelled");
    pub const PANICKED: Self = Self::from_static("core.panicked");

    pub const fn from_static(code: &'static str) -> Self {
        Self(Cow::Borrowed(code))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The part before the dot: `core` or a tool id.
    pub fn namespace(&self) -> &str {
        self.0.split_once('.').map_or("", |(ns, _)| ns)
    }

    pub fn is_valid(code: &str) -> bool {
        code.split_once('.')
            .is_some_and(|(ns, name)| is_ident(ns) && is_ident(name))
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Something a tool needs from the host beyond pure computation, such as
/// inspecting processes. Open set: `<area>.<verb>`, e.g. `process.inspect`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Capability(Cow<'static, str>);

impl Capability {
    pub const PROCESS_INSPECT: Self = Self::from_static("process.inspect");
    pub const PROCESS_KILL: Self = Self::from_static("process.kill");
    pub const CONTAINER_ENGINE: Self = Self::from_static("container.engine");

    pub const fn from_static(capability: &'static str) -> Self {
        Self(Cow::Borrowed(capability))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(capability: &str) -> bool {
        let mut parts = capability.split('.');
        let first_ok = parts.next().is_some_and(is_ident);
        let rest: Vec<&str> = parts.collect();
        first_ok && !rest.is_empty() && rest.iter().all(|part| is_ident(part))
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Navigation group, by id. Open set: a tool (or a later extension) may name
/// a category the host doesn't know. Presentation (label, sidebar position)
/// belongs to the host, not to tools: see [`CategoryInfo`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Category(Cow<'static, str>);

/// Built-in categories: id, English label, sidebar position.
const KNOWN_CATEGORIES: &[(&str, &str, u16)] = &[
    ("encoders", "Encoders", 10),
    ("formatters", "Formatters", 20),
    ("generators", "Generators", 30),
    ("system", "System", 40),
];

impl Category {
    pub const ENCODERS: Self = Self::from_static("encoders");
    pub const FORMATTERS: Self = Self::from_static("formatters");
    pub const GENERATORS: Self = Self::from_static("generators");
    pub const SYSTEM: Self = Self::from_static("system");

    pub const fn from_static(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(id: &str) -> bool {
        is_ident(id)
    }

    /// Host presentation: the built-in label and position, or the id itself
    /// after every built-in category for ids the host doesn't know.
    pub fn info(&self) -> CategoryInfo {
        let known = KNOWN_CATEGORIES
            .iter()
            .find(|(id, _, _)| *id == self.as_str());
        CategoryInfo {
            id: self.clone(),
            label: known.map_or_else(|| self.0.to_string(), |(_, label, _)| (*label).to_owned()),
            order: known.map_or(u16::MAX, |(_, _, order)| *order),
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How the shell presents a category: sent alongside the tool list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CategoryInfo {
    pub id: Category,
    /// English fallback; the front end translates `category.<id>`.
    pub label: String,
    /// Sidebar position, ascending; unknown categories come last, by id.
    pub order: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idents() {
        for ok in ["a", "uuid", "base64", "port_inspector", "a1_b2"] {
            assert!(is_ident(ok), "{ok}");
        }
        for bad in ["", "1abc", "_a", "Abc", "a-b", "a.b", "a b", "ä"] {
            assert!(!is_ident(bad), "{bad}");
        }
    }

    #[test]
    fn tool_ids() {
        assert!(ToolId::is_valid("uuid"));
        assert!(!ToolId::is_valid(&"a".repeat(65)));
        assert!(ToolId::from_static("acme__thing").is_extension_namespace());
        assert!(!ToolId::from_static("uuid").is_extension_namespace());
    }

    #[test]
    fn error_codes() {
        assert!(ErrorCode::is_valid("core.cancelled"));
        assert!(ErrorCode::is_valid("uuid.count_out_of_range"));
        for bad in ["core", "core.", ".x", "Core.x", "core.x.y", "core.x-y"] {
            assert!(!ErrorCode::is_valid(bad), "{bad}");
        }
        assert_eq!(ErrorCode::CANCELLED.namespace(), "core");
        for code in [
            ErrorCode::UNKNOWN_TOOL,
            ErrorCode::UNKNOWN_ACTION,
            ErrorCode::INVALID_INPUT,
            ErrorCode::INVALID_OUTPUT,
            ErrorCode::CANCELLED,
            ErrorCode::PANICKED,
        ] {
            assert!(ErrorCode::is_valid(code.as_str()), "{code}");
        }
    }

    #[test]
    fn capabilities() {
        assert!(Capability::is_valid("process.inspect"));
        assert!(Capability::is_valid("container.engine"));
        for bad in ["process", "process.", "Process.x", "a..b"] {
            assert!(!Capability::is_valid(bad), "{bad}");
        }
    }

    #[test]
    fn ids_serialize_as_plain_strings() {
        let json = serde_json::to_string(&ToolId::from_static("uuid")).unwrap();
        assert_eq!(json, "\"uuid\"");
        let back: ErrorCode = serde_json::from_str("\"core.cancelled\"").unwrap();
        assert_eq!(back, ErrorCode::CANCELLED);
    }
}
