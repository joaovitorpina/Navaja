# Navaja architecture

This describes the v1 design accepted in [ADR 0001](adr/0001-stack.md): Tauri 2 with Svelte 5, all tool logic in Rust, and fully offline operation.

- [roadmap.md](roadmap.md) has the milestones, the spikes and how the work is verified.
- A marker such as **(S2.2)** refers to a spike there. A spike gates the work that comes after it.

## 1. Invariants

- **Rust owns** the registry, tool logic, search ranking, settings, logs, clipboard writes and the updater.
- **The webview** renders data and sends intents. It has no network, filesystem, dialog or shell permission.
- **Tools are pure and synchronous** (JSON in, JSON out). They never print, prompt, start a runtime, open a connection or receive a file path. In v1, text tools take pasted text; file input and output come after v1.
- **Generic commands** serve every tool. `UiSpec` renders every text tool. Navigation, search and tray entries all come from `list_tools()`.
- **OS-specific code sits behind traits**, one module per OS, or per OS family where Linux and macOS share the code: `navaja_ports::Platform` and the app's `ShellPlatform`.

## 2. Repository layout

```
Navaja/
├── Cargo.toml            virtual workspace: crates/*, tools, app/src-tauri, xtask · [workspace.lints]
├── rust-toolchain.toml · clippy.toml · deny.toml · release-plz.toml · renovate.json · .gitattributes (eol=lf)
├── package.json · pnpm-workspace.yaml   (pnpm 11 pinned via packageManager; workspace root lets Vite serve ../tools)
├── eslint.config.mjs · prettier.config.mjs · .prettierignore   (load app/'s configs from the root, so they also cover tools/*/ui)
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
│   ├── src/lib/          ipc · links · router · i18n · theme · copy · CopyButton · ToolIcon · view-kit/ (the only API custom views may import) · components/ui/ (shadcn copies)
│   ├── src/shell/        Shell · Sidebar · CommandPalette · ToolHost · Home · SettingsView · AboutView · OpenLogsButton · dialogs
│   ├── src/generic/      GeneratorView · TransformView · OptionControl · OutputView (one renderer per OutputKind)
│   ├── src/editor/       CodeMirror 6 {@attach} + size thresholds
│   ├── e2e/              wdio.conf.ts · strace-guard.sh (Linux) · support/ · specs/{launch,smoke,single-instance,egress,json,ports}.e2e.ts
│   ├── eslint/           config.js (ESLint) · view-imports.js (the custom-view import allowlist, §4) · their tests
│   └── src-tauri/        crate navaja · features: default [docker], updater (M6), e2e
│       ├── tauri.conf.json · tauri.release.conf.json (updater artifacts + pubkey) · e2e.conf.json
│       ├── capabilities/main.json · acl.lock.json · nsis/hooks.nsh
│       ├── src/          commands · state · paths · window · guard · tray · opener · hotkey · args · clipboard · settings · channel · updater · logging · crash · platform/{mod,unix,linux,macos,windows}.rs
│       └── tests/        privacy.rs (canary input kept out of logs and crash files)
├── xtask/                check · acl · tool-gate · bindings · icons · notices · capture-ports · measure · verify-release · manifests
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
  - **Test builds only:** hyper also comes in through axum, the WebDriver server inside `tauri-plugin-wdio-webdriver`. That plugin is part of the app's test-only `e2e` feature (§5), never of a release build, and cargo-deny allows axum under it alone.
- **Lints** ban printing, `exit`, `unsafe` outside FFI modules, socket and resolver calls, and `Command::new`.
  - **One exception:** the bind-only reserved-range probe in navaja-ports' Windows module (M5). It never calls `listen()` or `connect()`, and it is allowlisted in its FFI module.
  - **Process start:** processes start only through `navaja_core::sys::spawn_system`, which takes an absolute OS program path, sets `CREATE_NO_WINDOW` and applies a timeout. The one exception is tauri-plugin-opener, which hands a page to the browser or a folder to the file manager for `open_url` and `open_logs` (§5). `clippy.toml` bans its functions (`open_url`, `open_path`, `reveal_item_in_dir`, `reveal_items_in_dir` and `OpenerExt::opener`) everywhere but the app's `opener.rs`.

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
    pub category: Category,                // open id newtype: ENCODERS, FORMATTERS, GENERATORS, SYSTEM;
                                           // label and position are host-side (CategoryInfo)
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

- **`Registry::new`** validates ids, spec references, categories and icons. It reports every problem of every tool, not just the first. The id `core` is reserved for host error codes.
  - A `Generator` action may not be `destructive`: the generator view runs on one click, with no confirmation step.
- **`Registry::categories()`** returns the categories in use, as `CategoryInfo { id, label, order }`. The labels and positions come from the host's built-in table; unknown ids sort last, by id.
- **`Registry::run`** wraps `invoke` in `catch_unwind` and returns `core.panicked` when a tool panics.
  - **Panic hook:** the panic payload is never returned. The app must also replace the default panic hook before anything else can panic (first thing in `navaja_lib::run`, see §8): the default hook prints the payload, which may contain input, to stderr.
  - **Debug checks:** with debug assertions (the default for `cargo test`), each output must round-trip exactly through its payload type. A tool may return only its own `<id>.*` codes plus `core.invalid_input`, `core.cancelled` and `core.panicked`.
  - **Panic strategy:** a `compile_error!` stops the build if anyone switches it to `abort`.
  - **Tests:** tool tests use `navaja_core::run_single`, which runs through the same checks.
- **`search::rank`** orders matches by prefix, then word start, then substring, then subsequence.
- **Payload types** (`KeyValueRow`, `Diagnostic`, `BinaryValue`) derive ts-rs. `TS_RS_LARGE_INT=number` is set so that `u64` does not become `bigint`.

**`registry_test.rs` checks that:**
- the tool folders equal the sorted list;
- each id equals its folder name;
- `ui/View.svelte` and `ui/i18n/` exist exactly for `Custom { view: id }`;
- error codes match `<id>.<snake>`;
- every declared action, called with a pre-cancelled `Ctx` and a junk input, returns `core.invalid_input` or `core.cancelled` (tools parse with `typed()` and call `ctx.check()` before any side effect);
- the generic views' inputs are accepted: defaults, every choice and both integer bounds;
- every `ErrorCode::from_static` literal in a tool's folder is in the tool's namespace;
- an undeclared action returns `core.unknown_action`.

## 4. Extensibility contract

`cargo xtask tool-gate` enforces this contract on every PR that adds `tools/<id>/`.

| Change | May touch | Never touches |
|---|---|---|
| **Text tool** | `tools/<id>/**`, **one line** in `tools/lib.rs`, `tools/Cargo.toml`, `Cargo.lock` | Anything else, including any TypeScript |
| **System tool** | The text-tool set, plus target-specific dependencies, an optional new `crates/navaja-<x>/`, generated `app/src/bindings/**`, and `tools/<id>/ui/**` (custom view and i18n) | `app/src/{shell,generic,lib}`, `app/src-tauri`, `navaja-core`, `xtask` |
| **New capability, output kind or host service** | A separate, reviewed `host-change` PR | none |

- **Strings:** text goes through `t(key, fallback)`. Keys derive from the tool id, action, option and error code, and Rust's English text is the fallback, so a text tool needs no `.ts` edit. A custom view adds its own `tools/<id>/ui/i18n/en.ts`.
- **Custom views** live in `tools/<id>/ui/`. The shell finds them with `import.meta.glob('@tools/*/ui/View.svelte')`.
  - **Imports:** the ESLint rule `navaja/view-imports` (`app/eslint/view-imports.js`) applies to every script file under `tools/*/ui/`. A view may import only:
    - relative paths that resolve inside its own `tools/<id>/ui/` folder, written without back slashes or percent escapes;
    - `svelte` and its browser subpaths, such as `svelte/store`, `svelte/transition` and `svelte/elements`. Not `svelte/internal` or anything under it, and not `svelte/compiler` or `svelte/server`;
      - the rule lists these subpaths, and a test fails when a Svelte update adds one, so each new one gets a decision;
    - `$lib/view-kit`, exactly;
    - `$bindings/<name>` and `$bindings/<dir>/<name>`, with no `.` or `..` segments.
  - **Tests:** a view's `*.test.ts` and `*.spec.ts` files may also import `vitest`, `@testing-library/svelte` and `@tauri-apps/api/mocks`, and nothing else.
  - **Forms checked:** static, type-only and side-effect imports, `export … from`, `import()` and `require()`, TypeScript's `import x = require()` and `typeof import()`, and the first argument of `new URL(x, import.meta.url)`. Vite bundles that file as an asset or a worker and resolves a bare `x` through its aliases, so `x` must be allowed like an import, and an absolute URL is refused too. A computed specifier is an error, and `import.meta.glob` is not allowed.
  - **No opt-out:** a view can't switch rules off with a comment (`noInlineConfig`, plus markup `<!-- eslint-disable -->`). `pnpm lint` runs ESLint from the repository root with `--config`, so a config file placed under `tools/` is never used. CI runs `pnpm lint` in the checks job.
  - **(S2.2)** If views outside `app/` turn out not to work, the fallback is `app/src/tools/<id>/`. That would be a brief deviation and needs sign-off.
- **Tool preferences** never become shell `Settings` fields. `Settings.tools`, a TOML table per tool id, is reserved until a tool needs it.

## 5. App shell (crate `navaja`)

**Commands.** Every command is declared in `AppManifest::commands` and granted individually.
- **Threads:** commands that touch the OS or the disk (`run_tool`, `copy_text`, `settings_set`, `open_logs`, `open_url`) run off the main thread. The rest are cheap (in memory, or a few window calls) and run inline on the main thread.
- **Panics:** `Registry::run`'s `catch_unwind` contains a tool panic, and `copy_text` wraps its clipboard call in its own.

| Command | Notes |
|---|---|
| `list_tools`, `search` | |
| `run_tool(tool, action, runId, input, progress)` | Runs under `spawn_blocking` and returns an `ipc::Response` envelope; progress travels over `ipc::Channel` |
| `cancel_run`, `copy_text` | |
| `settings_get`, `settings_set` | |
| `app_info`, `shell_ready` | |
| `open_logs` | Shows `APP_DIR/logs` in the file manager, creating it (privately) if it is missing. An error if Navaja has no `APP_DIR` |
| `quit` | The tray's Quit path, so the `RunEvent::Exit` clean-up runs. Logs whether the tray or the command asked |
| `open_url(url)` | Only the URLs Navaja links to, compared exactly (below) and then handed to the browser unchanged |
| `update_check`, `update_install` | Behind the `updater` feature (M6) |

**`open_url`'s rule.** The URL must equal, byte for byte, one entry of `ALLOWED_URLS` in `opener.rs`: the URLs the front end links to. Today that is only the repository, `https://github.com/joaovitorpina/Navaja`.
- Pages under the repository are refused too. GitHub serves a fork's commits under the parent's URLs (`/raw/<sha>/…`, `/archive/<sha>.zip`), so a rule for paths would let a page open content anyone can push.
- A new link adds its exact URL to the list.
- A refusal is logged without the URL, not even its scheme.

**Hand-offs to other programs.** `open_url` and `open_logs` call tauri-plugin-opener's free functions, through `opener.rs`, the one module the lints let call them (§2). The plugin is never registered, so its own commands and its link-click script do not exist. Each hand-off runs on a short-lived thread, because opening a folder on Windows initialises COM on the calling thread.
- **Windows:** a URL goes through `ShellExecuteExW`, so a browser it starts can be Navaja's child process. A folder goes through `SHOpenFolderAndSelectItems`, which Explorer opens in its own process.
- **macOS:** `/usr/bin/open` runs briefly as Navaja's child and asks LaunchServices, so launchd, not Navaja, starts the browser or Finder.
- **Linux:** `xdg-open` (or gio, gnome-open, kde-open) starts after a double fork and `setsid`. It leaves Navaja's process tree by parentage, but a tracer that follows forks, such as `strace -f`, still follows it and the browser.
  - **Failures after the start go unseen:** tauri-plugin-opener, through the `open` crate, returns success as soon as the launcher has been executed, without waiting for it. A launcher that then fails, with no default browser set for example, shows no alert and logs nothing.
- So the offline checks (the strace guard, the S2.7 capture) must never click these links, and no end-to-end test opens a URL or the logs folder.
- **A failed hand-off** logs the error's kind and the OS error code (an HRESULT in hex), never the error's text, which can quote the URL or the path.

**Input limits.** Rust checks what the webview sends:
- `run_tool` refuses malformed tool and action ids, and logs only ids the registry knows (`<unknown>` otherwise). It caps `input` at 64 MiB of string data (object keys included) and 1,000,000 JSON nodes, and returns `core.invalid_input` above that, without echoing the input. Tauri has already parsed the IPC body by then, so the cap bounds tool work, not IPC memory.
- Run ids are 1 to 64 ASCII letters, digits or `-`.
- `search` reads at most 200 characters of the query.
- `copy_text` refuses text over 64 MiB.
- `open_url` refuses any URL that is not on its list (`open_url`'s rule above).

Rust never runs a destructive action from argv.

**Webview hardening.** This keeps the "fully offline" promise.

- **CSP:** `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`.
  - It also sets `dangerousDisableAssetCspModification: ["style-src"]`. Without it, Tauri's style nonces would disable the `'unsafe-inline'` that CodeMirror needs.
- **Response headers**, set in `security.headers` in `tauri.conf.json`:
  - `Permissions-Policy` denies the camera, microphone, geolocation, display capture, USB, serial, HID, Bluetooth, MIDI, payment, WebAuthn (`publickey-credentials-get`), the screen wake lock, and clipboard read and write through the async Clipboard API;
  - `X-Content-Type-Options: nosniff`.
- **DNS prefetch:** `index.html` sets `x-dns-prefetch-control: off`, so the engine makes no speculative lookups for links.
- **No external links:** the page holds no external `href`. About's repository link is a button that asks `open_url`, so a middle-click, a prefetch or a navigation has nothing to follow.
- **Capability:** it grants only the app's own commands, with no `core:default` and no plugin permission.
  - **ACL snapshot:** `app/src-tauri/acl.lock.json` holds the resolved ACL of the release configuration: `tauri.conf.json`'s capabilities with the default features, not the e2e overlay. It lists every command the ACL lets the webview call, with its windows, webviews, origins and scopes, plus denied commands and global scopes, for Linux, macOS and Windows.
  - **The one exemption:** Tauri lets `plugin:__TAURI_CHANNEL__|fetch` skip the ACL, so no capability grants it and the lock cannot list it; its `$comment` names it. `ipc::Channel` uses it for large payloads, such as `run_tool`'s progress, and it only returns data queued for the calling webview.
  - **Updating it:** `cargo xtask acl` resolves it with Tauri's own code (tauri-utils, pinned to tauri's version) from what the app's build script writes, and rewrites the file. `cargo xtask check` resolves it again and fails, naming each command that differs, until the new file is committed. So a new grant is reviewed as a diff.
  - **What it covers:** `tauri.conf.json` alone (the capabilities it names, and any inline ones), every capability file tauri-build finds under `capabilities/`, and the permission manifests of the app (from `build.rs`) and of every plugin it depends on when built with its default features (so not the `e2e` plugins). It does not cover the e2e overlay or the `TAURI_CONFIG` variable the Tauri CLI passes it in; only test builds use them.
  - **What `cargo xtask check` refuses,** because the build would read it and the snapshot would not:
    - a per-platform config beside `tauri.conf.json`: `tauri.<os>.conf.json`, `tauri.<os>.conf.json5` or `Tauri.<os>.toml`, for linux, macos, windows, android or ios. Tauri merges it over `tauri.conf.json` when it builds for that OS;
    - a `generate_context!` call with arguments, such as a config path or `capabilities = [...]`, in any file under `app/src-tauri/src`, or a renamed import of the macro.
  - **Runtime grants:** Tauri's default `dynamic-acl` feature compiles in `Manager::add_capability`, which adds a capability after start. `clippy.toml` bans it.
- **Plugins never used:** http, fs, shell, dialog, clipboard-manager and store.
- **Window:** one Rust builder creates it with:
  - incognito mode;
  - new windows denied;
  - hidden until ready;
  - no devtools in release builds.
- **`navaja-guard` plugin:**
  - **Navigation allowlist:** only this OS's app origin, `tauri://localhost` on macOS and Linux and `http://tauri.localhost` on Windows. The Vite dev server is allowed only when `tauri::is_dev()` is true. A blocked navigation is logged by its scheme, never its URL.
  - **Init script** (`guard.js`), injected before any page script in each frame the engine covers. It removes `RTCPeerConnection`, `RTCDataChannel` and the other WebRTC constructors, whose ICE and STUN traffic the CSP does not govern. It also suppresses the native context menu (reload, inspect) outside text-entry fields. Inside them the native menu stays, for cut, copy and paste.
  - **One layer, not a guarantee.** The script is a layer under the CSP, and a frame it never runs in keeps the constructors.
    - WebView2 runs it in every frame, empty and `srcdoc` iframes included.
    - WebKitGTK ships with WebRTC disabled (`enable-webrtc` defaults to false, and wry leaves it).
    - On macOS, WebKit does not inject the script into `srcdoc` iframes, which the egress canary showed on its first macOS run. So macOS also turns peer connections off in the engine: `platform/macos.rs` calls `WKPreferences`' private `_setPeerConnectionEnabled:` (checked with `respondsToSelector:` first) right after the window is built, which covers every frame created afterwards. The end-to-end egress canary checks the page and four kinds of iframe on every OS.
  - **macOS text fields:** the native menu there also offers OS services: Look Up, Translate, Search With Google, Share and Services. They send the selected text only when the user picks one. `privacy.md` discloses them (roadmap M6, item 6).
- **Dead proxy (Linux):** outside dev mode, the webview's web traffic goes to `http://127.0.0.1:9`, a closed privileged loopback port. WebKitGTK looks a link's host up while it waits for the navigation decision, even when the guard then denies the navigation; the strace guard caught those lookups on the first Linux run of the navigation canary. Through the proxy nothing is resolved and nothing leaves the machine. The strace guard reports connections that stop there instead of failing on them. Windows and macOS have no proxy yet; S2.7's capture checks them.
- **WebView2 arguments:** any extra arguments must re-include wry's defaults.
- **Clipboard:** `copy_text` writes through arboard and always sets the OS's exclusion markers; there is no opt-out. A native copy (Ctrl/Cmd+C, the context menu) whose selection touches a region marked `data-output` goes through it too, wherever the selection starts; copies from text fields stay native, since they hold the user's own input. What the markers achieve:
  - **Windows:** the copy stays out of clipboard history (Win+V), cloud clipboard sync and clipboard monitors.
  - **Linux** (X11 and Wayland): `x-kde-passwordManagerHint: secret` keeps it out of Klipper and other history managers that honour the hint.
  - **macOS:** `org.nspasteboard.ConcealedType` keeps it out of history apps that follow nspasteboard.org. It still reaches Universal Clipboard on the user's own nearby devices; opting out with `currentHostOnly` is an M2b item.

**Plugins used:**
- Tauri core `tray-icon`;
- `single-instance`, registered first. Known issue: on macOS it hands off through a world-shared socket in `/tmp` with no peer check; the fix is roadmap M2b, item 9;
- `window-state`;
- `opener`, from Rust only and never registered (see "Hand-offs to other programs" above);
- `updater` (M6);
- `global-shortcut` (a stretch goal).

**End-to-end builds** (`--features e2e --config src-tauri/e2e.conf.json`) also load `tauri-plugin-wdio` and `tauri-plugin-wdio-webdriver`. They never reach a release. `cargo xtask check` fails if the default features reach `e2e` or either plugin, directly or through other features, if a feature other than `e2e` names a plugin, or if a plugin stops being optional. It also fails if `e2e.conf.json` overrides anything other than the identifier and the capabilities, so the tests run the shipped CSP and keep `withGlobalTauri` off. Each run is kept apart from a Navaja the developer already uses:
- **Own identifier:** `io.github.joaovitorpina.Navaja.e2e`. An installed copy or a `pnpm tauri dev` instance holds a different single-instance lock and webview profile, so it cannot take over the test launch.
- **Capability:** the overlay adds `wdio:default` and nothing else. There is still no `core:default`, so the app's own commands run under the same grants as in production.
- **WebDriver server:** it listens on 127.0.0.1:4445 (or `TAURI_WEBDRIVER_PORT`) with no authentication, so the app starts it only when a test harness launched it: `@wdio/tauri-service` sets `WDIO_EMBEDDED_SERVER`, and any other harness sets `NAVAJA_E2E_WEBDRIVER`. An e2e binary started by hand (it shares `target/debug/navaja` with ordinary debug builds) opens no port. The harness fails fast when the port already accepts connections, instead of driving whatever holds it, such as an app left over from an aborted run.
- **App directory:** the harness always points `NAVAJA_APP_DIR` at a fresh temporary directory, even when the developer has exported one, so a run never reads or writes real settings and logs.

**Lifecycle**, implemented through `ShellPlatform`:
- **Startup:** the window stays hidden until `shell_ready`, and shows an error view after 5 s. A second launch or a tray action that comes earlier waits for it: the requested tool is kept and opened once the shell is ready, so no blank window is shown.
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
- **Tray icon:**
  - Windows: a left click toggles the window, and a right click opens the menu. The click takes focus from the window before it arrives, so the toggle treats the window as focused if it had focus in the last 500 ms.
  - macOS: a template icon, tinted by the system; a click opens the menu.
  - Linux: the menu only, since click events do not arrive. When no appindicator library loads, there is no tray: a warning is logged and the app runs without it.
- **Single instance:** forwards `--toggle`, `--show` and `--tool <id>`.
  - It is keyed on the app identifier. A dev or debug launch while another Navaja runs, an installed one included, hands its arguments to that instance and exits with code 0. End-to-end builds use their own identifier, set in `e2e.conf.json`.
  - **Wayland:** a second launch cannot yet raise a window that is visible but unfocused, because the launcher's activation token is not forwarded (roadmap M2b, item 8). Tray-menu actions cannot take focus there at all.
- **Elevated or root start:** shows a "not needed" banner. On Linux and macOS, Navaja refuses to start as root unless given `--allow-root`.

## 6. Front end

- **Layout:**
  - a category sidebar sorted by category order, then name;
  - a Home grid;
  - a 30-line hash router;
  - a Ctrl/Cmd+K palette that searches through Rust's `search`.
- **State:** `$state` holds small UI state. Outputs, port rows and big strings live in `$state.raw`. No web storage is used.
- **Generic views:** `ToolHost` picks the view from the `UiSpec`. GeneratorView runs on demand, and TransformView (M3) debounces input by 150 ms. Both cancel the previous run and drop stale results. One ConfirmDialog serves every destructive action.
- **CodeMirror 6**, through a 60-line `{@attach}`:

  | Input size | Editor behaviour |
  |---|---|
  | Up to 1 MB | Full language support, with diagnostics from Rust |
  | 1-10 MB | Plain text, with a Run button |
  | Over 10 MB | Kept out of the editor; a preview plus "Copy all" |

  **(S3.1)** tunes these thresholds.
- **Components:** Bits UI 2 with shadcn-svelte copies, Tailwind v4 tokens and system fonts.
- **Accessibility:** landmarks, F6 to cycle panes, Esc, a live region, and real tables with `aria-sort`.
- **Lint:** `pnpm lint` runs ESLint (the recommended JavaScript, TypeScript and Svelte rules, plus the custom-view allowlist from §4) and Prettier over `app/` and `tools/*/ui/`. `pnpm format` applies Prettier.
- **Tests:** Vitest with `mockIPC` and axe, and WebdriverIO end-to-end tests on all three OSes **(S2.6)**.
  - The end-to-end tests drive the real debug app through its embedded WebDriver server (§5). On Linux they run under the strace network guard (roadmap §2).
  - axe is not wired in yet. It joins Vitest in M3 (roadmap M3, item 3).

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
- **`NAVAJA_APP_DIR`:** replaces `APP_DIR` in debug builds only, so tests use a throwaway directory; the end-to-end harness always sets it to a fresh temporary one (§5). Release builds ignore it.
- **Logs:** written with tracing to `APP_DIR/logs`, rotated daily, 7 kept. They record only the tool id, action, duration and error code, never payloads. Settings and the start-failure view open the folder through `open_logs`.
- **Crash files:** `APP_DIR/crash`, 10 kept, local only. They never contain the panic payload.
- **Panic hook:** the app replaces the panic hook first thing in `navaja_lib::run`, once it knows `APP_DIR`. The hook records the location, the thread and a backtrace (symbols and source paths only), and it never reads the payload. It also never chains to the default hook, which would print the payload to stderr (journald on Linux).

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
