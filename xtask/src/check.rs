//! `cargo xtask check`: architecture rules that cargo-deny can't express,
//! and the ACL snapshot (see acl.rs).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, Dependency, DependencyKind, Metadata, MetadataCommand, PackageId};
use serde_json::Value;

use crate::util::{report, root};

const APP: &str = "navaja";
const TOOLS: &str = "navaja-tools";
const CORE: &str = "navaja-core";
const PORTS: &str = "navaja-ports";
const DOCKER: &str = "navaja-docker";
const XTASK: &str = "xtask";

/// May only be reached from the app, navaja-docker and xtask.
const FORBIDDEN: &[&str] = &[
    "tauri", "wry", "tao", "tokio", "mio", "socket2", "hyper", "reqwest", "bollard", "ureq",
];

/// The only origins the production CSP may name.
const ALLOWED_CSP_ORIGINS: &[&str] = &["http://ipc.localhost"];

pub fn run() -> Result<()> {
    let root = root();
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .features(CargoOpt::AllFeatures)
        .exec()
        .context("cargo metadata")?;

    let mut problems = Vec::new();
    match metadata
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == APP)
    {
        Some(app) => problems.extend(release_feature_problems(&app.features, &app.dependencies)),
        None => problems.push("the app crate is not in the workspace".to_owned()),
    }
    problems.extend(edge_problems(&metadata));
    problems.extend(closure_problems(&metadata));
    problems.extend(parity_problems(&metadata, &root)?);
    problems.extend(config_problems(&root)?);
    // Last: it builds the app crate's build script (cargo check). A failure
    // is one more problem, so the ones found above are still reported.
    match crate::acl::problems(&root) {
        Ok(found) => problems.extend(found),
        Err(error) => problems.push(format!("ACL snapshot could not be resolved: {error:#}")),
    }
    report("xtask check", &problems)
}

/// The app's test-only feature: WebdriverIO's Tauri plugins.
const E2E: &str = "e2e";

/// WebdriverIO's Tauri plugins, one of them an embedded WebDriver server.
fn is_test_plugin(package: &str) -> bool {
    package.starts_with("tauri-plugin-wdio")
}

/// The test-only plugins never reach a default build: `default` turns on
/// neither `e2e` nor a plugin, even through other features; only `e2e` may
/// refer to the plugins; and the plugins stay optional dependencies.
///
/// Works on the manifest's `[features]` table as cargo metadata reports it,
/// implicit features of optional dependencies included.
fn release_feature_problems(
    features: &BTreeMap<String, Vec<String>>,
    dependencies: &[Dependency],
) -> Vec<String> {
    // Features name a dependency by its key in Cargo.toml, its rename if any.
    let packages: HashMap<&str, &str> = dependencies
        .iter()
        .map(|dep| {
            (
                dep.rename.as_deref().unwrap_or(&dep.name),
                dep.name.as_str(),
            )
        })
        .collect();
    let plugin = |key: &str| {
        let name = packages.get(key).copied().unwrap_or(key);
        is_test_plugin(name).then(|| name.to_owned())
    };

    let mut problems: Vec<String> = dependencies
        .iter()
        .filter(|dep| {
            is_test_plugin(&dep.name) && dep.kind != DependencyKind::Development && !dep.optional
        })
        .map(|dep| format!("{APP}: {} must be an optional dependency", dep.name))
        .collect();

    for (feature, values) in features {
        if feature == E2E {
            continue;
        }
        for value in values {
            if let (_, Some(key)) = feature_targets(value, features)
                && let Some(name) = plugin(key)
            {
                problems.push(format!(
                    "{APP}: feature `{feature}` refers to {name}; only `{E2E}` may"
                ));
            }
        }
    }

    // Everything `default` turns on, following features through each other.
    let mut parent: HashMap<&str, &str> = HashMap::new();
    let mut seen = HashSet::from(["default"]);
    let mut queue = VecDeque::from(["default"]);
    while let Some(feature) = queue.pop_front() {
        for value in features.get(feature).into_iter().flatten() {
            let (next, dep) = feature_targets(value, features);
            if let Some(name) = dep.and_then(plugin) {
                problems.push(format!(
                    "{APP}: default features reach {name}: {} -> {value}",
                    feature_path(&parent, feature)
                ));
            }
            let Some(next) = next else { continue };
            if !seen.insert(next) {
                continue;
            }
            parent.insert(next, feature);
            if next == E2E {
                problems.push(format!(
                    "{APP}: default features reach `{E2E}`: {}",
                    feature_path(&parent, next)
                ));
            } else {
                queue.push_back(next);
            }
        }
    }
    problems
}

/// What one `[features]` entry refers to: the feature it turns on, and the
/// dependency (by key) it touches. `dep:x` turns on x; `x/f` turns on x and
/// x's feature f, and also the feature named x if there is one; `x?/f` turns
/// on f only if something else turns on x; a bare name is a feature, or an
/// optional dependency's implicit feature.
fn feature_targets<'a>(
    value: &'a str,
    features: &BTreeMap<String, Vec<String>>,
) -> (Option<&'a str>, Option<&'a str>) {
    if let Some(dep) = value.strip_prefix("dep:") {
        return (None, Some(dep));
    }
    if let Some((dep, _)) = value.split_once('/') {
        return match dep.strip_suffix('?') {
            Some(weak) => (None, Some(weak)),
            None => (features.contains_key(dep).then_some(dep), Some(dep)),
        };
    }
    (
        Some(value),
        (!features.contains_key(value)).then_some(value),
    )
}

/// `default -> a -> feature`, through the features that turned it on.
fn feature_path<'a>(parent: &HashMap<&'a str, &'a str>, mut feature: &'a str) -> String {
    let mut path = vec![feature];
    while let Some(up) = parent.get(feature) {
        path.push(up);
        feature = up;
    }
    path.reverse();
    path.join(" -> ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    App,
    Tools,
    Xtask,
    /// A library under `crates/`.
    Lib,
}

fn kind_of(name: &str) -> Kind {
    match name {
        APP => Kind::App,
        TOOLS => Kind::Tools,
        XTASK => Kind::Xtask,
        _ => Kind::Lib,
    }
}

/// docs/architecture.md §2: `crates/*` depend only on navaja-core (navaja-docker
/// may also use navaja-ports); tools use any library except navaja-docker;
/// nothing depends on tools, the app or xtask, except the app on tools.
fn edge_allowed(from: &str, to: &str) -> bool {
    match (kind_of(from), kind_of(to)) {
        (_, Kind::App | Kind::Xtask) => false,
        (Kind::App, _) => true,
        (_, Kind::Tools) => false,
        (Kind::Xtask, _) => true,
        (Kind::Tools, Kind::Lib) => to != DOCKER,
        (Kind::Lib, Kind::Lib) => to == CORE || (from == DOCKER && to == PORTS),
    }
}

fn edge_problems(metadata: &Metadata) -> Vec<String> {
    let members: HashSet<&str> = metadata
        .workspace_packages()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    let mut problems = Vec::new();
    for package in metadata.workspace_packages() {
        for dep in &package.dependencies {
            let to = dep.name.as_str();
            if dep.kind == DependencyKind::Development || !members.contains(to) {
                continue;
            }
            if !edge_allowed(&package.name, to) {
                problems.push(format!(
                    "{} must not depend on {to} (docs/architecture.md §2, dependency rules)",
                    package.name
                ));
            }
        }
    }
    problems
}

fn is_forbidden(name: &str) -> bool {
    FORBIDDEN.contains(&name) || name.starts_with("tauri-") || name.starts_with("hyper-")
}

/// No normal-dependency closure outside the app, navaja-docker and xtask may
/// contain a UI runtime, an async runtime or anything network-capable.
fn closure_problems(metadata: &Metadata) -> Vec<String> {
    let Some(resolve) = &metadata.resolve else {
        return vec!["cargo metadata returned no resolve graph".to_owned()];
    };
    let names: HashMap<&PackageId, &str> = metadata
        .packages
        .iter()
        .map(|p| (&p.id, p.name.as_str()))
        .collect();
    let nodes: HashMap<&PackageId, _> = resolve.nodes.iter().map(|n| (&n.id, n)).collect();

    let mut problems = Vec::new();
    for package in metadata.workspace_packages() {
        if matches!(package.name.as_str(), APP | DOCKER | XTASK) {
            continue;
        }
        // Breadth-first over normal dependencies, remembering how we got there.
        let mut parent: HashMap<&PackageId, &PackageId> = HashMap::new();
        let mut seen: HashSet<&PackageId> = HashSet::from([&package.id]);
        let mut queue = VecDeque::from([&package.id]);
        while let Some(id) = queue.pop_front() {
            let Some(node) = nodes.get(id) else { continue };
            for dep in &node.deps {
                let normal = dep
                    .dep_kinds
                    .iter()
                    .any(|k| k.kind == DependencyKind::Normal);
                if !normal || !seen.insert(&dep.pkg) {
                    continue;
                }
                parent.insert(&dep.pkg, id);
                let name = names.get(&dep.pkg).copied().unwrap_or("?");
                if is_forbidden(name) {
                    let mut chain = vec![name];
                    let mut cursor = id;
                    while let Some(up) = parent.get(cursor) {
                        chain.push(names.get(cursor).copied().unwrap_or("?"));
                        cursor = up;
                    }
                    chain.push(package.name.as_str());
                    chain.reverse();
                    problems.push(format!(
                        "{} reaches {name}: {}",
                        package.name,
                        chain.join(" -> ")
                    ));
                } else {
                    queue.push_back(&dep.pkg);
                }
            }
        }
    }
    problems
}

/// `tauri` and the npm packages must agree on major.minor, and the npm side
/// must be pinned exactly.
fn parity_problems(metadata: &Metadata, root: &Path) -> Result<Vec<String>> {
    let Some(tauri) = metadata
        .packages
        .iter()
        .find(|p| p.name.as_str() == "tauri")
    else {
        return Ok(vec![
            "the tauri crate is not in the dependency graph".to_owned(),
        ]);
    };
    let crate_minor = (tauri.version.major, tauri.version.minor);
    let package: Value = read_json(&root.join("app/package.json"))?;
    let mut problems = Vec::new();
    for (section, name) in [
        ("dependencies", "@tauri-apps/api"),
        ("devDependencies", "@tauri-apps/cli"),
    ] {
        let spec = package[section][name].as_str().unwrap_or("");
        match parse_exact(spec) {
            None => problems.push(format!(
                "app/package.json {section}.{name} must be an exact version, found {spec:?}"
            )),
            Some(minor) if minor != crate_minor => problems.push(format!(
                "{name} {spec} does not match the tauri crate {} (same major.minor required)",
                tauri.version
            )),
            Some(_) => {}
        }
    }
    Ok(problems)
}

/// `1.2.3` -> (1, 2); anything with a range operator -> None.
fn parse_exact(spec: &str) -> Option<(u64, u64)> {
    let mut parts = spec.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let _patch: u64 = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((major, minor))
}

fn read_json(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn config_problems(root: &Path) -> Result<Vec<String>> {
    let tauri_dir = root.join("app/src-tauri");
    let config = read_json(&tauri_dir.join("tauri.conf.json"))?;
    let mut problems = tauri_config_problems(&config);
    let overlay = read_json(&tauri_dir.join("e2e.conf.json"))?;
    problems.extend(e2e_overlay_problems(&overlay));

    let mut names = Vec::new();
    for entry in
        std::fs::read_dir(&tauri_dir).with_context(|| format!("reading {}", tauri_dir.display()))?
    {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    problems.extend(platform_config_problems(&names));

    let mut sources = Vec::new();
    rust_files(&tauri_dir.join("src"), &mut sources)?;
    sources.sort();
    for path in sources {
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let file = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        problems.extend(context_macro_problems(&file, &source));
    }

    let capabilities = tauri_dir.join("capabilities");
    for entry in std::fs::read_dir(&capabilities)
        .with_context(|| format!("reading {}", capabilities.display()))?
    {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            let capability = read_json(&path)?;
            let file = path
                .file_name()
                .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
            problems.extend(capability_problems(&file, &capability));
        }
    }
    Ok(problems)
}

/// The targets tauri-utils looks up a config file of their own for.
const CONFIG_PLATFORMS: &[&str] = &["linux", "macos", "windows", "android", "ios"];

/// tauri-build and tauri-codegen merge `tauri.<os>.conf.json[5]` or
/// `Tauri.<os>.toml` over tauri.conf.json when building for that OS, but
/// the checks here and acl.lock.json read tauri.conf.json alone. Compared
/// without case: on Windows and macOS the lookup ignores it.
fn platform_config_problems(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| {
            CONFIG_PLATFORMS.iter().any(|os| {
                [
                    format!("tauri.{os}.conf.json"),
                    format!("tauri.{os}.conf.json5"),
                    format!("Tauri.{os}.toml"),
                ]
                .iter()
                .any(|platform| platform.eq_ignore_ascii_case(name))
            })
        })
        .map(|name| {
            format!(
                "app/src-tauri/{name}: per-platform config files are merged over \
                 tauri.conf.json at build time and are not covered by `cargo xtask check` \
                 or acl.lock.json; extend xtask before adding one"
            )
        })
        .collect()
}

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, files)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

const CONTEXT_MACRO: &str = "generate_context";

/// `tauri::generate_context!()` takes no arguments here. A config path,
/// `capabilities = [...]` or any other argument reaches tauri-codegen, and
/// so the built app, but not acl.lock.json or the checks above. A renamed
/// import would hide a call from this check, so it is refused too. Other
/// mentions, such as in prose, are not calls and pass.
fn context_macro_problems(file: &str, source: &str) -> Vec<String> {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut problems = Vec::new();
    for (start, _) in source.match_indices(CONTEXT_MACRO) {
        let after = &source[start + CONTEXT_MACRO.len()..];
        if source[..start].chars().next_back().is_some_and(is_ident)
            || after.chars().next().is_some_and(is_ident)
        {
            continue;
        }
        let line = source[..start].matches('\n').count() + 1;
        let rest = after.trim_start();
        if let Some(call) = rest.strip_prefix('!') {
            let mut chars = call.trim_start().chars();
            let close = match chars.next() {
                Some('(') => ')',
                Some('[') => ']',
                Some('{') => '}',
                _ => continue,
            };
            if !chars.as_str().trim_start().starts_with(close) {
                problems.push(format!(
                    "{file}:{line}: generate_context! must be called with no arguments; \
                     its arguments reach the build but not `cargo xtask check` or acl.lock.json"
                ));
            }
        } else if rest
            .strip_prefix("as")
            .is_some_and(|r| r.starts_with(char::is_whitespace))
        {
            problems.push(format!(
                "{file}:{line}: generate_context must not be renamed, so `cargo xtask check` \
                 sees every call"
            ));
        }
    }
    problems
}

fn tauri_config_problems(config: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    // `tauri build` passes these to cargo, which would put features such as
    // `e2e` into release builds behind the back of release_feature_problems.
    if !config["build"]["features"].is_null() {
        problems.push(
            "tauri.conf.json: build.features must not be set; release builds use the              crate's default features only"
                .to_owned(),
        );
    }
    let app = &config["app"];
    if app["withGlobalTauri"].as_bool().unwrap_or(false) {
        problems.push("tauri.conf.json: app.withGlobalTauri must be false".to_owned());
    }
    if app["security"]["assetProtocol"]["enable"]
        .as_bool()
        .unwrap_or(false)
    {
        problems.push("tauri.conf.json: the asset protocol must stay disabled".to_owned());
    }
    if app["security"]["capabilities"] != serde_json::json!(["main"]) {
        problems.push(
            "tauri.conf.json: app.security.capabilities must be exactly [\"main\"]".to_owned(),
        );
    }
    match app["security"]["csp"].as_object() {
        None => problems.push("tauri.conf.json: app.security.csp must be set".to_owned()),
        Some(csp) => {
            for (directive, sources) in csp {
                for source in sources.as_str().unwrap_or("").split_whitespace() {
                    // Host sources, wildcards and scheme sources such as `https:`;
                    // `ipc:` (Tauri IPC) and `data:` (inline images) stay local.
                    let scheme_source =
                        source.ends_with(':') && !matches!(source, "ipc:" | "data:");
                    let remote = source.contains("://") || source == "*" || scheme_source;
                    if remote && !ALLOWED_CSP_ORIGINS.contains(&source) {
                        problems.push(format!(
                            "tauri.conf.json: CSP {directive} allows {source}; the webview gets no network access"
                        ));
                    }
                }
            }
        }
    }
    problems
}

/// What the end-to-end build's overlay (`--config e2e.conf.json`, merged over
/// tauri.conf.json) may set, so the tests run the CSP, headers and other
/// security settings that ship. `app.withGlobalTauri` is not among them: the
/// suite drives the app over WebDriver alone, with no `window.__TAURI__`.
const E2E_OVERLAY_ALLOWED: &[&[&str]] = &[
    &["$schema"],
    &["identifier"],
    &["app", "security", "capabilities"],
];

fn e2e_overlay_problems(overlay: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    overlay_walk(overlay, &mut Vec::new(), &mut problems);
    let capabilities = &overlay["app"]["security"]["capabilities"];
    for capability in capabilities.as_array().into_iter().flatten() {
        problems.extend(e2e_capability_problems(capability));
    }
    problems
}

/// Inline capabilities in the overlay add WebdriverIO's own permissions and
/// nothing else: no core or other plugin permissions, and no remote origins.
/// Capability names refer to files in capabilities/, checked on their own.
fn e2e_capability_problems(capability: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    if capability.is_string() {
        return problems;
    }
    if !capability["remote"].is_null() {
        problems.push("e2e.conf.json: an e2e capability may not grant remote origins".to_owned());
    }
    for permission in capability["permissions"].as_array().into_iter().flatten() {
        let id = permission
            .as_str()
            .or_else(|| permission["identifier"].as_str())
            .unwrap_or("");
        if !id.starts_with("wdio:") {
            problems.push(format!(
                "e2e.conf.json: e2e capability permission {id:?} is not one of WebdriverIO's"
            ));
        }
    }
    problems
}

fn overlay_walk<'a>(value: &'a Value, path: &mut Vec<&'a str>, problems: &mut Vec<String>) {
    if E2E_OVERLAY_ALLOWED.contains(&path.as_slice()) {
        return;
    }
    // Objects merge key by key; anything else replaces what tauri.conf.json set.
    let leads_to_allowed = E2E_OVERLAY_ALLOWED
        .iter()
        .any(|allowed| allowed.starts_with(path));
    match value.as_object() {
        Some(object) if leads_to_allowed => {
            for (key, child) in object {
                path.push(key);
                overlay_walk(child, path, problems);
                path.pop();
            }
        }
        _ if path.is_empty() => problems.push("e2e.conf.json must be a JSON object".to_owned()),
        _ => problems.push(format!(
            "e2e.conf.json sets {}; the e2e build may only change identifier \
             and app.security.capabilities",
            path.join(".")
        )),
    }
}

/// Capabilities grant the app's own commands (`allow-*`) only: no core or
/// plugin permissions without a reviewed change to this check.
fn capability_problems(file: &str, capability: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    for permission in capability["permissions"].as_array().into_iter().flatten() {
        let id = permission
            .as_str()
            .or_else(|| permission["identifier"].as_str())
            .unwrap_or("");
        if !id.starts_with("allow-") || id.contains(':') {
            problems.push(format!(
                "capabilities/{file}: permission {id:?} is not one of the app's own commands"
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn edges() {
        assert!(edge_allowed(APP, TOOLS));
        assert!(edge_allowed(APP, CORE));
        assert!(edge_allowed(APP, DOCKER));
        assert!(edge_allowed(TOOLS, CORE));
        assert!(edge_allowed(TOOLS, PORTS));
        assert!(edge_allowed("navaja-env", CORE));
        assert!(edge_allowed(DOCKER, PORTS));

        assert!(!edge_allowed(TOOLS, DOCKER));
        assert!(!edge_allowed(PORTS, TOOLS));
        assert!(!edge_allowed(CORE, PORTS));
        assert!(!edge_allowed("navaja-env", PORTS));
        assert!(!edge_allowed(TOOLS, APP));
        assert!(!edge_allowed(CORE, XTASK));
    }

    #[test]
    fn forbidden_names() {
        for name in [
            "tokio",
            "tauri",
            "tauri-runtime-wry",
            "hyper",
            "hyper-util",
            "reqwest",
            "bollard",
        ] {
            assert!(is_forbidden(name), "{name}");
        }
        for name in ["serde", "uuid", "roxmltree", "tauri_like"] {
            assert!(!is_forbidden(name), "{name}");
        }
    }

    /// A `cargo metadata` graph. `packages` are names, with whether each is
    /// a workspace member; `edges` are (from, to, kind), the kind as cargo
    /// writes it: null for a normal dependency, "dev" or "build".
    fn graph(packages: &[(&str, bool)], edges: &[(&str, &str, Option<&str>)]) -> Metadata {
        let id = |name: &str| format!("path+file:///ws/{name}#1.0.0");
        let nodes: Vec<Value> = packages
            .iter()
            .map(|(name, _)| {
                let out = edges.iter().filter(|(from, _, _)| from == name);
                let deps: Vec<Value> = out
                    .clone()
                    .map(|(_, to, kind)| {
                        json!({ "name": to.replace('-', "_"), "pkg": id(to),
                                "dep_kinds": [{ "kind": kind, "target": null }] })
                    })
                    .collect();
                let dependencies: Vec<String> = out.map(|(_, to, _)| id(to)).collect();
                json!({ "id": id(name), "deps": deps, "dependencies": dependencies,
                        "features": [] })
            })
            .collect();
        let manifests: Vec<Value> = packages
            .iter()
            .map(|(name, _)| {
                json!({ "name": name, "version": "1.0.0", "id": id(name),
                        "dependencies": [], "targets": [], "features": {},
                        "manifest_path": format!("/ws/{name}/Cargo.toml") })
            })
            .collect();
        let members: Vec<String> = packages
            .iter()
            .filter(|(_, member)| *member)
            .map(|(name, _)| id(name))
            .collect();
        serde_json::from_value(json!({
            "packages": manifests, "workspace_members": members,
            "resolve": { "nodes": nodes, "root": null },
            "workspace_root": "/ws", "target_directory": "/ws/target", "version": 1,
        }))
        .expect("a valid cargo metadata graph")
    }

    /// Roadmap M2a item 5: tokio, hyper or tauri under navaja-tools fails the
    /// check, through any chain of normal dependencies. Dev-dependencies do
    /// not reach a build, and the app may use all three.
    #[test]
    fn forbidden_crates_under_navaja_tools_fail_the_check() {
        let metadata = graph(
            &[
                (TOOLS, true),
                (APP, true),
                (CORE, true),
                ("mid", false),
                ("serde", false),
                ("tokio", false),
                ("hyper", false),
                ("tauri", false),
            ],
            &[
                (TOOLS, CORE, None),
                (TOOLS, "hyper", None),
                (TOOLS, "mid", None),
                (TOOLS, "tauri", Some("dev")),
                ("mid", "serde", None),
                ("mid", "tokio", None),
                (CORE, "serde", None),
                (APP, TOOLS, None),
                (APP, "tokio", None),
                (APP, "tauri", None),
            ],
        );
        assert_eq!(
            closure_problems(&metadata),
            [
                "navaja-tools reaches hyper: navaja-tools -> hyper",
                "navaja-tools reaches tokio: navaja-tools -> mid -> tokio",
            ]
        );

        // The same tauri as a normal dependency, one level down.
        let metadata = graph(
            &[(TOOLS, true), ("mid", false), ("tauri", false)],
            &[(TOOLS, "mid", None), ("mid", "tauri", None)],
        );
        assert_eq!(
            closure_problems(&metadata),
            ["navaja-tools reaches tauri: navaja-tools -> mid -> tauri"]
        );
    }

    #[test]
    fn exact_versions() {
        assert_eq!(parse_exact("2.12.1"), Some((2, 12)));
        for bad in ["^2.12.1", "~2.12.1", "2.12", "2.x", "", "2.12.1-beta.1"] {
            assert_eq!(parse_exact(bad), None, "{bad}");
        }
    }

    #[test]
    fn csp_rejects_network_sources() {
        let good = json!({ "app": { "withGlobalTauri": false, "security": {
            "capabilities": ["main"],
            "csp": { "default-src": "'self'", "img-src": "'self' data:",
                     "connect-src": "ipc: http://ipc.localhost" } } } });
        assert!(
            tauri_config_problems(&good).is_empty(),
            "{:?}",
            tauri_config_problems(&good)
        );

        for (directive, value) in [
            (
                "connect-src",
                "ipc: http://ipc.localhost https://example.com",
            ),
            ("img-src", "'self' https:"),
            ("script-src", "'self' *"),
            ("connect-src", "ipc: ws://localhost:1420"),
        ] {
            let mut bad = good.clone();
            bad["app"]["security"]["csp"][directive] = json!(value);
            assert!(
                !tauri_config_problems(&bad).is_empty(),
                "{directive}: {value}"
            );
        }

        let mut global = good.clone();
        global["app"]["withGlobalTauri"] = json!(true);
        assert!(!tauri_config_problems(&global).is_empty());

        let mut features = good.clone();
        features["build"] = json!({ "features": ["e2e"] });
        assert!(!tauri_config_problems(&features).is_empty());
    }

    fn dependency(name: &str, rename: Option<&str>, optional: bool) -> Dependency {
        serde_json::from_value(json!({
            "name": name, "source": null, "req": "*", "kind": null,
            "optional": optional, "uses_default_features": true, "features": [],
            "target": null, "rename": rename, "registry": null, "path": null,
        }))
        .expect("a valid dependency")
    }

    fn feature_map(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(name, values)| {
                let values = values.iter().map(|v| (*v).to_owned()).collect();
                ((*name).to_owned(), values)
            })
            .collect()
    }

    /// The app's own shape: both plugins optional, enabled by `e2e` only.
    fn plugins() -> Vec<Dependency> {
        vec![
            dependency("serde", None, false),
            dependency("tauri-plugin-wdio", None, true),
            dependency("tauri-plugin-wdio-webdriver", None, true),
        ]
    }

    const E2E_ENTRY: (&str, &[&str]) = (
        "e2e",
        &["dep:tauri-plugin-wdio", "dep:tauri-plugin-wdio-webdriver"],
    );

    #[test]
    fn release_features_allow_e2e_off_by_default() {
        for features in [
            feature_map(&[E2E_ENTRY]),
            feature_map(&[E2E_ENTRY, ("default", &["ts"]), ("ts", &["serde/std"])]),
        ] {
            let problems = release_feature_problems(&features, &plugins());
            assert!(problems.is_empty(), "{problems:?}");
        }
    }

    #[test]
    fn release_features_reject_e2e_in_default() {
        let direct = feature_map(&[E2E_ENTRY, ("default", &["e2e"])]);
        let problems = release_feature_problems(&direct, &plugins());
        assert_eq!(
            problems,
            ["navaja: default features reach `e2e`: default -> e2e"]
        );

        let transitive = feature_map(&[
            E2E_ENTRY,
            ("default", &["ts", "docker"]),
            ("ts", &[]),
            ("docker", &["testing"]),
            ("testing", &["e2e"]),
        ]);
        let problems = release_feature_problems(&transitive, &plugins());
        assert_eq!(
            problems,
            ["navaja: default features reach `e2e`: default -> docker -> testing -> e2e"]
        );
    }

    #[test]
    fn release_features_reject_plugins_outside_e2e() {
        for (name, values) in [
            ("default", &["dep:tauri-plugin-wdio-webdriver"][..]),
            ("default", &["tauri-plugin-wdio/feature"]),
            ("default", &["tauri-plugin-wdio-webdriver?/feature"]),
            ("testing", &["dep:tauri-plugin-wdio"]),
            // An optional dependency nothing names as `dep:x` gets an implicit
            // feature, which cargo metadata lists.
            ("tauri-plugin-wdio", &["dep:tauri-plugin-wdio"]),
        ] {
            let features = feature_map(&[E2E_ENTRY, (name, values)]);
            let problems = release_feature_problems(&features, &plugins());
            assert!(
                problems
                    .iter()
                    .any(|p| p.contains(&format!("`{name}` refers to"))),
                "{name} = {values:?}: {problems:?}"
            );
        }

        // Through another feature, and through a renamed dependency.
        let mut dependencies = plugins();
        dependencies.push(dependency("tauri-plugin-wdio", Some("wd"), true));
        let features = feature_map(&[
            E2E_ENTRY,
            ("default", &["docker"]),
            ("docker", &["wd/feature"]),
        ]);
        let problems = release_feature_problems(&features, &dependencies);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("reach tauri-plugin-wdio: default -> docker -> wd/feature")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p.contains("`docker` refers to tauri-plugin-wdio")),
            "{problems:?}"
        );
    }

    #[test]
    fn release_features_require_optional_plugins() {
        let mut dependencies = plugins();
        dependencies[2] = dependency("tauri-plugin-wdio-webdriver", None, false);
        let problems = release_feature_problems(&feature_map(&[E2E_ENTRY]), &dependencies);
        assert_eq!(
            problems,
            ["navaja: tauri-plugin-wdio-webdriver must be an optional dependency"]
        );
    }

    #[test]
    fn release_features_of_the_app_manifest() {
        let metadata = MetadataCommand::new()
            .manifest_path(crate::util::root().join("Cargo.toml"))
            .no_deps()
            .exec()
            .expect("cargo metadata");
        let app = metadata
            .workspace_packages()
            .into_iter()
            .find(|p| p.name.as_str() == APP)
            .expect("the app crate");
        assert!(app.features.contains_key(E2E));
        assert!(app.dependencies.iter().any(|d| is_test_plugin(&d.name)));
        let problems = release_feature_problems(&app.features, &app.dependencies);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn e2e_overlay_changes_only_test_settings() {
        let ok = json!({
            "$schema": "https://schema.tauri.app/config/2",
            "identifier": "dev.navaja.e2e",
            "app": { "security": {
                "capabilities": ["main", { "identifier": "e2e", "permissions": ["wdio:default"] }] } }
        });
        assert!(
            e2e_overlay_problems(&ok).is_empty(),
            "{:?}",
            e2e_overlay_problems(&ok)
        );

        for (bad, path) in [
            (
                json!({ "app": { "security": { "csp": null } } }),
                "app.security.csp",
            ),
            (
                json!({ "app": { "security": { "headers": {} } } }),
                "app.security.headers",
            ),
            (
                json!({ "app": { "security": { "dangerousDisableAssetCspModification": true } } }),
                "app.security.dangerousDisableAssetCspModification",
            ),
            (
                json!({ "app": { "withGlobalTauri": true } }),
                "app.withGlobalTauri",
            ),
            (json!({ "app": { "windows": [] } }), "app.windows"),
            (json!({ "app": { "security": null } }), "app.security"),
            (json!({ "app": null }), "app"),
            (
                json!({ "build": { "devUrl": "http://localhost:1420" } }),
                "build",
            ),
            (json!({ "plugins": {} }), "plugins"),
        ] {
            let problems = e2e_overlay_problems(&bad);
            assert_eq!(problems.len(), 1, "{bad}: {problems:?}");
            assert!(
                problems[0].starts_with(&format!("e2e.conf.json sets {path};")),
                "{problems:?}"
            );
        }
        assert!(!e2e_overlay_problems(&json!([])).is_empty());

        for capability in [
            json!({ "identifier": "e2e", "permissions": ["core:default"] }),
            json!({ "identifier": "e2e", "permissions": [{ "identifier": "fs:read-all" }] }),
            json!({ "identifier": "e2e", "permissions": ["wdio:default"],
                    "remote": { "urls": ["https://example.com"] } }),
        ] {
            let overlay =
                json!({ "app": { "security": { "capabilities": ["main", capability] } } });
            assert_eq!(
                e2e_overlay_problems(&overlay).len(),
                1,
                "{overlay}: {:?}",
                e2e_overlay_problems(&overlay)
            );
        }
    }

    #[test]
    fn per_platform_configs_are_refused() {
        let names = |list: &[&str]| list.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let ok = names(&[
            "tauri.conf.json",
            "e2e.conf.json",
            "acl.lock.json",
            "Cargo.toml",
            "build.rs",
            "capabilities",
            "tauri.conf.json.bak",
            "tauri.freebsd.conf.json",
        ]);
        assert!(platform_config_problems(&ok).is_empty());

        let bad = names(&[
            "tauri.linux.conf.json",
            "tauri.macos.conf.json5",
            "Tauri.windows.toml",
            "tauri.android.conf.json",
            "Tauri.ios.toml",
            "TAURI.Windows.conf.JSON",
        ]);
        let problems = platform_config_problems(&bad);
        assert_eq!(problems.len(), bad.len(), "{problems:?}");
        assert!(
            problems[0].starts_with(
                "app/src-tauri/tauri.linux.conf.json: per-platform config files are merged over \
                 tauri.conf.json"
            ),
            "{problems:?}"
        );
    }

    #[test]
    fn generate_context_takes_no_arguments() {
        for ok in [
            "tauri::Builder::default().build(tauri::generate_context!())",
            "let context = generate_context! ( );",
            "generate_context![]\ngenerate_context!{ }",
            "// `generate_context!` reads tauri.conf.json",
            "fn my_generate_context() {}\nlet generate_contexts = 1;",
        ] {
            let problems = context_macro_problems("lib.rs", ok);
            assert!(problems.is_empty(), "{ok}: {problems:?}");
        }

        for bad in [
            "tauri::generate_context!(\"tauri.other.conf.json\")",
            "tauri::generate_context!(capabilities = [\"extra.json\"])",
            "generate_context![test = true]",
            "generate_context! {\n    assets = Assets,\n}",
            "use tauri::generate_context as context;",
            "use tauri::{generate_context as context, Manager};",
        ] {
            let problems = context_macro_problems("app/src-tauri/src/lib.rs", bad);
            assert_eq!(problems.len(), 1, "{bad}: {problems:?}");
            assert!(
                problems[0].starts_with("app/src-tauri/src/lib.rs:1: generate_context"),
                "{problems:?}"
            );
        }

        let source =
            "fn run() {\n    let a = 1;\n    tauri::generate_context!(capabilities = []);\n}";
        let problems = context_macro_problems("lib.rs", source);
        assert_eq!(
            problems,
            [
                "lib.rs:3: generate_context! must be called with no arguments; its arguments \
              reach the build but not `cargo xtask check` or acl.lock.json"
            ]
        );
    }

    #[test]
    fn the_app_sources_call_generate_context_plainly() {
        let root = crate::util::root();
        let mut sources = Vec::new();
        rust_files(&root.join("app/src-tauri/src"), &mut sources).expect("the app's sources");
        let calls = sources
            .iter()
            .map(|path| std::fs::read_to_string(path).expect("a source file"))
            .filter(|source| source.contains("generate_context!()"))
            .count();
        assert_eq!(calls, 1);
        let problems = config_problems(&root).expect("the app's config");
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn capabilities_grant_app_commands_only() {
        let ok = json!({ "permissions": ["allow-app-info", "allow-shell-ready"] });
        assert!(capability_problems("main.json", &ok).is_empty());
        for bad in ["core:default", "opener:allow-open-url", "fs:read-all"] {
            let cap = json!({ "permissions": [bad] });
            assert!(!capability_problems("main.json", &cap).is_empty(), "{bad}");
        }
    }
}
