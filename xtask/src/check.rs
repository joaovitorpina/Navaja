//! `cargo xtask check`: architecture rules that cargo-deny can't express.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, DependencyKind, Metadata, MetadataCommand, PackageId};
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
    problems.extend(release_feature_problems(&metadata));
    problems.extend(edge_problems(&metadata));
    problems.extend(closure_problems(&metadata));
    problems.extend(parity_problems(&metadata, &root)?);
    problems.extend(config_problems(&root)?);
    report("xtask check", &problems)
}

/// Test-only features (WebdriverIO plugins, an embedded WebDriver server)
/// must never be on by default.
fn release_feature_problems(metadata: &Metadata) -> Vec<String> {
    let Some(app) = metadata
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == APP)
    else {
        return vec!["the app crate is not in the workspace".to_owned()];
    };
    let defaults = app.features.get("default").cloned().unwrap_or_default();
    defaults
        .iter()
        .filter(|feature| feature.as_str() == "e2e" || feature.contains("wdio"))
        .map(|feature| format!("{APP}: feature `{feature}` must not be a default feature"))
        .collect()
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

fn tauri_config_problems(config: &Value) -> Vec<String> {
    let mut problems = Vec::new();
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
