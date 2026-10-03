//! Tool interface, registry and shared types for Navaja.
//!
//! See `docs/architecture.md` §3. This crate never depends on a UI toolkit,
//! an async runtime or anything that can open a network connection.

mod ctx;
mod error;
mod icon;
mod id;
mod input;
mod meta;
mod payload;
mod registry;
mod search;
mod tool;

pub use ctx::{
    Ctx, DuplicateService, Progress, RunEnv, Services, with_detached_ctx, with_detached_env,
};
pub use error::ToolError;
pub use icon::{IconError, validate_icon};
pub use id::{Capability, Category, CategoryInfo, ErrorCode, ToolId};
pub use input::typed;
pub use meta::{
    ActionMeta, Choice, Control, GeneratorSpec, InputSpec, OptionSpec, OutputKind, OutputSpec,
    ToolMeta, TransformSpec, UiSpec,
};
pub use payload::{BinaryValue, Diagnostic, KeyValueRow, Severity};
pub use registry::{Registry, RegistryError, run_single};
pub use search::{SearchHit, rank};
pub use serde_json::Value;
pub use tool::Tool;

/// Version of the tool specification (`ToolMeta`, `UiSpec`) this build
/// implements. A tool declares the version it was written against; built-in
/// tools must match exactly. Compatibility rules for extensions written
/// against other versions are defined with the extension API
/// (docs/architecture.md §9).
pub const SPEC_VERSION: u16 = 1;

#[cfg(test)]
mod tests;
