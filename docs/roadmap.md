# Navaja roadmap: milestones, spikes and verification

This is the execution plan accepted with [ADR 0001](adr/0001-stack.md). The design is in [architecture.md](architecture.md), and spike results are recorded in [spikes.md](spikes.md). A reference such as "architecture §7" points to a numbered section of architecture.md.

## 1. Milestones

**Rules:**
- **Done** means the definition of done in §2 below holds on Windows 11, Ubuntu 24.04 and macOS 26.
- **Spikes** are time-boxed, with PASS and FAIL criteria written first. Results go in `docs/spikes.md`. A fallback that gets taken becomes an ADR.
- **Budgets** are rough, about 21 focused weeks in total. A milestone that runs a third over budget drops items from its cut list, in order.
- **macOS work** that needs no human runs on `spikes.yml` (macos-26 and macos-26-intel). A Mac session at the end of each milestone covers UI-only checks.
- **macOS 14 or later** is the minimum: the webview's dead proxy needs it ([ADR 0003](adr/0003-webview-network.md)). Every macOS build targets it.

### M1 · Stack decision recorded (≈0.5 wk, no source code)

1. **`docs/adr/0001-stack.md`** (Accepted) contains:
   - the stack comparison and the decision for each open item;
   - the pinned versions;
   - **frozen identifiers**: repo, `io.github.joaovitorpina.Navaja`, feed URL, `APP_DIR` names, winget id and moniker, the exact NSIS publisher string, tap and bucket;
   - the signing cost table;
   - the Wayland shortcut exception;
   - why the kill walks the tree per PID (process groups and job objects would take in ancestors);
   - the accepted small v1 additions.
2. **`docs/architecture.md`**, this roadmap and the `docs/spikes.md` template.
3. **BRIEF.md:** tick the name (with GitHub status), CLI, updates, UDP and Docker questions, each linked to the ADR. The Windows cwd question stays open until M4.
4. **Repo files:**
   - `LICENSE` (MIT, João Vitor Pina);
   - a README stub saying "offline developer toolbox", never "Swiss Army knife";
   - `SECURITY.md`, `.gitignore` and `.gitattributes` (`* text=auto eol=lf`);
   - `assets/brand/GUIDELINES.md` with the mark, palette tokens and tool-icon rules.
5. **`ci.yml`:** an eol check and an identifier grep, run on windows-2025, ubuntu-24.04 and macos-26.
6. **GitHub settings:**
   - squash merges only;
   - private vulnerability reporting;
   - a read-only default token;
   - "Allow GitHub Actions to create and approve pull requests";
   - milestones M2a-M6.

**Exit:**
- No source code on main.
- Each identifier is defined once in the ADR. `scripts/check-identifiers.sh` passes on every runner for the repository URL, app id, feed, winget id, tap, bucket and publisher.
- CI is green on three runners.

### M2a · Shell and registry (≈4 wk). Brief M2: window, navigation, search, tray, registry, UUID end to end, CI on 3 OSes

1. **Skeleton PR, green on day one:**
   - stub crates, each with one real test;
   - an exact `@tauri-apps/cli` pin and `typescript ~6.0.3`;
   - `tauri.conf.json` with the identifier, CSP, `create: false` and `bundle.publisher`;
   - placeholder icons (Windows `tauri-build` needs `icon.ico`);
   - one Vitest test;
   - lints and `deny.toml`.
2. **CI** per §2 below, then branch protection, a PR-title check (conventional commits) and Renovate.
3. **`navaja-core`** (architecture §3) with unit tests for panics, unknown tools and actions, error codes, input never echoed, cancellation and output shapes.
4. **`tools/`:** `registry_test.rs` and `tools/uuid/`, then **S2.1**.
5. **xtask:** `check`, `bindings` and `tool-gate`, with tests proving that tokio, hyper or tauri under navaja-tools fails the check. `acl` writes the ACL snapshot (`acl.lock.json`), which `check` compares.
6. **App crate:**
   - `run_tool` and the other commands from architecture §5, except updates;
   - `open_logs`, `quit` and `open_url`, which About (the repository), Settings (the logs folder) and the start-failure view use;
   - settings, logging and crash files;
   - `platform/` and `copy_text`.
7. **S2.2**, then the front end:
   - ipc, registry, router, i18n and view-kit;
   - the shell;
   - the generic views with Generator and Text outputs;
   - theme;
   - accessibility basics;
   - ESLint (flat config) and Prettier as `pnpm lint`, run in the CI checks job, with the custom-view import allowlist from architecture §4 on `tools/*/ui/**`. `$lib/view-kit` is the one app module it lets custom views import.
8. **Window code:** `window.rs`, `guard.rs`, and `args.rs` with single instance and the elevation banner.
9. **Tray:** a minimal tray with placeholder icons (Open, Search, `meta.tray` entries, Quit). **S2.3** runs on it before M2a exit. The StatusNotifier host check moved to M2b, item 1, beside S2.4.
10. **End-to-end tests:** smoke (palette → UUID), single instance, a first launch with `--tool`, and the egress canary. A Linux strace guard is added, then **S2.6** and **S2.7**.
11. **`docs/adding-a-tool.md`.**

**Exit:**
- CI is required and green on three OSes.
- On Windows 11, Ubuntu 24.04 (Wayland and X11) and macOS 26, release builds start hidden and show without a flash.
- UUID comes from `list_tools()`, and searching "uid" ranks it first.
- 10,000 v7 UUIDs are unique and sorted.
- Copy stays out of Windows clipboard history.
- `--tool uuid` focuses the running instance. On Wayland, raising a window that is visible but unfocused is M2b, item 8.
- The tray works on Windows, macOS, Ubuntu GNOME with the AppIndicator extension, and KDE.
- A panicking tool returns `core.panicked` and the app keeps serving.
- The egress canary and strace guard are green.
- A throwaway tool written from the docs passes `tool-gate`.

**Cut first:** palette shell commands; the theme override; F6.

### M2b · Lifecycle, footprint, bundles (≈2.5 wk)

1. **S2.4** and the StatusNotifier host check in `platform/linux.rs`, then close behaviour per OS through `ShellPlatform`.
2. **`xtask measure`**, then **S2.5**. Implement the winning hide policy, delete the other path, and record the numbers in ADR 0004.
3. **Final brand SVGs** and `xtask icons`, which wraps `tauri icon` and also produces the macOS template tray icon.
4. **`bundle.yml`:** a weekly, keyless build of the exact release matrix. It also checks that the macOS bundle's `Info.plist` sets `LSMinimumSystemVersion` to 14.0 ([ADR 0003](adr/0003-webview-network.md)): no CI job builds a bundle before it.
5. **Stretch, the first thing to cut:** the global-shortcut recorder and `docs/wayland-shortcut.md`.
6. **`CONTRIBUTING.md`** with the QA checklist.
7. **macOS clipboard:** keep copies off Universal Clipboard. On macOS, `copy_text` writes through `NSPasteboard` (objc2-app-kit) instead of arboard: `prepareForNewContentsWithOptions(CurrentHostOnly)`, then the text and the ConcealedType marker. arboard's own `set` clears the pasteboard, which would drop that option.
8. **Wayland focus on a second launch.** The single-instance plugin forwards only argv and the working directory over D-Bus, so the launcher's `XDG_ACTIVATION_TOKEN` or `DESKTOP_STARTUP_ID` is lost. GNOME and KDE then refuse to raise a window that is visible but unfocused.
   - A second launch on Linux checks whether a running Navaja owns the plugin's D-Bus name. If one does, it makes the plugin's `ExecuteCallback` call itself, with the token as an extra argument, and exits with code 0. The plugin stays for the primary.
   - `args::parse` accepts the token only if it is printable ASCII and within a length cap.
   - The primary shows the window, then calls `set_startup_id` with the token on the main thread, instead of `set_focus`, which would mint a token the compositor refuses.
   - **Known limitation:** tray-menu actions cannot take focus on Wayland. The shell draws the menu (libappindicator), so the app never gets a token for the click.
9. **Single instance on macOS.** tauri-plugin-single-instance 2.5.2 hands off through a fixed `/tmp/<id>_si.sock` that every user shares, with no peer check. Another local user can squat it, and a second user on the same Mac loses single instance.
   - Fix it through a patched plugin (`[patch.crates-io]`): put the socket in the per-user temp dir (`_CS_DARWIN_USER_TEMP_DIR`), check the peer with `getpeereid` on connect and accept, and cap the read at a few KiB.
   - Report the issue upstream.

**Exit:**
- Close behaves correctly per OS, and every quit path works while the window is hidden.
- On GNOME and KDE Wayland, `--show`, `--toggle` and `--tool` raise and focus a window that is visible but unfocused.
- On macOS, a second user's Navaja keeps its own single instance, and a socket owned by another user is refused.
- On macOS, a copy does not appear on a Handoff-paired device.
- ADR 0004 has numbers per OS.
- `bundle.yml` is green for every format.
- Screen readers pass: NVDA, VoiceOver and Orca.
- 100-200 % scaling, light and dark themes, and the brand review all pass.

**Cut first:** the global shortcut; window-state; the macOS single-instance fix, which then moves to M6 and still lands before v1.

### M3 · Text tools (≈3 wk)

1. **S3.1.** Only if it fails, add `run_tool_raw` and/or a raw output tail.
2. **Editor:** CodeMirror and its thresholds, plus Code output with preview and "Copy all".
3. **Views:** TransformView; KeyValue, Diagnostics and Binary outputs; ConfirmDialog in view-kit; Vitest and axe for each spec variant.
4. **JSON:** `tools/json/format.rs`, with proptest against serde_json and fixtures for deep nesting, duplicate keys, huge numbers, surrogates and a BOM. Then **S3.2**, then `mod.rs` and its end-to-end spec.
5. **`tools/base64`, `url`, `hash` and `jwt`:** one PR each, with **nothing under `app/`**.
6. **Re-run S2.5.** Base64 becomes the worked example in `adding-a-tool.md`.

**Exit:**
- All tools work on three OSes, with 10,000 proptest cases.
- Each tool PR passes `tool-gate` and touches only its folder plus one `lib.rs` line.
- A 10 MB single-line paste causes no long task over 1 s, and a 50 MB paste formats and copies.
- Hash output matches `certutil`, `sha256sum` and `shasum`.
- An expired JWT shows "expired" with the relative time.
- axe reports nothing, and a keyboard-only pass succeeds.

**Cut first:** editor markers; the JWE header view.

### M4 · Port inspector (≈5 wk)

1. **navaja-ports skeleton** (architecture §7).
2. **S4.1**, then `sys/linux/`:
   - `/proc/net` and fd inodes;
   - sockets that cannot be attributed are reported with their uid and PID `NeedsRoot`;
   - a cgroup-to-unit lookup.
3. **`sys/windows/`:** owner tables with `liCreateTimestamp`, Toolhelp, cmdline, cwd for `why`, and owner verification.
4. **S4.2**, then `sys/macos/`: libproc, plus `pcblist_n` or uid-only attribution, and `KERN_PROC_PID`.
5. **gather, explain and render:**
   - golden tests from hand-written Facts;
   - `xtask capture-ports`, which redacts arguments by default;
   - insta snapshots on every OS.
6. **S4.3a, then kill (architecture §7), then S4.3b.**
7. **Copy commands and strings:** guarded elevation copy commands; Reason strings in `tools/ports/ui/i18n/en.ts`.
8. **`tools/ports/`** with `tray: true`, PortTable, WhyPanel and KillDialog.
9. **Live tests:** `tests/live.rs` spawns a listener and a child tree on `127.0.0.1:0`, in a serial group. Plus an end-to-end ports spec.

**Exit:**
- Live tests are green on three runners: list, why, kill, port freed, stale plan refused.
- The **first brief output matches byte for byte**, with the brief's `pd` placeholder passed as the command prefix.
- Reused-PID and stale-PPID fixtures are refused.
- Elevated or root listeners show a reason and a guarded command; no field is ever blank.
- A nodemon port is freed without killing the terminal or the IDE.
- `plan_kill` refuses PID 4, PID 1, svchost, launchd, Navaja itself and the compositor.
- UDP works, IPv4 and IPv6 are merged, and dev ports are highlighted.
- The tray entry appears with no change to `tray.rs`.
- The ports PR passes `tool-gate`.
- The BRIEF.md cwd question is ticked.

**Cut first:** `pcblist_n`; "Stop <supervisor>" plans.

### M5 · Port special cases (≈3.5 wk)

1. **S5.1-S5.3**, keeping the raw outputs as fixtures.
2. **`services.rs` and `httpsys.rs`:** http.sys request queues only if S5.3 passes.
3. **`reserved.rs`, `wsl.rs`** (wslrelay and portproxy only), the verdict order, and the **second brief output as a golden test**.
4. **S5.4a**, then navaja-docker with fake-engine tests on three OSes: answering, hung, and a remote pipe.
5. **`docker` feature:** registers the engine; the deny wrappers are re-checked.
6. **Ports UI:** container and compose project names, `stop_container`, the Linux fallbacks, then **S5.4b**.
7. **CI:** Linux CI runs a busybox container and stops it through Navaja.

**Exit:**
- Both goldens pass on three OSes.
- Services offer Stop-Service.
- Port 445 is labelled SMB, with no IIS advice.
- Reserved-range verdicts are correct in three locales.
- Docker is tested automatically on Ubuntu, and by hand on Docker Desktop for Windows and macOS.
- Remote endpoints are refused with no connection attempt.
- With no engine running, `why` takes 1.5 s or less.

**Cut first:** the portproxy verdict; http.sys queues.

### M6 · Release = v1 (≈3 wk)

1. **One PR enables `updater` by default:**
   - `updater.rs` and the final `channel.rs`;
   - FirstRunDialog and UpdateDialog;
   - Settings "Check now";
   - the tray item.
2. **Release mechanics:**
   - `hooks.nsh` and the portable-zip marker;
   - the release config overlay;
   - `release.yml` plus a reusable `release-build.yml` (release-plz, then tauri-action v1, then `verify-release`);
   - then **S6.1**.
3. **minisign key:**
   - a real key in a protected `release` environment, limited to `v*` tags;
   - two offline backups;
   - `docs/release.md`.
4. **Provenance:** attestations and a minisign-signed `SHA256SUMS`.
5. **Package-manager repos:** `xtask manifests`, the tap and bucket repos, and `xtask notices` (THIRD_PARTY_NOTICES; the npm side can reuse the package listing in `xtask licenses`).
6. **S6.2**, then `install.md` and `privacy.md`.
   - install.md covers SmartScreen "More info → Run anyway", Smart App Control, Gatekeeper "Open Anyway", the Linux tray host and the NVIDIA variables.
   - privacy.md lists the OS services in a macOS text field's context menu: Look Up, Translate, Search With Google, Share and Services. They send the selected text only when the user picks one.
   - Optional, later: on macOS, replace that menu with a native one built from `PredefinedMenuItem` cut, copy, paste and select all.
   - privacy.md carries ADR 0003's text for privacy.md word for word ([ADR 0003](adr/0003-webview-network.md), "What is disclosed"): the traffic of system services that the webview wakes and that Navaja cannot stop, and who could listen on the dead proxy's port. The ADR's table and `scripts/spikes/s2-7-disclosed.tsv` are S2.7's labels for that traffic, not the text.
   - Next to those services, privacy.md lists the hand-offs (architecture §5): About's repository link opens in the default browser and the logs folder in the file manager, only when the user asks. The browser or file manager loads them, not Navaja, even when Windows starts the browser as Navaja's child process.
7. **Final QA:**
   - `measure`;
   - S2.7 again, now with the updater;
   - the full manual matrix;
   - two rehearsals on a scratch repo.
8. **Release:**
   - run a TMview trademark search for classes 9 and 42 before tagging;
   - tag **v1.0.0**;
   - submit the binaries to Defender;
   - open the first winget PR by hand;
   - apply to SignPath.

**Exit:**
- v1.0.0 is published with every artifact, signature and `latest.json`.
- Clean installs work on Windows (setup.exe, Scoop), macOS 14 or later (DMG, tap), Ubuntu 22.04/24.04 (deb, AppImage) and Fedora (rpm).
- **Without consent:** no packets leave Navaja's process tree in 10 minutes.
- **With consent:** traffic goes only to github.com and release-assets.githubusercontent.com.
- One-click update works on every format.
- Tampered, downgraded and off-repo feeds are refused.
- Managed installs show the package manager's upgrade command.

**Cut first:** winget, moved to 1.0.1.

### M7 · After v1 (outline)

- **Signing:** SignPath, using the split pipeline (`--no-binary-patching` on Windows, `tauri signer sign --app-version`). Apple Developer ID and the official cask when the budget allows.
- **System tools:** environment and PATH, hosts and DNS, certificates. Each is a tool folder that passes the system-tool gate and reads local state only.
- **Extensions** (architecture §9), after a spike on the wasmtime adapter (S7.1, defined when M7 starts).
- **Then:** the Tauri v3 migration, the privileged helper, file input and output, and the Wayland portal shortcut.

### Spikes (PASS → proceed; FAIL → fallback)

| Spike | Gates | PASS | FAIL → |
|---|---|---|---|
| S2.1 IDE | macro registry | RustRover and rust-analyzer navigate and complete through `register_tools!` | Two-line form `mod x;` plus a list entry (needs sign-off) |
| S2.2 Views outside `app/` | custom-view discovery | HMR, build, packaged app, svelte-check, ESLint and Vitest all work on 3 OSes | tsconfig and Vitest aliases; otherwise `app/src/tools/<id>/` (needs sign-off) |
| S2.3 Tray icon | tray | Crisp at 100-200 % on Windows; macOS template icon works | Per-scale PNGs, or `with_inner_tray_icon` with an .ico |
| S2.4 Linux tray host | close behaviour | The host check is true exactly when the icon shows (GNOME with and without the extension, Fedora, KDE, X11) | Close always quits; "keep in tray" becomes opt-in |
| S2.5 Hide policy | window lifecycle | Destroying the webview saves at least 50 MB on Windows, and a cold show takes 500 ms or less at the median | Always warm |
| S2.6 WebdriverIO | e2e gating | 10 of 10 runs per OS, under 10 min | macOS end-to-end tests become non-gating, plus a manual smoke test |
| S2.7 Egress | offline claim | A VM NIC capture shows no packets from the process tree, idle or in use | Browser args or policies; anything left over is disclosed verbatim in privacy.md |
| S3.1 IPC and editor | large text | The envelope and the input each take 300 ms or less at 20 MB; CodeMirror keystroke p95 is 50 ms or less | Raw IPC tail; lower thresholds |
| S3.2 JSON at scale | JSON tool | 50 MB in 1 s or less; peak memory at most 3 times the input; 100k nesting levels without overflow | Profile, or lower the live limit |
| S4.1 Access matrix | Platform modules | Every field returns a value or one classified reason (same user, elevated, other user, SYSTEM, PPL, WOW64) | `NotSupported` for that class |
| S4.2 `pcblist_n` | macOS, other users | The right PID for a root listener without root, on arm64 and Intel | uid only, plus a copyable `sudo lsof` |
| S4.3a/b Supervisors and kill | kill rule | nodemon, `node --watch`, dotnet watch, watchexec, uvicorn, pm2 and systemd all explained; the kill frees the port; ancestors stay alive | Adjust the R and respawner rules |
| S5.1 netsh locales | reserved ranges | Identical ranges in en-US, pt-BR and de-DE | "Likely reserved" wording |
| S5.2 Probe codes | reserved ranges | A reserved port is told apart from a held one, with no firewall prompt | Drop the probe; use "likely" wording |
| S5.3 http.sys | PID 4 | URL ACLs and the request queue are readable without elevation | Static table plus "needs elevation" |
| S5.4a/b Engines | Docker | Container and project are named on Docker Desktop (Windows, macOS) and on rootful and rootless Engine; remote endpoints are refused with no traffic | Engine marked unsupported and documented |
| S6.1 Release dry run | M6 | All formats install and update with one click; a tampered or downgraded update is refused; the winget marker works; unsigned rpm installs | Fix or drop the format by ADR. A marker failure falls back to Direct |
| S6.2 Clean machines | install docs | The written steps get a fresh Windows 11 (Smart App Control on, evaluation and off), a fresh macOS user, Ubuntu and Fedora to a running app | State "not supported until signed builds exist" up front |

---

## 2. Verification

**Definition of done:**
- Works on Windows 11, Ubuntu 24.04 (GNOME Wayland, plus X11 where relevant) and macOS 26, with CI green on all three.
- Tool logic returns data or a `ToolError`, and never prints, prompts or connects.
- Every visible string goes through `t()`.
- A keyboard-only path works, and axe reports nothing. axe joins the Vitest suite in M3 (item 3); until then only the keyboard-only part applies.
- Logs carry no payloads.
- Docs and generated bindings are updated.

**Local commands:**
```sh
pnpm install --frozen-lockfile
cargo xtask check                    # crate edges, dependency closures, release config (CSP, features, hidden windows), ACL snapshot, version parity
cargo xtask acl                      # after a command or capability change: rewrite acl.lock.json, then review its diff
cargo xtask bindings --check         # ts-rs output == committed bindings
cargo xtask tool-gate origin/main    # tool PR touches only what architecture §4 allows
cargo fmt --all --check
rustfmt --edition 2024 --check tools/*/mod.rs  # tool modules, which cargo fmt skips (declared inside a macro)
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace        # includes serial live port tests from M4, and the clipboard markers check (below)
# The clipboard markers check replaces your clipboard's contents, and on X11 leaves it empty. Its ordinary copy can reach
# clipboard history (Win+V, Klipper, macOS history apps), and cloud clipboard or Universal Clipboard when they are on.
# Linux: it reads X11, so it skips with no X display, or where arboard copies over Wayland (KDE, wlroots). On GNOME
# Wayland arboard falls back to X11, so it runs there. This runs it on a private X server, away from your clipboard:
env -u WAYLAND_DISPLAY xvfb-run -a cargo nextest run -p navaja --test clipboard
cargo deny check bans licenses sources
cargo xtask licenses                 # npm licences vs deny.toml's allowlist + js-licenses.toml (after pnpm install)
bash scripts/check-fixture-secrets.sh  # gitleaks over tracked fixtures and snapshots (needs gitleaks on PATH)
pnpm check && pnpm lint && pnpm test # svelte-check, ESLint+Prettier, Vitest+axe
pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/e2e.conf.json && pnpm e2e
# Linux, as CI runs it after the same build: the end-to-end suite under the strace network guard
WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- xvfb-run -a bash app/e2e/strace-guard.sh
```

The end-to-end build is isolated from a Navaja you already run: it has its own identifier, starts its WebDriver server only when the harness asks, and gets a fresh temporary `NAVAJA_APP_DIR` (architecture §5).

**JS licences** (`cargo xtask licenses`):
- It lists every installed npm package with `pnpm licenses list`, dev dependencies included, since the front end bundles some of them.
- Each installed version's licence is read from its own `package.json` (`license`, or the legacy `licenses`), the way pnpm reads a manifest. pnpm's reported licence is not used: when a manifest declares none, or says `SEE LICENSE IN <file>`, pnpm reports the licence names it finds in the LICENSE file's text.
- Each licence is read as an SPDX expression and checked against deny.toml's `[licenses].allow`, the same list cargo-deny uses for crates.
- Anything else needs an entry in `js-licenses.toml`: the package, its licence exactly as its `package.json` declares it (`Unknown` when it declares none), and a one-line reason. A package that changes licence falls out of its entry.
- A missing, custom or unparseable licence fails unless it has an entry, and so does `SEE LICENSE IN <file>`. Before adding an entry for a package that declares no licence, read its LICENSE file. Every failing package is reported.
- It sees what is installed on the running OS, so another OS's native binaries (esbuild, Tailwind, Tauri CLI) are not listed. An entry that no installed package uses under its licence only warns, since another OS may need it.

**Secrets in fixtures** (`scripts/check-fixture-secrets.sh`):
- gitleaks scans the tracked files under `tests/fixtures/` and `snapshots/` and every `*.snap`, with its built-in rules.
- Fixtures and snapshots must be UTF-8 text, such as redacted dumps or JSON. gitleaks skips binary files, archives, UTF-16 text and symlinks without saying so, so the script fails on any of them before gitleaks runs. Git decides what is binary (`i/-text` in `git ls-files --eol`).
- gitleaks also skips the paths its built-in config allowlists, such as images and fonts, `*.pdf` and `*.bin`, lockfiles, and anything under `node_modules/` or `vendor/`. A fixture must not be named like one.
- With no such files it passes and says so. Otherwise it fails on a finding, or when gitleaks is missing or fails.
- CI installs gitleaks 8.30.1 from its GitHub release and checks the archive's SHA-256 first.

**Workflows:**

| Workflow | Runs |
|---|---|
| `ci.yml` checks (ubuntu-24.04) | rustfmt (`cargo fmt`, plus `rustfmt --check tools/*/mod.rs` for the tool modules it skips), ESLint and Prettier (`pnpm lint`), svelte-check, cargo-deny on 4 targets, `xtask check`, `bindings --check`, `tool-gate`, the JS licence allowlist (`xtask licenses`), gitleaks over fixtures and snapshots (`scripts/check-fixture-secrets.sh`) |
| `ci.yml` os, ×3 (windows-2025, ubuntu-24.04, macos-26) | clippy → nextest with live tests → doctests → `tauri build --debug --no-bundle` → the same build with `--features e2e` and `e2e.conf.json` → WebdriverIO. Linux runs the suite under the strace guard, inside `dbus-run-session -- xvfb-run` with `WEBKIT_DISABLE_DMABUF_RENDERER=1`. The job stops after 45 min, so a hung run fails instead of holding the runner for GitHub's 6 h. macOS also runs clippy, with the same bans and `-D warnings`, on the Intel slice of the universal build: `cargo clippy --workspace --all-targets --target x86_64-apple-darwin -- -D warnings`. That step keys on `runner.os` and fails if the runner is not arm64, since then nothing would lint the arm64 slice. After the builds, `scripts/check-macos-target.sh` reads the minimum macOS back from each file they produced, which must be 14.0 (ADR 0003) |
| `advisories.yml` | Daily `cargo deny check advisories`. Opens an issue but never blocks a PR |
| `bundle.yml` | Weekly, keyless build of the release matrix: NSIS and zip, universal DMG, deb, rpm and AppImage in `container: ubuntu:22.04` |
| `spikes.yml` | Manual, and on a PR that changes it or `scripts/spikes/`. Spike steps that need no human. Job `s2-2` (S2.2), on windows-2025, ubuntu-24.04 and macos-26: a throwaway tool with a custom view (`scripts/spikes/s2-2-probe.sh`) through `cargo test -p navaja-tools`, `pnpm check` (and a type error in a .ts and a .svelte file it must refuse), `pnpm lint` (and forbidden imports in a .ts and a .svelte file, and a misformatted file, it must refuse), `pnpm test`, `pnpm build`, the end-to-end build plus the probe's own spec, an HMR check against the Vite dev server (`hmr-check.mjs`), and an HMR check in the app's webview (a dev build without `custom-protocol` that loads the dev server, plus a second spec that edits the view). Job `s2-3` (S2.3), on macos-26 and windows-2025, runs the app as shipped (`scripts/spikes/s2-3-check.sh`, `s2-3-tray.ps1`). On macOS it captures the menu bar before the app starts and in the light and the dark appearance, and checks that the template icon is drawn, monochrome and tinted like the clock; its negative controls are the capture from before the app started and a build whose icon is not a template, which must fail on its tint. On Windows it promotes the notification-area icon, finds it through UI Automation, captures it at the runner's 100 %, and checks that an icon is drawn in its button; the negative control is the same check on an empty stretch of the taskbar. The captures are the run's artifact. Job `exit-clipboard-history` (M2a exit: copy stays out of Windows clipboard history), on windows-2025 (`scripts/spikes/exit-clipboard-check.sh`, `clipboard-history.ps1`): Navaja's Copy button through a spec written at run time, the exclusion formats on the clipboard (an ordinary copy must fail that check), and the history checks with their controls. Windows Server keeps clipboard history off, so there the summary marks history off and those checks inconclusive, and the job leaves a warning; the history checks have never run. Job `s2-7` (S2.7), on the same three runners (`scripts/spikes/s2-7-egress.sh`): a 5 min baseline without the app, then the e2e build idles 5 min with its window shown, then `pnpm e2e` runs, each phase under a capture of the VM's traffic. Linux runs the app and WebdriverIO in a network namespace with NAT and a public resolver and captures its veth and loopback with tcpdump, and the host's uplink for comparison with the baseline; after each phase the namespace must still reach the network. macOS captures with tcpdump on pktap, which names each packet's process, and counts the daemons WebKit starts for the app (webprivacyd, the Safe Browsing service) as the app's when they start within 15 s after it. Windows reads the WFP connection, process creation, DNS client and BITS client logs, names the hosts from a pktmon capture, and counts WAM's sign-in service and Microsoft-account provider in the 15 s after an app start as the app's. Traffic of those daemons and services that an entry of `scripts/spikes/s2-7-disclosed.tsv` covers is reported as disclosed, in its own summary row, and fails nothing (ADR 0003); anything else still fails. A control request to github.com (and on Windows a BITS download) must be captured and attributed first, each phase must show the app's own WebDriver traffic, and nothing in the baseline may count as the app's, so an empty capture can't pass and the attribution can't blame the machine's own traffic on the app. Each later phase also lists, for review, the traffic outside the tree that the baseline lacks, and on Windows what follows each app start. The captures are uploaded as an artifact for 14 days. Each check is its own step, and the run summary gets a PASS/FAIL table per OS, plus a findings table per phase for S2.7 |
| `release.yml` + `release-build.yml` | Every push to main runs release-plz, which opens or updates the release PR. Merging that PR runs the build: tauri-action v1, then `verify-release`, attestations, `SHA256SUMS.minisig`, undraft, and bucket and tap updates. `release-build.yml` can also be run by hand on a tag |

**CI hygiene:**
- Runner labels are pinned, never `-latest`; `ubuntu-latest` moves to 26.04 in Oct-Nov 2026.
- Actions are pinned by SHA.
- A tool that no pinned action installs is downloaded at a pinned version and checked against its published SHA-256 before use (gitleaks). Renovate does not bump these; update the version and checksum together.
- Swatinem/rust-cache with a per-OS key, saved only on main.
- Release jobs restore no cache.

**Offline and privacy checks:**
- **On every PR:**
  - an egress canary: fetch, image, beacon, WebSocket, `window.open`, top-level navigation, and WebRTC in the page and in the iframes it creates, all blocked. Fetch, image, beacon and WebSocket must each raise their own CSP violation, since a request that fails for some other reason proves nothing;
  - a Linux strace guard: a `connect`, `sendto`, `sendmsg` or `sendmmsg` to any address other than 127.0.0.1 or ::1 fails the run (the 127.0.0.53 DNS stub included). So does an inet call whose address can't be read, or an app process still running after the suite. The suite never opens a URL or the logs folder: strace would follow the browser or file manager that starts (architecture §5, "Hand-offs to other programs");
  - a canary input at `NAVAJA_LOG=trace`, plus a panicking tool, must not appear in logs, crash files or stderr. The integration test `app/src-tauri/tests/privacy.rs`, run by nextest, checks logs and crash files at trace level; it does not capture stderr yet;
  - the clipboard privacy markers (architecture §5, "Clipboard"): `copy_carries_the_privacy_markers` (`app/src-tauri/tests/clipboard`), run by nextest, copies a random canary through `copy_text`'s own call and reads the markers back through the OS's own API, against an ordinary copy that must carry none. It checks the three Windows formats and macOS's `org.nspasteboard.ConcealedType`. Its Linux part (`x-kde-passwordManagerHint`, over X11, which is also how arboard copies on GNOME Wayland) needs an X display, which CI's nextest step does not have, so on CI it prints why it skipped. It runs only where a developer runs it with a display, e.g. under `xvfb-run` (local commands above).
- **In M2a and M6:**
  - a whole-process-tree NIC capture on each OS (S2.7), without opening the repository link or the logs folder, whose browser or file manager may count as part of the tree. The webview's web traffic goes to a dead proxy on every OS (architecture §5). What system services the webview wakes, and Navaja cannot stop, is on S2.7's disclosed list ([ADR 0003](adr/0003-webview-network.md)): reported apart, never a pass for anything else;
  - the webview data folder holds no input text;
  - copied tokens are absent from clipboard history and Klipper:
    - the markers are checked on every PR on Windows and macOS (above). The Linux one is checked only when a developer runs the check with an X display;
    - a person checks that Windows clipboard history honours them (spikes.md, "M2a exit check");
    - a person checks Klipper during manual QA on KDE Plasma 6 Wayland (below). Its compositor offers data-control, so arboard copies over Wayland there, which no automated check reads;
    - nothing checks the macOS history apps that follow nspasteboard.org yet.

**Manual QA**, at each milestone end, on Windows 11, Ubuntu GNOME Wayland with AppIndicator, Ubuntu X11, Fedora GNOME (no tray), KDE Plasma 6 Wayland and macOS 26:
- **Shell:** starts hidden; tray menu; close and quit paths; `--toggle` and `--tool`; keyboard-only use; About's repository link opens the browser and Settings' "Open logs folder" opens the file manager (no automated test opens them).
- **Klipper, M2a and M6** (KDE Plasma 6 Wayland): a UUID copied with Navaja's Copy button, or with Ctrl+C on the output, pastes, and Klipper's history does not list it. An ordinary copy is listed, as the control.
- **Wayland focus, from M2b** (GNOME and KDE): `--show`, `--toggle` and `--tool` raise and focus a window that is visible but unfocused. Tray-menu actions may leave it unfocused; that is a known limitation (M2b, item 8).
- **M2b and M6:** screen readers (NVDA, Narrator, Orca, VoiceOver); 100-200 % scaling; light and dark themes; on macOS, a copy does not reach a Handoff-paired device.
- **Ports, from M4:** an elevated listener gets a reason and a guarded command; nodemon kill; pm2 respawn.
- **Docker Desktop, from M5.**
- **M6:** install, one-click update and uninstall per format, plus the SmartScreen and Gatekeeper wording.

---

## 3. Risks (top) and post-v1

| Risk | Mitigation |
|---|---|
| Killing the wrong process: PID reuse, stale PPIDs, dead socket owners | Edges ordered by start time; Windows owner verification; the plan is re-validated from a fresh snapshot; identity re-checked before each signal; denylist; ancestors never by default; spikes S4.3a and S4.3b |
| WebView2 memory comes in far above expectations (criterion 1) | S2.5 measures the whole process tree and picks the hide policy; a regression above 20 % blocks a milestone; the stack stays |
| "Fully offline" is undermined by webview telemetry, crash dumps, DNS prefetch or WebRTC | Hardening and the dead proxy on every OS (architecture §5), egress checks on every PR, S2.7 captures; anything uncontrollable is disclosed in privacy.md and the first-run dialog (ADR 0003's list) |
| Unsigned releases: SmartScreen, Smart App Control, Gatekeeper, Defender false positives | install.md walkthroughs; attestations and a signed `SHA256SUMS`; Defender submission per release; SignPath application right after v1 |
| Undocumented OS interfaces change: `pcblist_n`, the PEB layout, netsh text, http.sys | One module each, classified fallbacks ("likely", `NotSupported`), and locale and version fixtures |
| Tauri drift and v3 | Exact CLI pin plus a version-parity check; grouped Renovate PRs merged only after end-to-end tests on all three OSes; v3 after v1 |
| Linux desktop variance: no tray on vanilla GNOME, Wayland focus, WebKitGTK and NVIDIA | S2.4 decides the close behaviour; second launches forward the activation token (M2b); NVIDIA variables documented; S6.1 checks the AppImage sandbox |
| winget rejects the custom `/CHANNEL=winget` switch | Without the marker, the install falls back to Direct and shows the in-app update. This is documented as a known deviation |
| Solo maintainer, about 21 weeks | Budgets and cut lists per milestone; gating spikes first; the global shortcut is the first cut |

**Post-v1:**
- runtime extensions (architecture §9);
- file input and output for text tools, through Rust-side dialogs and `FileRef` tokens;
- a privileged helper (runas, pkexec, osascript);
- WSL depth: distro and Linux PID, mirrored mode;
- the Wayland portal shortcut;
- the Tauri v3 migration;
- signing (SignPath, Apple, Store);
- native arm64 Windows and Linux builds;
- text-tool extras: a full-URL query table, hash compare, JSONC, JWT verification with a pasted key;
- pinned tools and opt-in history;
- more system tools;
- a CLI front end.
