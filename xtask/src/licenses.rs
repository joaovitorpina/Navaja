//! `cargo xtask licenses`: every installed npm package is under a licence
//! that deny.toml allows for crates, or has an exception in js-licenses.toml.
//!
//! Dev dependencies count too: the front end bundles some of them (the
//! svelte runtime), and the rest runs on developer machines and in CI. The
//! listing comes from `pnpm licenses list`, so it covers what is installed
//! for this platform; run `pnpm install --frozen-lockfile` first.
//!
//! The licence itself is read from each installed package.json, not taken
//! from pnpm: when a manifest declares none, or says `SEE LICENSE IN
//! <file>`, pnpm reports whatever licence names it finds in the LICENSE
//! file's text, so a proprietary licence that mentions MIT comes out as
//! "MIT".

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
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

/// The licence recorded for a package whose package.json declares none: no
/// `license` or `licenses`, or an empty one. Its entry in js-licenses.toml
/// names this, and its reason says what the LICENSE file actually grants.
/// pnpm uses the same word when it finds no licence at all.
const NO_LICENSE: &str = "Unknown";

pub fn run() -> Result<()> {
    let root = root();
    let allowlist = Allowlist::from_deny_toml(&read(&root.join("deny.toml"))?)?;
    let exceptions = parse_exceptions(&read(&root.join(EXCEPTIONS))?)?;
    let packages = read_manifests(&parse_pnpm(&pnpm_licenses(&root)?)?)?;

    let mut problems = exception_problems(&exceptions, &allowlist);
    for manifest in MANIFESTS {
        let text = read(&root.join(manifest))?;
        let json: Value =
            serde_json::from_str(&text).with_context(|| format!("parsing {manifest}"))?;
        problems.extend(missing_dependencies(manifest, &json, &packages));
    }

    let judgement = check_packages(&packages, &allowlist, &exceptions);
    problems.extend(judgement.problems);
    // A stale entry still names one package and one licence, so it lets
    // nothing new in. It only warns: platform-specific packages (native
    // binaries) are installed on some OSes and not others.
    for exception in judgement.unused {
        eprintln!(
            "warning: {EXCEPTIONS}: no installed version of {} declares {:?}; the entry is unused here",
            exception.package, exception.license
        );
    }

    if problems.is_empty() {
        let versions: usize = packages.iter().map(|p| p.versions.len()).sum();
        println!(
            "{} npm packages ({versions} versions) checked; {} allowed through {EXCEPTIONS}",
            packages.len(),
            judgement.excepted
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
            "pnpm {} failed ({}): {}; run `pnpm install --frozen-lockfile` first",
            args.join(" "),
            output.status,
            pnpm_error(&output.stdout, &output.stderr)
        );
    }
    String::from_utf8(output.stdout).context("pnpm output is not UTF-8")
}

/// What a failed pnpm run said. Under `--json`, pnpm 11 prints its error
/// as JSON on stdout, `{ "error": { "code", "message" } }`, and leaves
/// stderr empty.
fn pnpm_error(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    if !stderr.trim().is_empty() {
        return stderr.trim().to_owned();
    }
    let stdout = String::from_utf8_lossy(stdout);
    let stdout = stdout.trim();
    if let Ok(value) = serde_json::from_str::<Value>(stdout) {
        let error = &value["error"];
        match (error["code"].as_str(), error["message"].as_str()) {
            (Some(code), Some(message)) => return format!("{code}: {message}"),
            (Some(text), None) | (None, Some(text)) => return text.to_owned(),
            (None, None) => {}
        }
    }
    if stdout.is_empty() {
        return "no output".to_owned();
    }
    let excerpt: String = stdout.chars().take(300).collect();
    if excerpt.len() < stdout.len() {
        format!("{excerpt}...")
    } else {
        excerpt
    }
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

/// One entry of pnpm's listing: a package name, its installed versions,
/// and the folders they are installed in. pnpm's licence for it is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Listed {
    name: String,
    versions: Vec<String>,
    paths: Vec<PathBuf>,
}

/// An installed package under one licence: its name, the versions whose
/// package.json declares that licence, and the licence as declared, or
/// NO_LICENSE.
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
/// holding the packages under it: `{ "MIT": [{ "name", "versions", "paths",
/// "license", ... }] }`. Anything else, or an empty listing, is an error,
/// so a pnpm change can't make the check pass on nothing. An entry without
/// paths is an error too: its package.json could not be read.
fn parse_pnpm(stdout: &str) -> Result<Vec<Listed>> {
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
            let strings = |field: &str| {
                entry[field]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            };
            let paths: Vec<PathBuf> = strings("paths").into_iter().map(PathBuf::from).collect();
            if paths.is_empty() {
                bail!("pnpm licenses list: {name} has no paths, so its package.json can't be read");
            }
            packages.push(Listed {
                name: name.to_owned(),
                versions: strings("versions"),
                paths,
            });
        }
    }
    if packages.is_empty() {
        bail!("pnpm licenses list reported no packages; run `pnpm install --frozen-lockfile`");
    }
    packages.sort_by(|a, b| (&a.name, &a.versions).cmp(&(&b.name, &b.versions)));
    Ok(packages)
}

/// Reads the package.json in every folder pnpm lists and groups the
/// versions by the licence they declare. Every version pnpm lists must turn
/// up in one of them, so no version goes unread.
fn read_manifests(listed: &[Listed]) -> Result<Vec<Package>> {
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for entry in listed {
        let mut found: Vec<(String, String)> = Vec::new();
        for dir in &entry.paths {
            let path = dir.join("package.json");
            let text = std::fs::read_to_string(&path).with_context(|| {
                format!(
                    "reading {}; is it installed? run `pnpm install --frozen-lockfile`",
                    path.display()
                )
            })?;
            let manifest: Value = serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()))?;
            if manifest["name"].as_str() != Some(entry.name.as_str()) {
                bail!("{} does not belong to {}", path.display(), entry.name);
            }
            let Some(version) = manifest["version"].as_str() else {
                bail!("{} has no version", path.display());
            };
            let license = declared_license(&manifest).unwrap_or_else(|| NO_LICENSE.to_owned());
            let pair = (version.to_owned(), license);
            if !found.contains(&pair) {
                found.push(pair);
            }
        }
        for version in &entry.versions {
            if !found.iter().any(|(v, _)| v == version) {
                bail!(
                    "pnpm lists {}@{version}, but no package.json in its paths has that version",
                    entry.name
                );
            }
        }
        // In pnpm's order, which sorts by semver.
        found.sort_by_key(|(v, _)| entry.versions.iter().position(|listed| listed == v));
        for (version, license) in found {
            let versions = groups.entry((entry.name.clone(), license)).or_default();
            if !versions.contains(&version) {
                versions.push(version);
            }
        }
    }
    Ok(groups
        .into_iter()
        .map(|((name, license), versions)| Package {
            name,
            versions,
            license,
        })
        .collect())
}

/// The licence a package.json declares, read as pnpm's
/// parseLicenseFromManifest reads it: `license`, else the legacy `licenses`;
/// a string as it is, an object by its `type` (else its `name`), and a list
/// as its entries joined with OR. `None` when it declares nothing. Unlike
/// pnpm, nothing is ever read from a LICENSE file.
fn declared_license(manifest: &Value) -> Option<String> {
    license_field(&manifest["license"]).or_else(|| license_field(&manifest["licenses"]))
}

fn license_field(field: &Value) -> Option<String> {
    let Some(entries) = field.as_array() else {
        return license_type(field);
    };
    let types: Vec<String> = entries.iter().filter_map(license_type).collect();
    match types.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        _ => Some(format!("({})", types.join(" OR "))),
    }
}

fn license_type(entry: &Value) -> Option<String> {
    let text = match entry {
        Value::String(text) => Some(text.as_str()),
        Value::Object(object) => ["type", "name"]
            .into_iter()
            .find_map(|key| object.get(key)?.as_str().filter(|text| !text.is_empty())),
        _ => None,
    };
    text.filter(|text| !text.is_empty()).map(str::to_owned)
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
    /// The licence exactly as the package.json declares it, or NO_LICENSE.
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

/// The verdicts on all installed packages.
struct Judgement<'a> {
    /// Packages allowed only through an entry in js-licenses.toml.
    excepted: usize,
    /// One line per rejected package.
    problems: Vec<String>,
    /// Entries that no installed package used: none of them is that
    /// package under that licence.
    unused: Vec<&'a Exception>,
}

fn check_packages<'a>(
    packages: &[Package],
    allowlist: &Allowlist,
    exceptions: &'a [Exception],
) -> Judgement<'a> {
    let mut excepted = 0;
    let mut problems = Vec::new();
    let mut used = HashSet::new();
    for package in packages {
        match verdict(package, allowlist, exceptions) {
            Verdict::Allowed => {}
            Verdict::Excepted => {
                excepted += 1;
                used.insert((package.name.as_str(), package.license.as_str()));
            }
            Verdict::Rejected(why) => {
                problems.push(format!("{} ({}): {why}", package.label(), package.license));
            }
        }
    }
    let unused = exceptions
        .iter()
        .filter(|e| !used.contains(&(e.package.as_str(), e.license.as_str())))
        .collect();
    Judgement {
        excepted,
        problems,
        unused,
    }
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
    if declares_none(license) {
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

/// Empty, NO_LICENSE, or only a pointer to a file (`SEE LICENSE IN <file>`,
/// npm's form for custom terms): no licence that can be judged here.
fn declares_none(license: &str) -> bool {
    let lower = license.trim().to_ascii_lowercase();
    lower.is_empty()
        || lower.eq_ignore_ascii_case(NO_LICENSE)
        || ["see license in ", "see licence in "]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
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
            // A pointer to a file declares nothing that can be judged.
            ("SEE LICENSE IN LICENSE.md", "declares no licence"),
            ("see licence in COPYING", "declares no licence"),
            ("SEE LICENSE", "not an SPDX expression"),
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
    fn unused_entries_are_found_per_package_and_licence() {
        let exceptions = [
            exception("x", "Unknown"),
            exception("x", "CC-BY-4.0"),
            exception("y", "MIT-0"),
            exception("z", "Unlicense"),
        ];
        let packages = [
            // x is installed under one of its two licences only.
            package("x", "Unknown"),
            // y is installed, but under a licence its entry doesn't name.
            package("y", "WTFPL"),
            package("z", "Unlicense"),
            package("allowed", "MIT"),
        ];
        let judgement = check_packages(&packages, &allowlist(), &exceptions);
        assert_eq!(judgement.excepted, 2);
        assert_eq!(
            judgement.problems,
            [
                "y@1.0.0 (WTFPL): not on the allowlist: WTFPL; js-licenses.toml excepts it only under \"MIT-0\""
            ]
        );
        let unused: Vec<(&str, &str)> = judgement
            .unused
            .iter()
            .map(|e| (e.package.as_str(), e.license.as_str()))
            .collect();
        assert_eq!(unused, [("x", "CC-BY-4.0"), ("y", "MIT-0")]);
    }

    #[test]
    fn parses_the_pnpm_listing() {
        let stdout = r#"{
          "MIT": [
            { "name": "svelte", "versions": ["5.57.1"], "license": "MIT", "paths": ["/nm/svelte"] },
            { "name": "@types/node", "versions": ["20.19.43", "24.19.1"], "license": "MIT",
              "paths": ["/nm/node@20", "/nm/node@24"] }
          ],
          "Unknown": [ { "name": "css-value", "versions": ["0.0.1"], "paths": ["/nm/css-value"] } ]
        }"#;
        let entry = |name: &str, versions: &[&str], paths: &[&str]| Listed {
            name: name.to_owned(),
            versions: versions.iter().map(|v| (*v).to_owned()).collect(),
            paths: paths.iter().map(PathBuf::from).collect(),
        };
        assert_eq!(
            parse_pnpm(stdout).unwrap_or_else(|e| panic!("{e:#}")),
            [
                entry(
                    "@types/node",
                    &["20.19.43", "24.19.1"],
                    &["/nm/node@20", "/nm/node@24"]
                ),
                entry("css-value", &["0.0.1"], &["/nm/css-value"]),
                entry("svelte", &["5.57.1"], &["/nm/svelte"]),
            ]
        );
        let package = Package {
            name: "@types/node".to_owned(),
            versions: vec!["20.19.43".to_owned(), "24.19.1".to_owned()],
            license: "MIT".to_owned(),
        };
        assert_eq!(package.label(), "@types/node@20.19.43, 24.19.1");
    }

    #[test]
    fn a_failed_pnpm_run_reports_what_pnpm_said() {
        let error = |stdout: &str, stderr: &str| pnpm_error(stdout.as_bytes(), stderr.as_bytes());
        // What pnpm 11 prints under --json when there is no lockfile.
        let json = r#"{
          "error": {
            "code": "ERR_PNPM_LICENSES_NO_LOCKFILE",
            "message": "No pnpm-lock.yaml found"
          }
        }"#;
        assert_eq!(
            error(json, ""),
            "ERR_PNPM_LICENSES_NO_LOCKFILE: No pnpm-lock.yaml found"
        );
        assert_eq!(error(json, " \n"), error(json, ""));
        assert_eq!(error(r#"{ "error": { "message": "m" } }"#, ""), "m");
        // stderr, when there is any, is what pnpm meant to say.
        assert_eq!(error(json, "  ERR_PNPM_X boom\n"), "ERR_PNPM_X boom");
        // Anything else is shown as it is, shortened.
        assert_eq!(error(" plain text\n", ""), "plain text");
        assert_eq!(error(r#"{ "MIT": [] }"#, ""), r#"{ "MIT": [] }"#);
        assert_eq!(error("", ""), "no output");
        let long = "x".repeat(1000);
        assert_eq!(error(&long, ""), format!("{}...", "x".repeat(300)));
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
            r#"{ "MIT": [ { "versions": ["1.0.0"], "paths": ["/nm/x"] } ] }"#,
            // No folder to read the package.json from.
            r#"{ "MIT": [ { "name": "x", "versions": ["1.0.0"] } ] }"#,
            r#"{ "MIT": [ { "name": "x", "versions": ["1.0.0"], "paths": [] } ] }"#,
        ] {
            assert!(parse_pnpm(stdout).is_err(), "{stdout:?}");
        }
    }

    #[test]
    fn declared_licences_are_read_as_pnpm_reads_the_manifest() {
        for (manifest, declared) in [
            (json!({ "license": "MIT" }), Some("MIT")),
            (
                json!({ "license": "(MIT OR CC0-1.0)" }),
                Some("(MIT OR CC0-1.0)"),
            ),
            (
                json!({ "license": { "type": "ISC", "url": "u" } }),
                Some("ISC"),
            ),
            (
                json!({ "license": { "type": "", "name": "BSD-3-Clause" } }),
                Some("BSD-3-Clause"),
            ),
            (json!({ "licenses": [{ "type": "MIT" }] }), Some("MIT")),
            (
                json!({ "licenses": [{ "type": "MIT" }, { "type": "Apache-2.0" }] }),
                Some("(MIT OR Apache-2.0)"),
            ),
            (
                json!({ "licenses": [{ "type": "MIT" }, "GPL-2.0-only", 7] }),
                Some("(MIT OR GPL-2.0-only)"),
            ),
            // `license` wins over `licenses`; an empty one does not count.
            (
                json!({ "license": "ISC", "licenses": [{ "type": "MIT" }] }),
                Some("ISC"),
            ),
            (
                json!({ "license": "", "licenses": [{ "type": "MIT" }] }),
                Some("MIT"),
            ),
            (
                json!({ "license": "SEE LICENSE IN LICENSE.md" }),
                Some("SEE LICENSE IN LICENSE.md"),
            ),
            (json!({}), None),
            (json!({ "license": "" }), None),
            (json!({ "license": {} }), None),
            (json!({ "license": null, "licenses": [] }), None),
            (json!({ "license": 1 }), None),
        ] {
            assert_eq!(
                declared_license(&manifest).as_deref(),
                declared,
                "{manifest}"
            );
        }
    }

    /// A folder under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "navaja-xtask-licenses-{name}-{}",
                std::process::id()
            ));
            // Left over from an earlier run with the same process id.
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            Self(dir)
        }

        /// Writes `<folder>/package.json` (and, if given, `<folder>/LICENSE`)
        /// and returns the folder as pnpm would list it.
        fn install(&self, folder: &str, manifest: &Value, license_file: Option<&str>) -> String {
            let dir = self.0.join(folder);
            std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            let write = |name: &str, text: &str| {
                std::fs::write(dir.join(name), text)
                    .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            };
            write("package.json", &manifest.to_string());
            if let Some(text) = license_file {
                write("LICENSE", text);
            }
            dir.to_string_lossy().into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn licences_come_from_each_package_json_not_from_pnpm() {
        let scratch = Scratch::new("manifests");
        let install = |name: &str, version: &str, licence: Value, file: Option<&str>| {
            let mut manifest = json!({ "name": name, "version": version });
            if let (Some(fields), Some(object)) = (licence.as_object(), manifest.as_object_mut()) {
                object.extend(fields.clone());
            }
            scratch.install(&format!("{name}@{version}"), &manifest, file)
        };
        let string = install("string", "1.0.0", json!({ "license": "MIT" }), None);
        let object = install(
            "object",
            "1.0.0",
            json!({ "license": { "type": "ISC" } }),
            None,
        );
        let legacy = install(
            "legacy",
            "1.0.0",
            json!({ "licenses": [{ "type": "MIT" }, { "type": "Apache-2.0" }] }),
            None,
        );
        let absent = install("absent", "1.0.0", json!({}), None);
        let pointer = install(
            "pointer",
            "1.0.0",
            json!({ "license": "SEE LICENSE IN EULA.txt" }),
            None,
        );
        // pnpm reads a LICENSE file when the manifest declares nothing, and
        // reports every licence name it finds there: this one as "MIT".
        let inferred = install(
            "inferred",
            "1.0.0",
            json!({}),
            Some("Proprietary. You may not use this as you would MIT code."),
        );
        // Two versions under one pnpm licence, declared apart.
        let split_old = install("split", "1.0.0", json!({}), Some("MIT License"));
        let split_new = install("split", "2.0.0", json!({ "license": "MIT" }), None);
        let stdout = json!({
            "MIT": [
                { "name": "string", "versions": ["1.0.0"], "paths": [string], "license": "MIT" },
                { "name": "inferred", "versions": ["1.0.0"], "paths": [inferred], "license": "MIT" },
                { "name": "split", "versions": ["1.0.0", "2.0.0"], "paths": [split_old, split_new], "license": "MIT" }
            ],
            "ISC": [ { "name": "object", "versions": ["1.0.0"], "paths": [object], "license": "ISC" } ],
            "(MIT OR Apache-2.0)": [
                { "name": "legacy", "versions": ["1.0.0"], "paths": [legacy], "license": "(MIT OR Apache-2.0)" }
            ],
            "Unknown": [ { "name": "absent", "versions": ["1.0.0"], "paths": [absent] } ],
            "SEE LICENSE IN EULA.txt": [
                { "name": "pointer", "versions": ["1.0.0"], "paths": [pointer], "license": "SEE LICENSE IN EULA.txt" }
            ]
        })
        .to_string();

        let packages = read_manifests(&parse_pnpm(&stdout).unwrap_or_else(|e| panic!("{e:#}")))
            .unwrap_or_else(|e| panic!("{e:#}"));
        let found: Vec<(&str, &str, String)> = packages
            .iter()
            .map(|p| (p.name.as_str(), p.license.as_str(), p.versions.join(",")))
            .collect();
        let row = |name, license, versions: &str| (name, license, versions.to_owned());
        assert_eq!(
            found,
            [
                row("absent", "Unknown", "1.0.0"),
                row("inferred", "Unknown", "1.0.0"),
                row("legacy", "(MIT OR Apache-2.0)", "1.0.0"),
                row("object", "ISC", "1.0.0"),
                row("pointer", "SEE LICENSE IN EULA.txt", "1.0.0"),
                row("split", "MIT", "2.0.0"),
                row("split", "Unknown", "1.0.0"),
                row("string", "MIT", "1.0.0"),
            ]
        );

        // What pnpm inferred never gets anything through.
        let list = allowlist();
        let verdicts: Vec<(&str, Verdict)> = packages
            .iter()
            .map(|p| (p.name.as_str(), verdict(p, &list, &[])))
            .collect();
        let no_licence = || Verdict::Rejected("declares no licence".to_owned());
        assert_eq!(
            verdicts,
            [
                ("absent", no_licence()),
                ("inferred", no_licence()),
                ("legacy", Verdict::Allowed),
                ("object", Verdict::Allowed),
                ("pointer", no_licence()),
                ("split", Verdict::Allowed),
                ("split", no_licence()),
                ("string", Verdict::Allowed),
            ]
        );
        let excepted = [exception("inferred", "MIT")];
        assert_eq!(
            verdict(&packages[1], &list, &excepted),
            Verdict::Rejected(
                "declares no licence; js-licenses.toml excepts it only under \"MIT\"".to_owned()
            )
        );
        let excepted = [exception("inferred", NO_LICENSE)];
        assert_eq!(verdict(&packages[1], &list, &excepted), Verdict::Excepted);
    }

    #[test]
    fn every_listed_version_needs_its_package_json() {
        let scratch = Scratch::new("missing");
        let one = scratch.install("one", &json!({ "name": "one", "version": "1.0.0" }), None);
        let listing = |name: &str, versions: &[&str], paths: &[&str]| {
            json!({ "MIT": [ { "name": name, "versions": versions, "paths": paths } ] }).to_string()
        };
        let read = |stdout: String| {
            let listed = parse_pnpm(&stdout).unwrap_or_else(|e| panic!("{e:#}"));
            match read_manifests(&listed) {
                Ok(packages) => panic!("{packages:?}"),
                Err(error) => format!("{error:#}"),
            }
        };
        let not_installed = scratch.0.join("gone").to_string_lossy().into_owned();
        assert!(
            read(listing("gone", &["1.0.0"], &[&not_installed]))
                .contains("run `pnpm install --frozen-lockfile`")
        );
        assert!(
            read(listing("one", &["1.0.0", "2.0.0"], &[&one])).contains(
                "pnpm lists one@2.0.0, but no package.json in its paths has that version"
            )
        );
        assert!(read(listing("other", &["1.0.0"], &[&one])).contains("does not belong to other"));
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
