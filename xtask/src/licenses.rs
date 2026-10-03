//! `cargo xtask licenses`: every installed npm package is under a licence
//! that deny.toml allows for crates, or has an exception in js-licenses.toml.
//!
//! Dev dependencies count too: the front end bundles some of them (the
//! svelte runtime), and the rest runs on developer machines and in CI. The
//! listing comes from `pnpm licenses list`, so it covers what is installed
//! for this platform; run `pnpm install --frozen-lockfile` first.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::spdx::{Expr, License};
use crate::util::{report, root};

/// Per-package exceptions, next to deny.toml.
const EXCEPTIONS: &str = "js-licenses.toml";

/// The workspace manifests (pnpm-workspace.yaml). Their direct dependencies
/// must all appear in the listing, which catches pnpm listing less than the
/// whole install.
const MANIFESTS: &[&str] = &["package.json", "app/package.json"];

/// What pnpm reports for a package with no `license` field.
const NO_LICENSE: &str = "Unknown";

pub fn run() -> Result<()> {
    let root = root();
    let allowlist = Allowlist::from_deny_toml(&read(&root.join("deny.toml"))?)?;
    let exceptions = parse_exceptions(&read(&root.join(EXCEPTIONS))?)?;
    let packages = parse_pnpm(&pnpm_licenses(&root)?)?;

    let mut problems = exception_problems(&exceptions, &allowlist);
    for manifest in MANIFESTS {
        let text = read(&root.join(manifest))?;
        let json: Value =
            serde_json::from_str(&text).with_context(|| format!("parsing {manifest}"))?;
        problems.extend(missing_dependencies(manifest, &json, &packages));
    }

    let mut excepted = 0;
    let mut needed = HashSet::new();
    for package in &packages {
        match verdict(package, &allowlist, &exceptions) {
            Verdict::Allowed => {}
            Verdict::Excepted => {
                excepted += 1;
                needed.insert(package.name.as_str());
            }
            Verdict::Rejected(why) => {
                needed.insert(package.name.as_str());
                problems.push(format!("{} ({}): {why}", package.label(), package.license));
            }
        }
    }
    // A stale entry still names one package and one licence, so it lets
    // nothing new in. It only warns: platform-specific packages (native
    // binaries) are installed on some OSes and not others.
    for exception in &exceptions {
        if !needed.contains(exception.package.as_str()) {
            eprintln!(
                "warning: {EXCEPTIONS}: no installed package needs the exception for {} ({})",
                exception.package, exception.license
            );
        }
    }

    if problems.is_empty() {
        let versions: usize = packages.iter().map(|p| p.versions.len()).sum();
        println!(
            "{} npm packages ({versions} versions) checked; {excepted} allowed through {EXCEPTIONS}",
            packages.len()
        );
    } else {
        eprintln!(
            "allowed licences: deny.toml [licenses].allow; per-package exceptions: {EXCEPTIONS}"
        );
    }
    report("xtask licenses", &problems)
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

/// Runs `pnpm licenses list --json --recursive` in the repository root.
/// Without `--recursive`, pnpm looks only at the root package, which has no
/// dependencies, and prints a plain-text "No licenses in packages found".
fn pnpm_licenses(root: &Path) -> Result<String> {
    let args = ["licenses", "list", "--json", "--recursive"];
    let output = run_pnpm(root, &args)?;
    if !output.status.success() {
        bail!(
            "pnpm {} failed ({}): {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("pnpm output is not UTF-8")
}

/// On Windows pnpm is usually a `.cmd` shim (npm, corepack), which Command
/// does not find by its bare name; a standalone `pnpm.exe` is.
fn run_pnpm(root: &Path, args: &[&str]) -> Result<Output> {
    let candidates: &[&str] = if cfg!(windows) {
        &["pnpm.cmd", "pnpm"]
    } else {
        &["pnpm"]
    };
    for program in candidates {
        match Command::new(program).args(args).current_dir(root).output() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            result => return result.with_context(|| format!("running {program}")),
        }
    }
    bail!("pnpm is not on PATH (see packageManager in package.json)")
}

/// One entry of the listing: a package name, the installed versions that
/// share a licence, and that licence as the package declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Package {
    name: String,
    versions: Vec<String>,
    license: String,
}

impl Package {
    /// `name@1.0.0, 2.0.0`.
    fn label(&self) -> String {
        if self.versions.is_empty() {
            self.name.clone()
        } else {
            format!("{}@{}", self.name, self.versions.join(", "))
        }
    }
}

/// `pnpm licenses list --json` prints an object keyed by licence, each
/// holding the packages under it: `{ "MIT": [{ "name", "versions",
/// "license", ... }] }`. Anything else, or an empty listing, is an error,
/// so a pnpm change can't make the check pass on nothing.
fn parse_pnpm(stdout: &str) -> Result<Vec<Package>> {
    let excerpt: String = stdout.trim().chars().take(200).collect();
    let value: Value = serde_json::from_str(stdout)
        .with_context(|| format!("pnpm licenses list printed no JSON: {excerpt:?}"))?;
    let Some(groups) = value.as_object() else {
        bail!("pnpm licenses list: expected an object keyed by licence, got {excerpt:?}");
    };
    let mut packages = Vec::new();
    for (key, entries) in groups {
        let Some(entries) = entries.as_array() else {
            bail!("pnpm licenses list: {key:?} does not hold a list of packages");
        };
        for entry in entries {
            let Some(name) = entry["name"].as_str() else {
                bail!("pnpm licenses list: an entry under {key:?} has no name");
            };
            let versions = entry["versions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            packages.push(Package {
                name: name.to_owned(),
                versions,
                license: entry["license"].as_str().unwrap_or(key).to_owned(),
            });
        }
    }
    if packages.is_empty() {
        bail!("pnpm licenses list reported no packages; run `pnpm install --frozen-lockfile`");
    }
    packages.sort_by(|a, b| (&a.name, &a.versions).cmp(&(&b.name, &b.versions)));
    Ok(packages)
}

/// deny.toml's `[licenses].allow`, each entry a single licence, optionally
/// with an exception.
struct Allowlist(Vec<License>);

impl Allowlist {
    fn from_deny_toml(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Deny {
            licenses: Licenses,
        }
        #[derive(Deserialize)]
        struct Licenses {
            allow: Vec<String>,
        }
        let deny: Deny = toml::from_str(text).context("parsing deny.toml")?;
        let mut allowed = Vec::new();
        for entry in deny.licenses.allow {
            match Expr::parse(&entry) {
                Ok(Expr::License(license)) if !license.or_later => allowed.push(license),
                _ => bail!("deny.toml: [licenses].allow entry {entry:?} is not a single licence"),
            }
        }
        Ok(Self(allowed))
    }

    /// Ids compare case-insensitively and an exception must match exactly,
    /// as in cargo-deny. `X+` is met by an allowed X, since the licensee may
    /// pick that version; a later version on the list is not considered.
    fn allows(&self, license: &License) -> bool {
        self.0.iter().any(|allowed| {
            allowed.id.eq_ignore_ascii_case(&license.id)
                && match (&allowed.exception, &license.exception) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    _ => false,
                }
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Exception {
    package: String,
    /// The licence exactly as pnpm reports it.
    license: String,
    reason: String,
}

fn parse_exceptions(text: &str) -> Result<Vec<Exception>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct File {
        #[serde(default)]
        exception: Vec<Exception>,
    }
    let file: File = toml::from_str(text).with_context(|| format!("parsing {EXCEPTIONS}"))?;
    Ok(file.exception)
}

/// Entries that are malformed, repeated, or not needed for any licence. A
/// package may have one entry per licence, for versions licensed apart.
fn exception_problems(exceptions: &[Exception], allowlist: &Allowlist) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for exception in exceptions {
        let name = &exception.package;
        if !seen.insert((name.as_str(), exception.license.as_str())) {
            problems.push(format!(
                "{EXCEPTIONS}: {name} ({}) is listed more than once",
                exception.license
            ));
        }
        if exception.reason.trim().is_empty() {
            problems.push(format!("{EXCEPTIONS}: {name} needs a reason"));
        }
        if rejection(&exception.license, allowlist).is_none() {
            problems.push(format!(
                "{EXCEPTIONS}: {name}: {} is already allowed by deny.toml; drop the entry",
                exception.license
            ));
        }
    }
    problems
}

/// Direct dependencies of a manifest that the listing does not contain.
/// Workspace and local packages are not in it by design.
fn missing_dependencies(manifest: &str, json: &Value, packages: &[Package]) -> Vec<String> {
    let listed: HashSet<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    let mut problems = Vec::new();
    for section in ["dependencies", "devDependencies"] {
        for (name, spec) in json[section].as_object().into_iter().flatten() {
            let local = spec.as_str().is_some_and(|spec| {
                ["workspace:", "link:", "file:"]
                    .iter()
                    .any(|prefix| spec.starts_with(prefix))
            });
            if !local && !listed.contains(name.as_str()) {
                problems.push(format!(
                    "{manifest}: {section}.{name} is not in `pnpm licenses list`; is it installed?"
                ));
            }
        }
    }
    problems
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Allowed,
    /// Allowed only through its entry in js-licenses.toml.
    Excepted,
    Rejected(String),
}

fn verdict(package: &Package, allowlist: &Allowlist, exceptions: &[Exception]) -> Verdict {
    let Some(why) = rejection(&package.license, allowlist) else {
        return Verdict::Allowed;
    };
    let entries: Vec<&Exception> = exceptions
        .iter()
        .filter(|e| e.package == package.name)
        .collect();
    if entries.iter().any(|e| e.license == package.license) {
        return Verdict::Excepted;
    }
    if entries.is_empty() {
        return Verdict::Rejected(why);
    }
    let excepted: Vec<String> = entries.iter().map(|e| format!("{:?}", e.license)).collect();
    Verdict::Rejected(format!(
        "{why}; {EXCEPTIONS} excepts it only under {}",
        excepted.join(", ")
    ))
}

/// Why a declared licence is not acceptable on its own, or `None` if it is.
fn rejection(license: &str, allowlist: &Allowlist) -> Option<String> {
    let license = license.trim();
    if license.is_empty() || license.eq_ignore_ascii_case(NO_LICENSE) {
        return Some("declares no licence".to_owned());
    }
    match Expr::parse(license) {
        Err(error) => Some(format!("not an SPDX expression: {error}")),
        Ok(expr) if expr.satisfied_by(&|l| allowlist.allows(l)) => None,
        Ok(expr) => {
            let mut outside: Vec<String> = Vec::new();
            for l in expr.licenses() {
                let text = l.to_string();
                if !allowlist.allows(l) && !outside.contains(&text) {
                    outside.push(text);
                }
            }
            Some(format!("not on the allowlist: {}", outside.join(", ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DENY: &str = r#"
        [graph]
        all-features = true

        [licenses]
        confidence-threshold = 0.93
        allow = ["MIT", "Apache-2.0 WITH LLVM-exception", "ISC", "BSD-3-Clause"]
    "#;

    fn allowlist() -> Allowlist {
        Allowlist::from_deny_toml(DENY).unwrap_or_else(|e| panic!("{e:#}"))
    }

    fn package(name: &str, license: &str) -> Package {
        Package {
            name: name.to_owned(),
            versions: vec!["1.0.0".to_owned()],
            license: license.to_owned(),
        }
    }

    fn exception(package: &str, license: &str) -> Exception {
        Exception {
            package: package.to_owned(),
            license: license.to_owned(),
            reason: "test".to_owned(),
        }
    }

    fn judge(name: &str, license: &str, exceptions: &[Exception]) -> Verdict {
        verdict(&package(name, license), &allowlist(), exceptions)
    }

    #[test]
    fn allowlist_mirrors_deny_toml() {
        let list = allowlist();
        let license = |text: &str| match Expr::parse(text) {
            Ok(Expr::License(license)) => license,
            other => panic!("{text}: {other:?}"),
        };
        assert!(list.allows(&license("MIT")));
        assert!(list.allows(&license("mit")));
        assert!(list.allows(&license("MIT+")));
        assert!(list.allows(&license("Apache-2.0 WITH LLVM-exception")));
        assert!(!list.allows(&license("Apache-2.0")));
        assert!(!list.allows(&license("MIT WITH LLVM-exception")));
        assert!(!list.allows(&license("GPL-3.0-only")));
    }

    #[test]
    fn allowlist_entries_are_single_licences() {
        for bad in [
            r#"[licenses]
               allow = ["MIT OR ISC"]"#,
            r#"[licenses]
               allow = ["GPL-2.0+"]"#,
            r#"[licenses]
               allow = ["SEE LICENSE"]"#,
            "[licenses]",
        ] {
            assert!(Allowlist::from_deny_toml(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_repository_allowlist_loads() {
        let text = read(&root().join("deny.toml")).unwrap_or_else(|e| panic!("{e:#}"));
        let list = Allowlist::from_deny_toml(&text).unwrap_or_else(|e| panic!("{e:#}"));
        assert!(rejection("MIT", &list).is_none());
        assert!(rejection("Apache-2.0 OR MIT", &list).is_none());
        assert!(rejection("GPL-3.0-only", &list).is_some());
    }

    #[test]
    fn the_repository_exceptions_are_needed_and_explained() {
        let deny = read(&root().join("deny.toml")).unwrap_or_else(|e| panic!("{e:#}"));
        let list = Allowlist::from_deny_toml(&deny).unwrap_or_else(|e| panic!("{e:#}"));
        let text = read(&root().join(EXCEPTIONS)).unwrap_or_else(|e| panic!("{e:#}"));
        let exceptions = parse_exceptions(&text).unwrap_or_else(|e| panic!("{e:#}"));
        assert!(!exceptions.is_empty());
        assert_eq!(exception_problems(&exceptions, &list), Vec::<String>::new());
    }

    #[test]
    fn exceptions_file_rejects_unknown_fields() {
        let text = r#"
            [[exception]]
            package = "x"
            license = "Unknown"
            reason = "r"
            version = "1.0.0"
        "#;
        assert!(parse_exceptions(text).is_err());
        assert!(parse_exceptions("").is_ok_and(|e| e.is_empty()));
    }

    #[test]
    fn exception_problems_flag_bad_entries() {
        let mut empty_reason = exception("b", "Unknown");
        empty_reason.reason = " ".to_owned();
        let problems = exception_problems(
            &[
                exception("a", "Unknown"),
                // One entry per licence is fine.
                exception("a", "WTFPL"),
                exception("a", "Unknown"),
                empty_reason,
                exception("c", "MIT OR GPL-3.0-only"),
            ],
            &allowlist(),
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("a (Unknown) is listed more than once"));
        assert!(problems[1].contains("b needs a reason"));
        assert!(problems[2].contains("c: MIT OR GPL-3.0-only is already allowed"));
    }

    #[test]
    fn expressions_are_judged_by_spdx_rules() {
        for allowed in [
            "MIT",
            "(MIT OR GPL-3.0-or-later)",
            "MIT AND ISC",
            "(MIT AND BSD-3-Clause) OR GPL-2.0-only",
            "Apache-2.0 WITH LLVM-exception",
        ] {
            assert_eq!(judge("p", allowed, &[]), Verdict::Allowed, "{allowed}");
        }
        assert_eq!(
            judge("p", "(MIT AND Zlib) OR (GPL-3.0-only AND ISC)", &[]),
            Verdict::Rejected("not on the allowlist: Zlib, GPL-3.0-only".to_owned())
        );
        assert_eq!(
            judge("p", "Apache-2.0", &[]),
            Verdict::Rejected("not on the allowlist: Apache-2.0".to_owned())
        );
    }

    #[test]
    fn missing_custom_and_malformed_licences_fail() {
        for (license, why) in [
            ("Unknown", "declares no licence"),
            ("UNKNOWN", "declares no licence"),
            ("", "declares no licence"),
            ("LicenseRef-Proprietary", "not on the allowlist"),
            ("UNLICENSED", "not on the allowlist"),
            ("SEE LICENSE IN LICENSE.md", "not an SPDX expression"),
            ("MIT or ISC", "not an SPDX expression"),
        ] {
            match judge("p", license, &[]) {
                Verdict::Rejected(text) => assert!(text.starts_with(why), "{license}: {text}"),
                other => panic!("{license}: {other:?}"),
            }
        }
    }

    #[test]
    fn exceptions_match_name_and_exact_licence() {
        let exceptions = [
            exception("css-value", "Unknown"),
            exception("argparse", "Python-2.0"),
            exception("odd", "SEE LICENSE IN LICENSE.md"),
        ];
        assert_eq!(
            judge("css-value", "Unknown", &exceptions),
            Verdict::Excepted
        );
        assert_eq!(
            judge("argparse", "Python-2.0", &exceptions),
            Verdict::Excepted
        );
        assert_eq!(
            judge("odd", "SEE LICENSE IN LICENSE.md", &exceptions),
            Verdict::Excepted
        );
        // Another package with the same licence is not covered.
        assert!(matches!(
            judge("other", "Python-2.0", &exceptions),
            Verdict::Rejected(_)
        ));
        // A relicensed package falls out of its exception.
        assert_eq!(
            judge("argparse", "GPL-3.0-only", &exceptions),
            Verdict::Rejected(
                "not on the allowlist: GPL-3.0-only; js-licenses.toml excepts it only under \"Python-2.0\""
                    .to_owned()
            )
        );
        // An exception never takes away an allowed licence.
        assert_eq!(judge("argparse", "MIT", &exceptions), Verdict::Allowed);

        // Versions licensed apart need one entry each.
        let split = [exception("x", "Unknown"), exception("x", "CC-BY-4.0")];
        assert_eq!(judge("x", "Unknown", &split), Verdict::Excepted);
        assert_eq!(judge("x", "CC-BY-4.0", &split), Verdict::Excepted);
        assert_eq!(
            judge("x", "WTFPL", &split),
            Verdict::Rejected(
                "not on the allowlist: WTFPL; js-licenses.toml excepts it only under \"Unknown\", \"CC-BY-4.0\""
                    .to_owned()
            )
        );
    }

    #[test]
    fn parses_the_pnpm_listing() {
        let stdout = r#"{
          "MIT": [
            { "name": "svelte", "versions": ["5.57.1"], "license": "MIT", "paths": [] },
            { "name": "@types/node", "versions": ["20.19.43", "24.19.1"], "license": "MIT" }
          ],
          "Unknown": [ { "name": "css-value", "versions": ["0.0.1"] } ],
          "(MIT OR CC0-1.0)": [ { "name": "type-fest", "versions": ["4.41.0"], "license": "(MIT OR CC0-1.0)" } ]
        }"#;
        let entry = |name: &str, versions: &[&str], license: &str| Package {
            name: name.to_owned(),
            versions: versions.iter().map(|v| (*v).to_owned()).collect(),
            license: license.to_owned(),
        };
        let packages = parse_pnpm(stdout).unwrap_or_else(|e| panic!("{e:#}"));
        assert_eq!(
            packages,
            [
                entry("@types/node", &["20.19.43", "24.19.1"], "MIT"),
                // No `license` field: the licence it is listed under.
                entry("css-value", &["0.0.1"], "Unknown"),
                entry("svelte", &["5.57.1"], "MIT"),
                entry("type-fest", &["4.41.0"], "(MIT OR CC0-1.0)"),
            ]
        );
        assert_eq!(packages[0].label(), "@types/node@20.19.43, 24.19.1");
    }

    #[test]
    fn an_empty_or_foreign_listing_is_an_error() {
        for stdout in [
            // What pnpm prints at the root without --recursive.
            "No licenses in packages found",
            "",
            "{}",
            "[]",
            r#"{ "MIT": {} }"#,
            r#"{ "MIT": [ { "versions": ["1.0.0"] } ] }"#,
        ] {
            assert!(parse_pnpm(stdout).is_err(), "{stdout:?}");
        }
    }

    #[test]
    fn direct_dependencies_must_be_listed() {
        let manifest = json!({
            "dependencies": { "bits-ui": "^2.19.4", "local": "workspace:*" },
            "devDependencies": { "svelte": "^5.57.1", "vite": "~8.3.2" },
            "optionalDependencies": { "fsevents": "^2" }
        });
        let packages = [package("bits-ui", "MIT"), package("svelte", "MIT")];
        assert_eq!(
            missing_dependencies("app/package.json", &manifest, &packages),
            [
                "app/package.json: devDependencies.vite is not in `pnpm licenses list`; is it installed?"
            ]
        );
    }
}
