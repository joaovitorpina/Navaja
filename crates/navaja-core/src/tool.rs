use serde_json::Value;

use crate::ctx::Ctx;
use crate::error::ToolError;
use crate::meta::ToolMeta;

/// A Navaja tool: pure, synchronous logic behind a JSON interface.
///
/// Rules (docs/architecture.md §1, §3):
/// - Never print, prompt, start a runtime, open a connection or take a file path.
/// - Parse input with [`crate::typed`] and call [`Ctx::check`] before any side effect.
/// - Return outputs keyed as declared in `meta().ui`; errors carry stable codes
///   (`<id>.<snake_case>`) and messages that never echo the input.
pub trait Tool: Send + Sync + 'static {
    /// Called once at registration; the result is cached.
    fn meta(&self) -> ToolMeta;

    /// Runs `action`. The registry only calls declared actions.
    fn invoke(&self, action: &str, input: Value, ctx: &Ctx<'_>) -> Result<Value, ToolError>;

    /// How likely `sample` (for example clipboard text) is meant for this tool,
    /// 0-100. Reserved for smart detection and extensions; unused in v1.
    fn detect(&self, _sample: &str) -> Option<u8> {
        None
    }
}
