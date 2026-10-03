# ADR 0001: Stack, distribution and identifiers

- **Status:** Accepted
- **Date:** 2026-10-02
- **Decider:** João Vitor Pina

## Context

`BRIEF.md` fixes Rust for all tool logic, support for Windows, Linux and macOS from the first commit with CI on all three, fully offline operation, and the MIT licence. It left several things open:
- the UI approach;
- the front-end framework;
- tray and global-shortcut support;
- packaging and updates;
- the port-inspector crates;
- the path to runtime extensions;
- the updater mechanism and the cost of code signing.

The brief's criteria, in order, are:
1. runtime performance and memory use;
2. maturity and maintenance of the dependencies;
3. tooling and developer experience;
4. how little code a new tool needs;
5. startup time and installer size.

The options were researched on 2026-10-02 in nine tracks, each adversarially fact-checked against primary sources, and then scored by three independent judges. The judges each applied a different lens: the criteria strictly in order, the long-term maintainer's view, and v1 delivery risk.

## Decision

### UI: Tauri 2 + Svelte 5

Every memory figure below is an estimate; Navaja has not been measured yet. Spike S2.5 in M2b measures it.

| | **Tauri 2.12 + Svelte 5** (chosen) | Slint 1.18 | egui 0.36 |
|---|---|---|---|
| 1. Performance and memory | An estimated 80-200 MB with the window open on Windows, across every WebView2 process (browser, GPU, renderer, utility). The webview can be destroyed while hidden in the tray | One process; an estimated 30-50 MB, from a single anecdote | About 135 MB (an anecdote). TextEdit re-layout is O(N) on multi-MB text |
| 2. Maturity | Very active, with first-party tray, updater, single-instance and shortcut plugins. v3 is in alpha, so a migration comes after v1 | Company-backed 1.x. Its packager (cargo-packager) has had no release since 2025-11 | Breaking release every quarter |
| 3. Tooling and DX | Vite HMR, Vitest, WebDriver end-to-end tests on all three OSes, a JetBrains Svelte plugin. Two toolchains | Live preview and a DSL; the JetBrains plugin is third-party | Plain Rust |
| 4. Code per new tool | 0 UI lines, using the generic schema-driven view | 0 UI lines | One `fn ui` per tool |
| 5. Startup and installer | About 0.5 s; installers of 3-10 MB (the AppImage is about 70 MB) | About 0.2 s; small installers | About 18 MB |
| Large-JSON editor | CodeMirror 6, virtualised | Must be built in-house: there is no rich TextEdit (slint#2723) | Weak |
| Licence of the shipped app | MIT/Apache, plus some MPL-2.0 | GPL-3.0 "as a whole", or Slint's proprietary royalty-free licence with attribution | MIT/Apache |
| Judges (strict order / maintainer / delivery risk) | 2 / **1** / **1** | **1** / 2 / 2 | 3 / 3 / 3 |

**Why Tauri, although Slint leads on criterion 1:**
- **The MIT licence is a fixed requirement.** Slint's FAQ states that linking its GPLv3 option makes "the work as a whole" GPL. Its royalty-free licence is not OSI-approved.
- **Free Windows signing.** SignPath Foundation requires "OSI-approved … without commercial dual-licensing for all components", and Slint fails that under either licence. SignPath is the only free route for the NSIS and zip builds Tauri produces. The Store's free MSIX route is closed to Tauri, which cannot produce MSIX (tauri#4818).
- **Delivery risk.** Tauri gives the two hardest v1 parts off the shelf: an editor that handles 1-10 MB of JSON, and a signed updater with installers on three OSes.
- **Memory is bounded and will be measured.** It is limited by the hide policy (S2.5). A bad number changes that policy, not the stack.

**Rejected:**
- Dioxus desktop: the same webview costs, with weaker distribution and no updater.
- Tauri + Leptos: adds a WASM payload, a slow rebuild loop and a thin component kit.
- GPUI + gpui-kit: depends on a GPUI snapshot republished by a single maintainer.
- iced: no 2026 release and no accessibility.
- Makepad, Floem, Xilem, Freya and Blitz: immature.
- Qt and GTK: licence and cross-platform friction.

### Decisions per open item

| Open item | Decision |
|---|---|
| **Front-end framework** | **Svelte 5 in TypeScript, as a plain Vite SPA with no SvelteKit.**<br>- Components: Bits UI 2 with shadcn-svelte copied into the repo, and Tailwind v4.<br>- Editor: CodeMirror 6 through a small `{@attach}`. Monaco is rejected.<br>- Types: generated from Rust with ts-rs, and CI fails on drift. tauri-specta stays out while it is a release candidate.<br>- Package manager: pnpm 11, pinned through `packageManager`, because npm's `latest` now resolves to the pnpm 12 rewrite.<br>- TypeScript: pinned to ~6.0.3, because TS 7 is not supported yet by svelte-check or typescript-eslint.<br><br>Alternatives:<br>- Solid is mid-move from 1.9 to 2.0 RC, and Kobalte is in alpha.<br>- Vue Vapor is still an RC.<br>- React costs more runtime and ceremony. |
| **Tray** | Tauri's core tray, built on tray-icon and muda.<br>- Linux: menu only, because click events do not fire there. Close-to-tray applies only when a StatusNotifier host is registered; otherwise closing quits.<br>- macOS: a template icon.<br>- Windows: a left click toggles the window. |
| **Global shortcut** | `tauri-plugin-global-shortcut`, built on global-hotkey 0.8, on Windows, macOS and X11. It is an M2b stretch goal.<br><br>**Recorded exception:** in Wayland sessions Navaja registers no global shortcut. GNOME 49+ and Plasma 6.8 are Wayland-only. The docs show how to bind a desktop shortcut to `navaja --toggle`, which the single-instance plugin forwards.<br><br>The XDG GlobalShortcuts portal comes after v1, through ashpd or through global-hotkey PR #172. |
| **Installers** | The Tauri bundler, all unsigned for v1:<br>- Windows: a per-user NSIS installer, plus a portable zip for Scoop.<br>- macOS: a universal DMG, ad-hoc signed.<br>- Linux: .deb, .rpm and AppImage, built in a `container: ubuntu:22.04` job on `ubuntu-24.04` so the glibc baseline is 2.35.<br><br>**No Flatpak or Snap:** their sandboxes hide host processes, and Flathub rejects host system utilities. **No MSI** unless enterprise users ask for one. |
| **Package managers** | - **winget:** the first PR by hand, then winget-releaser.<br>- **Scoop:** our own bucket; we move to Extras at about 100 stars or 50 forks.<br>- **Homebrew:** our own tap, installed with the fully qualified `brew install --cask joaovitorpina/tap/navaja`, since taps must be trusted explicitly from Homebrew 6. The official cask requires notarized builds, which is post-v1.<br>- **AUR:** left to the community. |
| **Updates** | `tauri-plugin-updater` 2.13, called from Rust only. It updates deb, rpm and AppImage since 2.10.<br>- Signatures are minisign and cannot be turned off, and `requireSignedVersion` is on, so downgrades are refused.<br>- The feed is a static `latest.json` on GitHub Releases.<br>- The download URL must start with this release's GitHub download path.<br>- The check is opt-in: Direct installs are asked once, and closing the dialog counts as No. "Check now" always works.<br>- The dialog shows the version and a sanitised changelog, and installs only after a click.<br><br>The privacy text says what reaches GitHub: github.com and release-assets.githubusercontent.com see the IP address and the time of the request, plus, when an update is downloaded, which platform's artifact was fetched. No user or machine identifiers are sent.<br><br>Managed installs show that package manager's upgrade command instead:<br>- winget and Scoop installs are detected by a marker file beside the exe;<br>- Homebrew installs by the `Caskroom/navaja` path. |
| **Memory while hidden** | Spike S2.5 measures the whole process tree.<br>- If destroying the webview saves at least 50 MB on Windows and a cold show takes 500 ms or less at the median, the policy is hybrid: warm for 5 minutes after hiding, then destroyed.<br>- Otherwise the webview always stays warm.<br><br>A bad number changes this policy, not the stack. ADR 0002 records the measurements. |
| **Port-inspector protocols** | TCP by default, with UDP as an option. IPv4 and IPv6 are merged in one list. |
| **Port-inspector crates** | Crates by role:<br>- Windows: windows-sys 0.61, covering IpHelper (`GetExtendedTcpTable` and `GetExtendedUdpTable`), ToolHelp, Threading, Services, HttpServer and Security.<br>- Linux: procfs 0.18.<br>- macOS: libproc 0.14 plus libc `sysctl`.<br>- Signals on Unix: rustix 1.1.<br>- Docker: bollard 0.21 with only the `pipe` feature.<br><br>`sysinfo`, `netstat2` and `listeners` are not used in production, because they hide permission failures. `listeners` serves as a CI test oracle, and jkfran/killport (MIT) as reference code. |
| **Docker detection** | Through the **engine socket or named pipe, not the CLI.** Navaja tries `DOCKER_HOST`, then the current context, then the well-known sockets. Every endpoint passes one validator, which accepts only `unix://` with an absolute local path or `npipe://` with the local server `.`. `tcp://`, `ssh://`, `https://` and remote pipes are refused before any connection attempt. |
| **Runtime extensions (post-v1)** | **Model:** WebAssembly components on the wasmtime LTS line. A synchronous WIT world `navaja:tool@1` mirrors the v1 `Tool` trait, with JSON in and out and the same declarative UiSpec, as in DevToys. Extensions therefore reuse the registry and the generic view.<br><br>**Sandbox:**<br>- `wasi:sockets` and `wasi:http` are never linked;<br>- capabilities are granted per extension;<br>- each extension gets a trapping epoch deadline and `StoreLimits`;<br>- no directories are preopened;<br>- Pulley runs on notarized macOS, since the hardened runtime breaks the Cranelift JIT (wasmtime#11989).<br><br>**Rejected:** native dynamic libraries, and Extism, which pins an out-of-support wasmtime. |
| **Tool registry** | An explicit `register_tools!` list in `tools/lib.rs`, one sorted line per tool. A test compares the folders on disk with the list. `inventory` and `linkme` are rejected: their registration order is unspecified and they have open linker issues. A build.rs scan is rejected because it hides the registry from the IDE. |

### Code signing cost per year (checked 2026-10-02)

| Route | Cost | Notes |
|---|---|---|
| Apple Developer Program | $99 | Needed for notarization. Since 2026-09-01, official Homebrew casks must pass Gatekeeper |
| SignPath Foundation (Windows) | $0 | Needs OSI licences for all components, existing releases, an active project, team roles with MFA, and a code-signing policy on the project home page (team roles, privacy statement, SignPath attribution). Signs as "SignPath Foundation" |
| Microsoft Store MSIX | $0 | Microsoft re-signs the package. Tauri cannot produce MSIX (tauri#4818) |
| Certum Open Source | €49-69 | Out of stock on 2026-10-02; signing is manual |
| SSL.com IV | $129, or about $369 with eSigner for signing in CI | Paid fallback |
| Azure Artifact Signing | About $9.99 a month | Individuals in the US or Canada only |
| EV certificate | n/a (rejected) | No longer bypasses SmartScreen |

**v1 spends $0.** All builds ship unsigned, and macOS builds are ad-hoc signed.

What users will see:
- **Windows:** a SmartScreen warning ("More info → Run anyway"). Its reputation resets with every release. Smart App Control blocks the installer where it is on.
- **macOS:** Gatekeeper's "Open Anyway" under Privacy & Security.

**Mitigations:**
- build-provenance attestations;
- a minisign-signed `SHA256SUMS`;
- a Defender file submission for each release;
- `docs/install.md` walkthroughs.

**After v1:** apply to SignPath Foundation once public releases exist, then consider Apple's $99 and a Store MSIX.

### Frozen identifiers

Each of these is defined here and must match everywhere else.
- **Checked by CI:** `scripts/check-identifiers.sh` enforces the repository URL, app identifier, updater feed, winget PackageIdentifier, Homebrew tap, Scoop bucket and publisher.
- **Checked in review:** the rest.

Changing an identifier after the first release is expensive, especially the winget publisher.

| Identifier | Value |
|---|---|
| Repository | `https://github.com/joaovitorpina/Navaja` |
| Product name | `Navaja` |
| Executable | `navaja` (`navaja.exe` on Windows) |
| App identifier (bundle id, desktop file, AUMID) | `io.github.joaovitorpina.Navaja` |
| Publisher (NSIS, deb, winget) | `João Vitor Pina` |
| Updater feed | `https://github.com/joaovitorpina/Navaja/releases/latest/download/latest.json` |
| Release download prefix | `https://github.com/joaovitorpina/Navaja/releases/download/v<version>/` |
| Settings and log directory (`APP_DIR`) | `%APPDATA%\Navaja` (Windows), `$XDG_CONFIG_HOME/navaja` (Linux, default `~/.config/navaja`), `~/Library/Application Support/Navaja` (macOS). Never derived from the app identifier |
| winget | PackageIdentifier `JoaoVitorPina.Navaja`, moniker `navaja` |
| Homebrew | tap repo `joaovitorpina/homebrew-tap`, cask `navaja`, installed with `brew install --cask joaovitorpina/tap/navaja` |
| Scoop | bucket repo `joaovitorpina/scoop-bucket`, app `navaja` |
| Rust crates | `navaja` (app), `navaja-core`, `navaja-ports`, `navaja-docker`, `navaja-tools` (all `publish = false`) |

**Name check (2026-10-02):**
- "Navaja" is free on crates.io, winget, Homebrew (formulae and casks), Scoop, Flathub, AUR and npm.
- The GitHub login `navaja` is taken, so the repository stays under `joaovitorpina`.
- A knockout search on 2026-10-02 (USPTO, EUIPO, Brazil's INPI, and Spanish data via TMview, since OEPM's own search sits behind a CAPTCHA) found no identical class 9 mark "NAVAJA".
  - Similar marks and class 42 were not fully checked.
  - This is not legal advice.
  - Run a TMview search for classes 9 and 42 before v1.0.0 (roadmap M6).
- A future CLI must not be called `pd`, which collides with Homebrew's `pd` cask and the `pd` crate.
- Navaja is never marketed as a "Swiss Army knife".

### Pinned toolchain

- **Rust:** 1.99.0 in `rust-toolchain.toml` (MSRV 1.90, set by Tauri 2.12), edition 2024, resolver 3.
- **Release profile:** `panic = "unwind"`, against Tauri's size guide, so the per-tool `catch_unwind` works.
- **Tauri:** `tauri` ~2.12 with the 2.x plugins (`tauri-plugin-updater` at 2.13 or later). `@tauri-apps/cli` is pinned to an exact version and kept in parity with the crate by `cargo xtask check`.
- **Front end:** Node 24, pnpm 11.28.x, Svelte 5.57, Vite 8.3, TypeScript ~6.0.3.
- **CI runners:** pinned to `windows-2025`, `ubuntu-24.04` and `macos-26`, never `-latest`. `ubuntu-latest` moves to 26.04 in Oct-Nov 2026.

### Port inspector: why kill walks the tree per PID

The brief's starting points were the process group on Unix, and job objects or a child walk on Windows. Process groups and job objects are rejected, and the per-PID child walk is used on every OS:
- A process group or session can include the shell, the IDE terminal or a supervisor that started the server. Those are **ancestors**, and Navaja never kills ancestors by default.
- On Windows, a running tree cannot reliably be placed into a job object after the fact.

The kill therefore works per PID:
1. Take a snapshot.
2. Choose the topmost socket holder as R.
3. Collect R and its descendants, deepest first. An edge only counts when the child's start time is at or after the parent's.
4. Re-check each identity (PID plus start time) immediately before signalling it.
5. Rescan, then report survivors and whether the port is free.

User-space respawners such as nodemon and pm2 get a separate, confirmed "Stop <supervisor>" action. systemd units and Windows services get a copyable command instead.

### Accepted v1 additions beyond the brief

These are all small:
- a JWE header view in the JWT decoder;
- wslrelay and portproxy verdicts in `why`;
- window-state persistence;
- `navaja --tool <id>`;
- a light/dark theme override;
- a global-shortcut recorder.

**Cut from v1:** file input and output for text tools, such as file hashing and Base64 to and from files. Doing it safely needs Rust-side dialogs and file tokens, so it comes after v1.

## Consequences

- **Two languages.** Rust holds all logic and Svelte/TypeScript holds the UI. Text tools need **no** TypeScript, because the generic view renders their `UiSpec`; only custom views (such as the port inspector) do.
- **The webview runtime is part of the offline promise.** CSP, capabilities, an incognito webview, WebRTC removal and egress checks in CI keep it offline. Anything uncontrollable is measured (spike S2.7) and disclosed in `docs/privacy.md`.
- **Unsigned v1.** Install friction on Windows and macOS is documented. There is no official Homebrew cask until notarization.
- **Tauri v3** is a planned migration after v1.
- **Memory** is the main open risk against criterion 1. ADR 0002 records the measured numbers and the hide policy.

## Sources (selection)

- Slint licensing: https://raw.githubusercontent.com/slint-ui/slint/master/FAQ.md and `LICENSES/LicenseRef-Slint-Royalty-free-2.0.md` in the same repository
- SignPath Foundation terms: https://signpath.org/terms
- WebView2 process model: https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model
- Tauri WebDriver testing: https://v2.tauri.app/develop/tests/webdriver/
- Tauri Flatpak guide: https://v2.tauri.app/distribute/flatpak/
- Tauri bundler changelog: https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-bundler/CHANGELOG.md
- global-hotkey portal PR: https://github.com/tauri-apps/global-hotkey/pull/172
- Homebrew tap trust: https://docs.brew.sh/Tap-Trust
- Azure Artifact Signing: https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart
- Certum Open Source: https://shop.certum.eu/open-source-code-signing.html
- Winsock SO_EXCLUSIVEADDRUSE: https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse
- XNU proc_info: https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/kern/proc_info.c
- GitHub runner images: https://github.com/actions/runner-images
- wasmtime hardened-runtime issue: https://github.com/bytecodealliance/wasmtime/issues/11989
- killport (reference): https://github.com/jkfran/killport
