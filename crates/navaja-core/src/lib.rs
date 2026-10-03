//! Tool interface, registry and shared types for Navaja.
//!
//! See `docs/architecture.md` §3. This crate never depends on a UI toolkit,
//! an async runtime or anything that can open a network connection.

/// Version of the tool specification (`ToolMeta`) this build understands.
pub const SPEC_VERSION: u16 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_version_starts_at_one() {
        assert_eq!(SPEC_VERSION, 1);
    }
}
