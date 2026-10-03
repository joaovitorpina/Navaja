//! `cargo xtask tool-gate <base>`: a PR that adds `tools/<id>/` may touch only
//! what docs/architecture.md §4 allows, and adds exactly one registration line
//! per new tool.

use std::collections::BTreeSet;

use anyhow::{Result, bail};

use crate::util::git;

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The diff adds no tool; nothing to check.
    NotAToolPr,
    /// The base has no tool registry yet (the PR that introduces it).
    Bootstrap,
    Text(Vec<String>),
    System(Vec<String>),
}

pub fn run(base: &str) -> Result<()> {
    let range = format!("{base}...HEAD");
    let changed = git(&["diff", "--name-only", "--no-renames", &range])?;
    let tools_at_base = git(&["ls-tree", "--name-only", base, "tools/"])?;
    let crates_at_base = git(&["ls-tree", "--name-only", base, "crates/"])?;
    let lib_diff = git(&["diff", "-U0", &range, "--", "tools/lib.rs"])?;

    match evaluate(
        &changed.lines().collect::<Vec<_>>(),
        &tools_at_base.lines().collect::<Vec<_>>(),
        &crates_at_base.lines().collect::<Vec<_>>(),
        &lib_diff,
    ) {
        Ok(Verdict::NotAToolPr) => println!("tool-gate: no new tool in this diff"),
        Ok(Verdict::Bootstrap) => println!("tool-gate: the base has no tool registry yet; skipped"),
        Ok(Verdict::Text(ids)) => println!("tool-gate: ok (text tool: {})", ids.join(", ")),
        Ok(Verdict::System(ids)) => println!("tool-gate: ok (system tool: {})", ids.join(", ")),
        Err(problems) => {
            for problem in &problems {
                eprintln!("  - {problem}");
            }
            bail!(
                "tool-gate: {} problem(s); see docs/adding-a-tool.md §6",
                problems.len()
            );
        }
    }
    Ok(())
}

/// Second path segment of `tools/<id>/...` when it is a folder.
fn tool_folder(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("tools/")?;
    let (id, tail) = rest.split_once('/')?;
    (!tail.is_empty()).then_some(id)
}

fn crate_folder(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("crates/")?;
    rest.split_once('/').map(|(name, _)| name)
}

pub fn evaluate(
    changed: &[&str],
    tools_at_base: &[&str],
    crates_at_base: &[&str],
    lib_diff: &str,
) -> Result<Verdict, Vec<String>> {
    if !tools_at_base.contains(&"tools/lib.rs") {
        return Ok(Verdict::Bootstrap);
    }
    let existing_tools: BTreeSet<&str> = tools_at_base
        .iter()
        .filter_map(|p| p.strip_prefix("tools/"))
        .collect();
    let existing_crates: BTreeSet<&str> = crates_at_base
        .iter()
        .filter_map(|p| p.strip_prefix("crates/"))
        .collect();

    let new_tools: BTreeSet<&str> = changed
        .iter()
        .filter_map(|p| tool_folder(p))
        .filter(|id| !existing_tools.contains(id))
        .collect();
    if new_tools.is_empty() {
        return Ok(Verdict::NotAToolPr);
    }
    let new_crates: BTreeSet<&str> = changed
        .iter()
        .filter_map(|p| crate_folder(p))
        .filter(|name| !existing_crates.contains(name))
        .collect();

    let mut problems = Vec::new();
    let mut system = false;
    for path in changed {
        let text_ok = tool_folder(path).is_some_and(|id| new_tools.contains(id))
            || matches!(*path, "tools/lib.rs" | "tools/Cargo.toml" | "Cargo.lock");
        let system_ok = crate_folder(path).is_some_and(|name| new_crates.contains(name))
            || path.starts_with("app/src/bindings/");
        if text_ok {
            continue;
        }
        if system_ok {
            system = true;
            continue;
        }
        problems.push(format!("a tool PR must not change {path}"));
    }

    let expected: BTreeSet<String> = new_tools.iter().map(|id| format!("{id},")).collect();
    let mut added = Vec::new();
    for line in lib_diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if let Some(line) = line.strip_prefix('+') {
            added.push(line.trim().to_owned());
        } else if line.starts_with('-') {
            problems.push(format!(
                "tools/lib.rs: a tool PR must not remove or edit lines ({line})"
            ));
        }
    }
    let added_set: BTreeSet<String> = added.iter().cloned().collect();
    if added.len() != added_set.len() || added_set != expected {
        problems.push(format!(
            "tools/lib.rs must gain exactly one line per new tool ({}), found {:?}",
            expected.iter().cloned().collect::<Vec<_>>().join(" "),
            added
        ));
    }

    let ids: Vec<String> = new_tools.iter().map(|s| (*s).to_owned()).collect();
    if !problems.is_empty() {
        Err(problems)
    } else if system {
        Ok(Verdict::System(ids))
    } else {
        Ok(Verdict::Text(ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_TOOLS: &[&str] = &[
        "tools/Cargo.toml",
        "tools/lib.rs",
        "tools/registry_test.rs",
        "tools/uuid",
    ];
    const BASE_CRATES: &[&str] = &["crates/navaja-core"];
    const LIB_ADD_URL: &str = "diff --git a/tools/lib.rs b/tools/lib.rs\n--- a/tools/lib.rs\n+++ b/tools/lib.rs\n@@ -24,0 +25 @@ register_tools! {\n+    url,\n";

    #[test]
    fn bootstrap_pr_is_skipped() {
        let changed = [
            "tools/lib.rs",
            "tools/uuid/mod.rs",
            "crates/navaja-core/src/lib.rs",
        ];
        assert_eq!(
            evaluate(&changed, &[], BASE_CRATES, ""),
            Ok(Verdict::Bootstrap)
        );
    }

    #[test]
    fn not_a_tool_pr() {
        let verdict = evaluate(&["app/src/App.svelte"], BASE_TOOLS, BASE_CRATES, "");
        assert_eq!(verdict, Ok(Verdict::NotAToolPr));
        // Editing an existing tool is not adding one.
        let verdict = evaluate(&["tools/uuid/mod.rs"], BASE_TOOLS, BASE_CRATES, "");
        assert_eq!(verdict, Ok(Verdict::NotAToolPr));
    }

    #[test]
    fn text_tool_with_one_line() {
        let changed = [
            "tools/url/mod.rs",
            "tools/url/icon.svg",
            "tools/url/tests.rs",
            "tools/lib.rs",
            "tools/Cargo.toml",
            "Cargo.lock",
        ];
        let verdict = evaluate(&changed, BASE_TOOLS, BASE_CRATES, LIB_ADD_URL);
        assert_eq!(verdict, Ok(Verdict::Text(vec!["url".into()])));
    }

    #[test]
    fn system_tool_with_new_crate_and_bindings() {
        let changed = [
            "tools/env/mod.rs",
            "tools/env/ui/View.svelte",
            "tools/env/ui/i18n/en.ts",
            "crates/navaja-env/Cargo.toml",
            "crates/navaja-env/src/lib.rs",
            "app/src/bindings/EnvVar.ts",
            "tools/lib.rs",
            "tools/Cargo.toml",
            "Cargo.lock",
        ];
        let lib = LIB_ADD_URL.replace("url,", "env,");
        assert_eq!(
            evaluate(&changed, BASE_TOOLS, BASE_CRATES, &lib),
            Ok(Verdict::System(vec!["env".into()]))
        );
    }

    #[test]
    fn shell_changes_are_rejected() {
        let changed = [
            "tools/url/mod.rs",
            "tools/lib.rs",
            "app/src/shell/Sidebar.svelte",
            "crates/navaja-core/src/meta.rs",
        ];
        let problems = evaluate(&changed, BASE_TOOLS, BASE_CRATES, LIB_ADD_URL).unwrap_err();
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn registration_line_is_checked() {
        let changed = ["tools/url/mod.rs", "tools/lib.rs"];
        // Missing line.
        assert!(evaluate(&changed, BASE_TOOLS, BASE_CRATES, "").is_err());
        // Extra edit.
        let extra = format!("{LIB_ADD_URL}-    uuid,\n+    uuid ,\n");
        assert!(evaluate(&changed, BASE_TOOLS, BASE_CRATES, &extra).is_err());
        // Wrong id.
        let wrong = LIB_ADD_URL.replace("url,", "uri,");
        assert!(evaluate(&changed, BASE_TOOLS, BASE_CRATES, &wrong).is_err());
    }
}
