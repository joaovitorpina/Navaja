//! Command-line arguments, also forwarded from a second launch by the
//! single-instance plugin. Arguments only show, hide or navigate: nothing on
//! the command line ever runs a tool action.

use std::ffi::OsString;

use navaja_core::ToolId;

/// Why a start as root stops (see [`root_refusal`]).
const ROOT_REFUSAL: &str = "Navaja does not need root and will not run as root. \
                            Start it as your own user, or pass --allow-root.";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Args {
    /// Show the window if hidden, hide it if shown and focused.
    pub toggle: bool,
    /// Show and focus the window.
    pub show: bool,
    /// Open this tool (the id is checked against the registry by the caller).
    pub tool: Option<String>,
    /// Unix: allow running as root (refused by default).
    pub allow_root: bool,
}

/// Parses `argv` (including the program name). Unknown arguments are ignored:
/// desktop launchers add their own (macOS `-psn_…`, file paths). So are
/// arguments that are not valid Unicode, which are never Navaja's own.
pub fn parse<I>(argv: I) -> Args
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let mut args = Args::default();
    // `None` stands for a non-Unicode argument; it still takes its place, so
    // it can't shift `--tool`'s value onto the next argument.
    let mut iter = argv
        .into_iter()
        .skip(1)
        .map(|arg| arg.into().into_string().ok())
        .peekable();
    while let Some(arg) = iter.next() {
        match arg.as_deref() {
            Some("--toggle") => args.toggle = true,
            Some("--show") => args.show = true,
            Some("--allow-root") => args.allow_root = true,
            // A tool id never starts with '-', so a flag after `--tool` stays
            // a flag and `--tool` has no value.
            Some("--tool") => {
                args.tool = iter
                    .next_if(|next| !next.as_deref().is_some_and(|text| text.starts_with('-')))
                    .flatten()
                    .filter(|id| ToolId::is_valid(id));
            }
            Some(other) => {
                if let Some(id) = other.strip_prefix("--tool=") {
                    args.tool = Some(id.to_owned()).filter(|id| ToolId::is_valid(id));
                }
            }
            None => {}
        }
    }
    args
}

/// The message that stops a start as root, unless `--allow-root` is given.
/// A GUI run as root breaks on Wayland and leaves root-owned files in the
/// user's config. Navaja never needs it.
pub fn root_refusal(args: &Args, is_root: bool) -> Option<&'static str> {
    (is_root && !args.allow_root).then_some(ROOT_REFUSAL)
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

    /// An argument that is not valid Unicode on this OS.
    fn not_unicode() -> OsString {
        #[cfg(unix)]
        let arg = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![b'-', b'-', 0x80])
        };
        #[cfg(windows)]
        let arg = {
            use std::os::windows::ffi::OsStringExt;
            // A lone surrogate.
            OsString::from_wide(&[u16::from(b'-'), u16::from(b'-'), 0xD800])
        };
        assert!(arg.to_str().is_none());
        arg
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
    fn tool_never_takes_a_flag_as_its_value() {
        let args = parse(argv(&["--tool", "--toggle"]));
        assert!(args.toggle);
        assert_eq!(args.tool, None);
    }

    #[test]
    fn unknown_arguments_are_ignored() {
        let args = parse(argv(&["-psn_0_12345", "/some/file", "--tool", "uuid"]));
        assert_eq!(args.tool.as_deref(), Some("uuid"));
    }

    #[test]
    fn non_unicode_arguments_are_ignored() {
        let args = parse([
            OsString::from("navaja"),
            not_unicode(),
            OsString::from("--toggle"),
            OsString::from("--tool"),
            not_unicode(),
            OsString::from("--show"),
        ]);
        assert!(args.toggle && args.show);
        assert_eq!(args.tool, None);

        let args = parse([
            OsString::from("navaja"),
            not_unicode(),
            OsString::from("--tool"),
            OsString::from("uuid"),
        ]);
        assert_eq!(args.tool.as_deref(), Some("uuid"));
    }

    #[test]
    fn root_is_refused_unless_allowed() {
        let allowed = parse(argv(&["--allow-root"]));
        assert_eq!(root_refusal(&Args::default(), false), None);
        assert_eq!(root_refusal(&allowed, false), None);
        assert_eq!(root_refusal(&allowed, true), None);

        let message = root_refusal(&Args::default(), true).unwrap();
        assert!(message.contains("--allow-root"), "{message}");
        assert!(!message.contains("  "), "{message:?}");
    }
}
