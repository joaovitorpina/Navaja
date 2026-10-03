# Navaja architecture

This describes the v1 design accepted in [ADR 0001](adr/0001-stack.md): Tauri 2 with Svelte 5, all tool logic in Rust, and fully offline operation.

- [roadmap.md](roadmap.md) has the milestones, the spikes and how the work is verified.
- A marker such as **(S2.2)** refers to a spike there. A spike gates the work that comes after it.

## 1. Invariants

- **Rust owns** the registry, tool logic, search ranking, settings, logs, clipboard writes and the updater.
- **The webview** renders data and sends intents. It has no network, filesystem, dialog or shell permission.
- **Tools are pure and synchronous** (JSON in, JSON out). They never print, prompt, start a runtime, open a connection or receive a file path. In v1, text tools take pasted text; file input and output come after v1.
- **Generic commands** serve every tool. `UiSpec` renders every text tool. Navigation, search and tray entries all come from `list_tools()`.
- **OS-specific code sits behind traits**, one module per OS: `navaja_ports::Platform` and the app's `ShellPlatform`.

## 2. Repository layout

```
Navaja/
├── Cargo.toml            virtual workspace: crates/*, tools, app/src-tauri, xtask · [workspace.lints]
├── rust-toolchain.toml · clippy.toml · deny.toml · release-plz.toml · renovate.json · .gitattributes (eol=lf)
├── package.json · pnpm-workspace.yaml   (pnpm 11 pinned via packageManager; workspace root lets Vite serve ../tools)
├── .github/workflows/    ci · advisories · bundle · spikes · release · release-build
├── crates/
│   ├── navaja-core/      Tool, ToolMeta, UiSpec, payload types, ToolError, Ctx, Registry, search::rank, sys::spawn_system
│   ├── navaja-ports/     model · platform · gather · explain · render · kill · sys/{windows,linux,macos}/ · tests/{golden,fixtures,live.rs}
│   └── navaja-docker/    ContainerEngine over bollard 0.21 ("pipe" feature only) + local-only endpoint validator
├── tools/                crate navaja-tools, one folder per tool
│   ├── lib.rs            register_tools! { … }   ← the one registration line per tool
│   ├── registry_test.rs  folders == list, ids == folders, spec/view/i18n consistency, side-effect-free action probe
│   ├── base64/ hash/ json/ jwt/ url/ uuid/       mod.rs · icon.svg · tests.rs · snapshots/
│   └── ports/            mod.rs · icon.svg · ui/View.svelte (+PortTable, WhyPanel, KillDialog) · ui/i18n/en.ts
├── app/                  the only pnpm package (Vite root + Tauri project)
│   ├── src/bindings/     ts-rs output; never hand-edited; CI drift check
│   ├── src/lib/          ipc · registry · router · i18n · view-kit/ (the only API custom views may import) · components/ui/ (shadcn copies)
│   ├── src/shell/        Sidebar · CommandPalette · ToolHost · Home · Settings · About · dialogs
│   ├── src/generic/      GenericToolView · TransformView · GeneratorView · outputs/
│   ├── src/editor/       CodeMirror 6 {@attach} + size thresholds
│   ├── e2e/              smoke · single-instance · egress-canary · json · ports
│   └── src-tauri/        crate navaja · features: default [docker], updater (M6), e2e
│       ├── tauri.conf.json · tauri.release.conf.json (updater artifacts + pubkey) · e2e.conf.json
│       ├── capabilities/main.json · acl.lock.json · nsis/hooks.nsh
│       └── src/          commands · runner · window · guard · tray · hotkey · args · clipboard · settings · channel · updater · logging · crash · platform/{windows,linux,macos}.rs
├── xtask/                check · tool-gate · bindings · icons · notices · capture-ports · measure · verify-release · manifests
├── assets/brand/         navaja.svg · tray-template.svg · tray-color.svg · GUIDELINES.md
├── packaging/            winget / scoop / homebrew templates · dryrun.json (scratch repo only)
├── scripts/              check-eol.sh · check-identifiers.sh (CI)
└── docs/                 adr/ · architecture.md · roadmap.md · spikes.md · adding-a-tool.md · install.md · privacy.md · wayland-shortcut.md · release.md
```

### Dependency rules

```
navaja (app) ─► navaja-tools ─► navaja-ports ─► navaja-core
   ├─► navaja-core      └──────────────────────► navaja-core
   └─► navaja-docker [feature "docker"] ─► navaja-ports
```

- Crates under `crates/*` depend only on `navaja-core`. The one exception is `navaja-docker`, which also depends on `navaja-ports` for the `ContainerEngine` trait and the model types.
- Tools may use any `crates/navaja-*` crate except `navaja-docker`.
- Nothing depends on `tools/` or on the app.
- **No networking outside `navaja-docker` and the app.** Only their dependency trees may contain Tauri, tokio, mio, socket2, an HTTP client or bollard. `cargo xtask check` asserts this, and cargo-deny `wrappers` back it up.
- **Two network dependencies, each with one owner.** Only `navaja-docker` pulls in bollard and hyper, and only `tauri-plugin-updater` pulls in reqwest.
- **Lints** ban printing, `exit`, `unsafe` outside FFI modules, socket and resolver calls, and `Command::new`.
  - **One exception:** the bind-only reserved-range probe in navaja-ports' Windows module (M5). It never calls `listen()` or `connect()`, and it is allowlisted in its FFI module.
  - **Process start:** processes start only through `navaja_core::sys::spawn_system`, which takes an absolute OS program path, sets `CREATE_NO_WINDOW` and applies a timeout.

## 3. Core interfaces (`navaja-core`) and the registry

```rust
pub trait Tool: Send + Sync + 'static {
    fn meta(&self) -> ToolMeta;                                        // called once, cached
    fn invoke(&self, action: &str, input: Value, ctx: &Ctx) -> Result<Value, ToolError>;
    fn detect(&self, _sample: &str) -> Option<u8> { None }            // reserved for smart detection/extensions
}
pub struct ToolMeta {
    pub spec_version: u16,                 // 1
    pub id: ToolId,                        // [a-z][a-z0-9_]*, == folder; "<publisher>__<name>" reserved for extensions
    pub name: String, pub description: String,   // English fallbacks for i18n
    pub category: Category,                // open newtype: ENCODERS, FORMATTERS, GENERATORS, SYSTEM
    pub keywords: Vec<String>, pub icon: String, // allowlisted SVG, rendered via CSS mask (never {@html})
    pub capabilities: Vec<Capability>,     // open: "process.inspect", "process.kill", "container.engine"
    pub actions: Vec<ActionMeta>,          // { id, label, destructive }
    pub tray: bool,                        // appears in the tray menu (no tray.rs change per tool)
    pub ui: UiSpec,
}
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiSpec { Transform(TransformSpec), Generator(GeneratorSpec), Custom { view: String }, #[serde(other)] Unsupported }
pub enum OutputKind { Text, Code { lang: String }, KeyValue, Diagnostics, Binary, #[serde(other)] Unsupported }
pub struct ToolError { pub code: ErrorCode, pub message: String, pub details: Option<Value> }
// codes: core.{unknown_tool, unknown_action, invalid_input, cancelled, panicked} | <id>.<snake>; messages never echo input
pub struct Ctx<'a> { /* cancel flag, throttled progress callback, capability-gated services */ }
```

```rust
// tools/lib.rs — one sorted line per tool; registry_test.rs fails if a folder is missing from the list
register_tools! {
    base64,
    hash,
    json,
    jwt,
    ports,
    url,
    uuid,
}
```

- **`Registry::new`** validates ids, spec references, categories and icons.
- **`Registry::run`** wraps `invoke` in `catch_unwind` and returns `core.panicked` when a tool panics. In debug builds it also checks each output against its payload type.
- **`search::rank`** orders matches by prefix, then word start, then substring, then subsequence.
- **Payload types** (`KeyValueRow`, `Diagnostic`, `BinaryValue`) derive ts-rs. `TS_RS_LARGE_INT=number` is set so that `u64` does not become `bigint`.

**`registry_test.rs` checks that:**
- the tool folders equal the sorted list;
- each id equals its folder name;
- `ui/View.svelte` and `ui/i18n/` exist exactly for `Custom { view: id }`;
- error codes match `<id>.<snake>`;
- every declared action, called with a pre-cancelled `Ctx` and a sentinel input, returns `core.invalid_input` or `core.cancelled`, so no side effect runs;
- an undeclared action returns `core.unknown_action`.

## 4. Extensibility contract

`cargo xtask tool-gate` enforces this contract on every PR that adds `tools/<id>/`.

| Change | May touch | Never touches |
|---|---|---|
| **Text tool** | `tools/<id>/**`, **one line** in `tools/lib.rs`, `tools/Cargo.toml`, `Cargo.lock` | Anything else, including any TypeScript |
| **System tool** | The text-tool set, plus target-specific dependencies, an optional new `crates/navaja-<x>/`, generated `app/src/bindings/**`, and `tools/<id>/ui/**` (custom view and i18n) | `app/src/{shell,generic,lib}`, `app/src-tauri`, `navaja-core`, `xtask` |
| **New capability, output kind or host service** | A separate, reviewed `host-change` PR | none |

- **Strings:** text goes through `t(key, fallback)`. Keys derive from the tool id, action, option and error code, and Rust's English text is the fallback, so a text tool needs no `.ts` edit. A custom view adds its own `tools/<id>/ui/i18n/en.ts`.
- **Custom views:** they import only `./`, `svelte`, `$lib/view-kit` and `$bindings/*`, enforced by an ESLint allowlist. The shell finds them with `import.meta.glob('@tools/*/ui/View.svelte')`.
  - **(S2.2)** If views outside `app/` turn out not to work, the fallback is `app/src/tools/<id>/`. That would be a brief deviation and needs sign-off.
- **Tool preferences** never become shell `Settings` fields. `Settings.tools`, a TOML table per tool id, is reserved until a tool needs it.

## 5. App shell (crate `navaja`)

**Commands.** Every command is async, declared in `AppManifest::commands`, granted individually and wrapped in catch-unwind:

| Command | Notes |
|---|---|
| `list_tools`, `search` | |
| `run_tool(tool, action, runId, input, progress)` | Runs under `spawn_blocking` and returns an `ipc::Response` envelope; progress travels over `ipc::Channel` |
| `cancel_run`, `copy_text` | |
| `settings_get`, `settings_set` | |
| `app_info`, `shell_ready`, `open_logs`, `quit` | |
| `open_url` | Only `https://github.com/joaovitorpina/Navaja/...` |
| `update_check`, `update_install` | Behind the `updater` feature (M6) |

Rust caps input sizes and never runs a destructive action from argv.

**Webview hardening.** This keeps the "fully offline" promise.

- **CSP:** `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`.
  - It also sets `dangerousDisableAssetCspModification: ["style-src"]`. Without it, Tauri's style nonces would disable the `'unsafe-inline'` that CodeMirror needs.
- **Capability:** it grants only the app's own commands, with no `core:default`, and the resolved ACL is snapshotted in `acl.lock.json`.
- **Plugins never used:** http, fs, shell, dialog, clipboard-manager and store.
- **Window:** one Rust builder creates it with:
  - incognito mode;
  - new windows denied;
  - hidden until ready;
  - no devtools in release builds.
- **`navaja-guard` plugin:** adds a navigation allowlist and an all-frames script that removes `RTCPeerConnection` and `RTCDataChannel` and replaces the native context menu.
- **WebView2 arguments:** any extra arguments must re-include wry's defaults.
- **Clipboard:** `copy_text` uses arboard with per-OS markers that keep copies out of clipboard history and cloud sync: Windows history and cloud clipboard, KDE's Klipper, and macOS.

**Plugins used:**
- Tauri core `tray-icon`;
- `single-instance`, registered first;
- `window-state`;
- `opener`;
- `updater` (M6);
- `global-shortcut` (a stretch goal).

Tests also load `tauri-plugin-wdio` and `tauri-plugin-wdio-webdriver`, behind the `e2e` feature and `e2e.conf.json`.

**Lifecycle**, implemented through `ShellPlatform`:
- **Startup:** the window stays hidden until `shell_ready`, and shows an error view after 5 s.
- **Close:**
  - Windows asks once whether to go to the tray or quit.
  - macOS hides the window. Dock reopen shows it, Cmd+Q quits, and the menu gains "Settings… ⌘,".
  - Linux hides only when a StatusNotifier host is registered, using a cached zbus check with a 200 ms timeout. Otherwise closing quits.
- **Hide policy (S2.5):** if destroying the webview saves at least 50 MB on Windows and a cold show takes at most 500 ms at the median, the policy is hybrid. The webview stays warm for 5 minutes after hiding and is then destroyed. Otherwise it always stays warm.
- **Tray menu:**
  - Open Navaja;
  - Search tools…;
  - every tool with `meta.tray`;
  - Check for updates (M6, direct installs only);
  - Quit.
- **Single instance:** forwards `--toggle`, `--show` and `--tool <id>`.
- **Elevated or root start:** shows a "not needed" banner. On Linux it requires `--allow-root`.

## 6. Front end

- **Layout:**
  - a category sidebar sorted by category order, then name;
  - a Home grid;
  - a 30-line hash router;
  - a Ctrl/Cmd+K palette that searches through Rust's `search`.
- **State:** `$state` holds small UI state. Outputs, port rows and big strings live in `$state.raw`. No web storage is used.
- **GenericToolView:** debounces input by 150 ms, cancels the previous run and drops stale results. One ConfirmDialog serves every destructive action.
- **CodeMirror 6**, through a 60-line `{@attach}`:

  | Input size | Editor behaviour |
  |---|---|
  | Up to 1 MB | Full language support, with diagnostics from Rust |
  | 1-10 MB | Plain text, with a Run button |
  | Over 10 MB | Kept out of the editor; a preview plus "Copy all" |

  **(S3.1)** tunes these thresholds.
- **Components:** Bits UI 2 with shadcn-svelte copies, Tailwind v4 tokens and system fonts.
- **Accessibility:** landmarks, F6 to cycle panes, Esc, a live region, and real tables with `aria-sort`.
- **Tests:** Vitest with `mockIPC` and axe, and WebdriverIO end-to-end tests on all three OSes **(S2.6)**.

## 7. Port inspector (`navaja-ports`, `navaja-docker`, `tools/ports`)

```rust
pub enum Field<T> { Value(T), Unavailable(Reason) }                 // the UI never shows a blank
pub enum Reason { NeedsElevation, NeedsRoot, OtherUser, Protected, Exited, NotSupported, QueryFailed }
pub struct ProcId { pub pid: u32, pub started_ms: i64 }            // identity = pid + start time
pub trait Platform: Send + Sync {
    fn sockets(&self, q: &SocketQuery) -> Result<Vec<Socket>, PortsError>;   // TCP default, UDP option, v4+v6
    fn process_table(&self) -> Result<ProcessTable, PortsError>;            // pid, ppid, start, image path
    fn process(&self, pid: u32, detail: Detail) -> ProcessInfo;             // exe, cmdline, user; cwd only for `why`
    fn findings(&self, port: u16, proto: Proto, facts: &Facts) -> Vec<Finding>;
    fn signal(&self, target: ProcId, sig: Signal) -> Result<Signalled, KillError>;
}
pub trait ContainerEngine: Send + Sync { fn published(&self) -> …; fn stop(&self, id: &str) -> …; } // navaja-docker, via ctx.service
```

**Mechanisms per OS:**

| OS | Sockets to PIDs | Process details |
|---|---|---|
| Windows | `GetExtendedTcpTable` and `GetExtendedUdpTable`, with the socket's `liCreateTimestamp` | Toolhelp, plus a PEB read for the command line and working directory |
| Linux | `/proc/net` and fd inodes (procfs) | `/proc/<pid>` |
| macOS | libproc; `pcblist_n` only if spike S4.2 passes | `sysctl KERN_PROC_PID` |

rustix provides signals and pidfd on Unix.

**From facts to text:**
- `gather()` collects `Facts`, and `explain()` is pure.
- `render::why_text()` returns a String. It reproduces both target outputs in `BRIEF.md` byte for byte, which the golden tests check.
  - The brief's `pd` is a placeholder, so the command prefix is a parameter. The golden tests pass `pd`, and the app names its Kill action instead.
- A reason is shown whenever a field is unavailable, and arguments that look like secrets are masked.

### Kill

1. A parent-to-child edge counts only if the child started at or after the parent.
2. R is the **topmost socket holder**. `plan_kill` is the dry run: it returns R and its descendants, deepest first, plus refusals.
3. `kill` builds the plan again from a fresh snapshot. It refuses on any mismatch (`ports.plan_stale`) and signals only the confirmed set.
4. Each process's identity (PID plus start time) is re-checked just before it is signalled.
5. Unix sends SIGTERM, waits up to 3 s, then sends SIGKILL. Windows uses `TerminateProcess`.
6. A rescan reports what was killed, gone or failed, any survivors, and whether the port is now free.

**Safety rules:**
- **Ancestors are never killed by default.**
  - User-space respawners (nodemon, `node --watch`, dotnet watch, watchexec, pm2) get a separately confirmed "Stop <supervisor>".
  - systemd units and Windows services get a copyable command instead.
- **Denylist**, matched by verified identity and image path:

  | Target | Navaja's response |
  |---|---|
  | PIDs 0, 1 and 4 | Refused |
  | Navaja itself, its ancestors and its webview children | Refused |
  | Critical system processes: csrss, wininit, winlogon, lsass, services, launchd, WindowServer, the compositor | Refused |
  | svchost | Redirected to Stop-Service |
  | Docker processes | Redirected to "Stop container" |
  | WSL processes | Explained |

- **Reused PIDs (Windows):** an owner PID created after the socket's timestamp counts as reused. `why` then looks for inheriting children; if there are none, the kill is refused.
- **Elevation in v1:** Navaja shows a classified reason plus a guarded copy command that checks the expected start time. A privileged helper comes after v1.

### Windows special cases (M5)

- **svchost:** mapped to service names.
- **PID 4:** a static table separates http.sys and URL ACLs, IIS, SMB on port 445 and NetBIOS on port 139.
- **No listener at all:**
  - `netsh` exclusions are parsed by digit rows only, so parsing works in any display language.
  - TCP probes bind `0.0.0.0` and `[::]` separately with `SO_EXCLUSIVEADDRUSE`.
  - The probes never call `listen()` and never touch UDP.
- **Verdict order:**
  1. socket owner;
  2. a port published by a container;
  3. wslrelay;
  4. iphlpsvc with a PortProxy rule;
  5. an exclusion plus a failed probe, meaning a reserved range;
  6. unknown.
- Fixes are explained, never performed.

### Docker (M5)

- **Endpoint order:** `DOCKER_HOST`, then the current context, then the well-known sockets.
- **Validator:** every endpoint goes through one check. It accepts `unix://` with an absolute local path, or `npipe://` with the local server `.`. Everything else is refused before any connection attempt.
- **Calls:** a 1 s timeout; list and stop only.
- **Stop container:** plan, confirm, then re-validate. It warns when the container uses AutoRemove.

## 8. Text tools, updater and distribution

| Tool | Crates | v1 behaviour |
|---|---|---|
| UUID | uuid 1.26 | v4 and v7, 1 to 10,000 at a time, four formats, upper-case option |
| JSON | Own tokenizer, with serde_json as the test oracle | Format, minify and validate in one pass. Tokens are kept exactly as written. Errors give the line and UTF-16 column |
| Base64 | data-encoding | Standard or URL-safe alphabet, with or without padding. Decoding accepts any variant. Non-UTF-8 output is shown as Binary, with "Copy hex" |
| URL | percent-encoding, form_urlencoded | Component and form encode and decode |
| Hash | md-5, sha1, sha2 0.11 | MD5, SHA-1, SHA-256 and SHA-512 at once, in hex |
| JWT | data-encoding, serde_json, jiff | Strips `Bearer`. Shows exp, nbf and iat as epoch, UTC and relative time, with a status. A JWE shows its header only. Always labelled "signature not verified" |

### Updater (M6, Rust only)

- **Plugin:** `tauri-plugin-updater`, with the minisign public key and the feed URL compiled in.
- **Integrity:** `requireSignedVersion` is on and downgrades are refused. The download URL must start with `https://github.com/joaovitorpina/Navaja/releases/download/v<ver>/`.
- **No api.github.com:** `xtask verify-release` rewrites the URLs in `latest.json` so that api.github.com is never contacted.
- **Consent:** asked once on direct installs, and closing the dialog counts as No. "Check now" always works.
- **The update dialog** shows the version and a sanitised changelog, and installs only after a click.
- **Privacy text** names github.com and release-assets.githubusercontent.com, and says GitHub sees the IP address and the time of the check.

### Install channel

| Channel | How it is detected |
|---|---|
| Dev | Debug builds |
| Managed | `NAVAJA_CHANNEL` is set |
| winget | A marker beside the exe, written by NSIS `/CHANNEL=winget` (`nsis/hooks.nsh`) |
| Scoop, portable zip | A marker beside the exe |
| Homebrew | `Caskroom/navaja` |
| Direct | Everything else |

Installs from a package manager show that manager's upgrade command instead of installing in-app.

### Bundles (unsigned in v1)

- **Windows:** a per-user NSIS installer with an explicit `bundle.publisher`, plus a portable zip.
- **macOS:** a universal DMG, ad-hoc signed.
- **Linux:** deb, rpm and AppImage, built in `container: ubuntu:22.04`.
- **Updater artifacts:** produced only through `tauri.release.conf.json`.

### Operations

- **Settings:** a typed TOML file in `APP_DIR`. Writes are atomic, and a corrupt file is set aside. Unix permissions are 0700 for the directory and 0600 for files.
- **Logs:** written with tracing, rotated daily, 7 kept. They record only the tool id, action, duration and error code, never payloads.
- **Crash files:** kept locally, and never contain the panic payload.

## 9. Runtime extensions after v1, and why v1 does not block them

```wit
package navaja:tool@1.0.0;
interface host { is-cancelled: func() -> bool; progress: func(done: u64, total: option<u64>); }
world tool { import host; export meta: func() -> string; export invoke: func(action: string, input: string) -> result<string, string>; export detect: func(sample: string) -> option<u8>; }
```

**The plan:**
- **Adapter:** a `WasmTool` adapter implements `Tool` on the wasmtime LTS line, behind the `extensions` feature.
- **Capabilities:** each maps to a host interface granted per extension. `wasi:sockets` and `wasi:http` are never linked.
- **Sandbox:** a trapping epoch deadline and `StoreLimits` apply, nothing is preopened, and Pulley is used on notarized macOS.
- **Files:** reach extensions only as Rust-minted `FileRef` tokens.
- **Views:** declarative first. Later, a sandboxed iframe with a postMessage bridge and an injected `CommandScope` on `run_tool`; `dynamic-acl` alone is not enough.

**What makes v1 ready for this:**
- the synchronous JSON trait;
- the versioned `ToolMeta`;
- the `Unsupported` variants;
- open `Category` and `Capability` types;
- the reserved id namespace;
- `Ctx` as the host surface.
