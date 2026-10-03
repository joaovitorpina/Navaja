//! Navaja's built-in tools, one folder per tool.
//!
//! Adding a tool (docs/adding-a-tool.md): create `tools/<id>/mod.rs` with a
//! `pub(crate) const TOOL` implementing `navaja_core::Tool`, then add one
//! line to `register_tools!` below, keeping the list sorted.
//! `registry_test.rs` fails if a folder is missing from the list.

macro_rules! register_tools {
    ($($id:ident),+ $(,)?) => {
        $(mod $id;)+

        /// Tool folders, in registration order.
        pub const MODULES: &[&str] = &[$(stringify!($id)),+];

        /// One instance of every built-in tool.
        pub fn all() -> Vec<std::sync::Arc<dyn navaja_core::Tool>> {
            vec![$(std::sync::Arc::new(self::$id::TOOL) as std::sync::Arc<dyn navaja_core::Tool>),+]
        }
    };
}

register_tools! {
    uuid,
}

#[cfg(test)]
mod registry_test;
