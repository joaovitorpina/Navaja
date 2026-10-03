//! Command-line arguments, also forwarded from a second launch by the
//! single-instance plugin. Arguments only show, hide or navigate: nothing on
//! the command line ever runs a tool action.

use navaja_core::ToolId;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Args {
    /// Show the window if hidden, hide it if shown and focused.
    pub toggle: bool,
    /// Show and focus the window.
    pub show: bool,
    /// Open this tool (the id is checked against the registry by the caller).
    pub tool: Option<String>,
    /// Linux: allow running as root (refused by default).
    pub allow_root: bool,
}

/// Parses `argv` (including the program name). Unknown arguments are ignored:
/// desktop launchers add their own (macOS `-psn_…`, file paths).
pub fn parse<I: IntoIterator<Item = String>>(argv: I) -> Args {
    let mut args = Args::default();
    let mut iter = argv.into_iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--toggle" => args.toggle = true,
            "--show" => args.show = true,
            "--allow-root" => args.allow_root = true,
            "--tool" => args.tool = iter.next().filter(|id| ToolId::is_valid(id)),
            other => {
                if let Some(id) = other.strip_prefix("--tool=") {
                    args.tool = Some(id.to_owned()).filter(|id| ToolId::is_valid(id));
                }
            }
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(list: &[&str]) -> Vec<String> {
        std::iter::once("navaja")
            .chain(list.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn flags() {
        assert_eq!(parse(argv(&[])), Args::default());
        let args = parse(argv(&["--toggle", "--show", "--allow-root"]));
        assert!(args.toggle && args.show && args.allow_root);
    }

    #[test]
    fn tool_in_both_forms() {
        assert_eq!(
            parse(argv(&["--tool", "uuid"])).tool.as_deref(),
            Some("uuid")
        );
        assert_eq!(parse(argv(&["--tool=json"])).tool.as_deref(), Some("json"));
    }

    #[test]
    fn invalid_or_missing_tool_ids_are_dropped() {
        for list in [
            &["--tool"][..],
            &["--tool", "../x"],
            &["--tool", "UUID"],
            &["--tool=a b"],
            &["--tool", "acme__thing'); alert(1); ('"],
        ] {
            assert_eq!(parse(argv(list)).tool, None, "{list:?}");
        }
    }

    #[test]
    fn unknown_arguments_are_ignored() {
        let args = parse(argv(&["-psn_0_12345", "/some/file", "--tool", "uuid"]));
        assert_eq!(args.tool.as_deref(), Some("uuid"));
    }
}
