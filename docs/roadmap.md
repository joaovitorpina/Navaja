# Navaja roadmap: milestones, spikes and verification

This is the execution plan accepted with [ADR 0001](adr/0001-stack.md). The design is in [architecture.md](architecture.md), and spike results are recorded in [spikes.md](spikes.md). A reference such as "architecture §7" points to a numbered section of architecture.md.

## 1. Milestones

**Rules:**
- **Done** means the definition of done in §2 below holds on Windows 11, Ubuntu 24.04 and macOS 26.
- **Spikes** are time-boxed, with PASS and FAIL criteria written first. Results go in `docs/spikes.md`. A fallback that gets taken becomes an ADR.
- **Budgets** are rough, about 21 focused weeks in total. A milestone that runs a third over budget drops items from its cut list, in order.
- **macOS work** that needs no human runs in `ci.yml` and `spikes.yml` on GitHub's macos-26 runners, which are arm64. No workflow runs on an Intel Mac: `ci.yml` lints the Intel slice of the universal build with clippy on macos-26 (`--target x86_64-apple-darwin`). A Mac session at the end of each milestone covers UI-only checks ([manual-qa.md](manual-qa.md)).
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
10. **End-to-end tests:** smoke (palette → UUID), single instance, a first launch with `--tool`, the egress canary, and the data-folder canary (§2). A Linux strace guard is added, then **S2.6** and **S2.7**.
11. **`docs/adding-a-tool.md`.**

**Exit:**
- CI is required and green on three OSes.
- On Windows 11, Ubuntu 24.04 (Wayland and X11) and macOS 26, release builds start hidden and show without a flash.
- UUID comes from `list_tools()`, and searching "uid" ranks it first.
- 10,000 v7 UUIDs are unique and sorted.
- Copy stays out of Windows clipboard history.
- `--tool uuid` focuses the running instance. On Wayland, focus on a second launch or from the tray's Open is M2b, item 8, whether the window is visible but unfocused, minimized or hidden; there M2a asks only that the requested tool opens and that a hidden window shows again.
- The tray works on Windows, macOS, Ubuntu GNOME with the AppIndicator extension, and KDE. On Wayland, its Open has the carve-out above.
- A panicking tool returns `core.panicked` and the app keeps serving.
- The egress canary and strace guard are green.
- A throwaway tool written from the docs passes `tool-gate`.

**Exit status** (2026-10-05): `main` after PRs [#14](https://github.com/joaovitorpina/Navaja/pull/14), [#17](https://github.com/joaovitorpina/Navaja/pull/17), [#18](https://github.com/joaovitorpina/Navaja/pull/18), [#19](https://github.com/joaovitorpina/Navaja/pull/19), [#22](https://github.com/joaovitorpina/Navaja/pull/22) and [#23](https://github.com/joaovitorpina/Navaja/pull/23). GitHub's milestone [M2a · Shell and registry](https://github.com/joaovitorpina/Navaja/milestone/1) lists every M2a PR. [RUN_MAIN](https://github.com/joaovitorpina/Navaja/actions/runs/RUN_MAIN) is the first `ci.yml` push run on `main` after them. M2a is not closed: four criteria, the spikes S2.1 and S2.3, and §2's manual QA wait for a person. Their steps are in [manual-qa.md](manual-qa.md), which also holds the results table.
- **CI is required and green on three OSes: met.** Branch protection on `main` requires `checks`, `web`, `os (windows-2025)`, `os (ubuntu-24.04)`, `os (macos-26)` and `conventional title`, and is strict and enforced for admins (read back on 2026-10-05). The 12 `ci.yml` push runs on `main` up to `9988b86` all passed, and so did RUN_MAIN.
- **Release builds start hidden and show without a flash: needs a person** on Windows 11, Ubuntu 24.04 (Wayland and X11) and macOS 26 (manual-qa.md, check 1). CI covers the parts it can: `cargo xtask check` requires `"create": false` and `"visible": false` (`check::tests::every_window_starts_hidden`); second launches and tray actions that arrive before `shell_ready` are kept until it, and only the latest tool is opened (`window::tests::requests_wait_for_the_front_end`); and the front end calls `shell_ready` only once its shell, or its start-failure view, has rendered (Vitest, `App.test.ts`: "builds navigation from list_tools and reports ready after the first frame" and "shows the start failure before reporting ready"). Nothing tests the show itself or the 5 s fallback. CI builds only `--debug`, and nothing looks at the first frame.
- **UUID comes from `list_tools()`, and "uid" ranks it first: met.** `registry_test::uid_finds_uuid_first` searches the registry the app ships. `search::tests::uid_ranks_uuid_first` ranks uuid above two tools that also match "uid". The smoke spec "opens the UUID generator from the palette and generates a UUID" empties the palette's list before it types "uid", so its first option can only be a "uid" result. All three pass in RUN_MAIN, as they did on PR [#17](https://github.com/joaovitorpina/Navaja/pull/17)'s commit `97ecc15` ([37268660667](https://github.com/joaovitorpina/Navaja/actions/runs/37268660667)).
- **10,000 v7 UUIDs are unique and sorted: met.** `uuid::tests::ten_thousand_v7_are_unique_and_sorted` runs at the tool's cap and passes in RUN_MAIN.
- **Copy stays out of Windows clipboard history: needs a person** on Windows 11 ([spikes.md](spikes.md), "M2a exit check", by a person). Windows Server keeps history off, so `spikes.yml`'s `exit-clipboard-history` job ([#13](https://github.com/joaovitorpina/Navaja/pull/13)) can't answer: history stayed off in every run, and the job marks the check inconclusive. On every PR, nextest's `copy_carries_the_privacy_markers` checks that `copy_text` sets the three exclusion formats; it passes in RUN_MAIN, as it did on PR [#23](https://github.com/joaovitorpina/Navaja/pull/23)'s commit `59e7f55` ([37270619718](https://github.com/joaovitorpina/Navaja/actions/runs/37270619718)).
- **`--tool uuid` focuses the running instance: the hand-off is met; focus needs a person** (manual-qa.md, check 2). The spec "a second launch hands --tool to the running window and exits" passes on all three OSes in RUN_MAIN: the second process exits with code 0, and the running page opens `#/tool/uuid`. No test checks focus, or a window that is hidden, minimized or covered.
- **The tray works on Windows, macOS, Ubuntu GNOME with the AppIndicator extension, and KDE: needs a person** (manual-qa.md, check 3). S2.3's job finds the icon on windows-2025 and macos-26, as in `spikes.yml` run [37269710807](https://github.com/joaovitorpina/Navaja/actions/runs/37269710807) on PR [#19](https://github.com/joaovitorpina/Navaja/pull/19)'s commit `f3b5990`. `tray::tests` cover the menu's ids and tool entries. Nothing opens the menu, and no runner has GNOME or KDE.
- **A panicking tool returns `core.panicked` and the app keeps serving: met.** `commands::tests::a_panicking_tool_leaves_the_app_serving` runs a panicking tool through `execute()`, then the real uuid tool on the same state, twice. It passes in RUN_MAIN, with `tests::panics_become_errors_without_the_payload` and `privacy`'s `canary_input_never_reaches_logs_or_crash_files`.
- **The egress canary and strace guard are green: met.** In RUN_MAIN, "blocks every way the page could reach the network" and "refuses to navigate away from the app" pass on all three OSes, and the Linux guard reports that no connection left the machine. On Windows and macOS, traffic the engine starts by itself is S2.7's to show.
- **A throwaway tool written from the docs passes `tool-gate`: met, by hand.** On 2026-10-05 a `line_order` Transform tool, written from `adding-a-tool.md` on a branch from `9988b86`, got `tool-gate: ok (text tool: line_order)` on its first commit. Its tests, clippy, `xtask check`, `bindings --check` and cargo-deny passed. `rustfmt --check` on its `mod.rs` failed once, on the author's over-long line, and passed after the fix. PR [#11](https://github.com/joaovitorpina/Navaja/pull/11)'s `lorem` tool had passed the gate on 2026-10-03. Neither ran on CI, which runs the gate only on a pull request, and neither branch was kept. The doc gaps both runs found are fixed in `adding-a-tool.md`.
- **Items:**
  1. Skeleton: done ([#1](https://github.com/joaovitorpina/Navaja/pull/1)).
  2. CI, branch protection, the PR-title check and Renovate: done. The three `ci.yml` jobs came with [#1](https://github.com/joaovitorpina/Navaja/pull/1), the PR-title check and Renovate with [#3](https://github.com/joaovitorpina/Navaja/pull/3), and the licence, gitleaks and Intel macOS checks with [#10](https://github.com/joaovitorpina/Navaja/pull/10). `advisories.yml` runs `cargo deny check advisories` daily, and `renovate.json` waits for approval on majors, Rust releases and the `wdio` group ([#18](https://github.com/joaovitorpina/Navaja/pull/18)).
  3. `navaja-core`: done ([#3](https://github.com/joaovitorpina/Navaja/pull/3)).
  4. `tools/`: done ([#3](https://github.com/joaovitorpina/Navaja/pull/3)). S2.1 is not finished (below).
  5. xtask: done ([#3](https://github.com/joaovitorpina/Navaja/pull/3); `acl` and the ACL snapshot, [#9](https://github.com/joaovitorpina/Navaja/pull/9)). `check::tests::forbidden_crates_under_navaja_tools_fail_the_check` runs the dependency walk on a made-up graph ([#17](https://github.com/joaovitorpina/Navaja/pull/17)).
  6. App crate: done ([#1](https://github.com/joaovitorpina/Navaja/pull/1), [#4](https://github.com/joaovitorpina/Navaja/pull/4), [#6](https://github.com/joaovitorpina/Navaja/pull/6), [#9](https://github.com/joaovitorpina/Navaja/pull/9)).
  7. S2.2 and the front end: done, with two cuts (below). The front end came with [#4](https://github.com/joaovitorpina/Navaja/pull/4), ESLint and Prettier with [#8](https://github.com/joaovitorpina/Navaja/pull/8), and S2.2 with [#12](https://github.com/joaovitorpina/Navaja/pull/12).
  8. Window code: done ([#1](https://github.com/joaovitorpina/Navaja/pull/1), [#6](https://github.com/joaovitorpina/Navaja/pull/6)). The elevation banner and the root refusal have never run on a real elevated or root start (manual-qa.md, check 7).
  9. Tray: done ([#6](https://github.com/joaovitorpina/Navaja/pull/6)), with one exception: its menu labels are English literals (below). S2.3 is not finished (below). No shipped tool sets `tray: true`, so `tray::tests` cover the tool entries; ports is the first real one (M4, item 8).
  10. End-to-end tests: done ([#7](https://github.com/joaovitorpina/Navaja/pull/7)), five spec files with the data-folder canary ([#22](https://github.com/joaovitorpina/Navaja/pull/22)). S2.6 and S2.7 pass.
  11. `adding-a-tool.md`: done ([#3](https://github.com/joaovitorpina/Navaja/pull/3), [#11](https://github.com/joaovitorpina/Navaja/pull/11), and the fixes from the 2026-10-05 tool run).
- **Spikes:**
  - S2.1: not finished. rust-analyzer passes in `spikes.yml`'s `s2-1` job ([#19](https://github.com/joaovitorpina/Navaja/pull/19), [37268969385](https://github.com/joaovitorpina/Navaja/actions/runs/37268969385)); RustRover needs a person (spikes.md, S2.1).
  - S2.2: PASS ([#12](https://github.com/joaovitorpina/Navaja/pull/12), [37146842128](https://github.com/joaovitorpina/Navaja/actions/runs/37146842128), [ADR 0002](adr/0002-views-outside-app.md)).
  - S2.3: not finished. The macOS template icon works on macos-26 at 1× ([#13](https://github.com/joaovitorpina/Navaja/pull/13), [37161147668](https://github.com/joaovitorpina/Navaja/actions/runs/37161147668)); Windows at 100-200 % needs a person (spikes.md, S2.3).
  - S2.6: PASS, 10 of 10 runs per OS, the slowest `os` job 6 min 1 s, on the suite from [#7](https://github.com/joaovitorpina/Navaja/pull/7) ([37149162891](https://github.com/joaovitorpina/Navaja/actions/runs/37149162891)).
  - S2.7: PASS on the process-tree criterion, with the fallback ([#14](https://github.com/joaovitorpina/Navaja/pull/14), [37252894573](https://github.com/joaovitorpina/Navaja/actions/runs/37252894573), [ADR 0003](adr/0003-webview-network.md)).
- **§2's M2a checks:** the data-folder canary runs in every `pnpm e2e` (RUN_MAIN). S2.7 is the NIC capture. Klipper needs a person (manual-qa.md, check 8).
- **Cut:** F6 to cycle panes, which no code handles yet, moves to M3, item 3. The palette's shell commands are cut: the palette searches tools only, and they move to the post-v1 list. The theme override shipped: Settings > Theme offers "Same as the system", "Light" and "Dark".
- **Exception:** the tray menu's labels ("Open Navaja", "Search tools…", "Quit Navaja") are English literals in `tray.rs`, outside `t()`, against §2's definition of done. They go through i18n in M2b, item 1.

**Cut first:** palette shell commands; the theme override; F6.

### M2b · Lifecycle, footprint, bundles (≈2.5 wk)

1. **S2.4** and the StatusNotifier host check in `platform/linux.rs`, then close behaviour per OS through `ShellPlatform`. The tray menu's labels go through i18n too: M2a ships them as English literals in `tray.rs`, an exception to §2's definition of done recorded at M2a's exit.
2. **`xtask measure`**, then **S2.5**. Implement the winning hide policy, delete the other path, and record the numbers in ADR 0004.
3. **Final brand SVGs** and `xtask icons`, which wraps `tauri icon` and also produces the macOS template tray icon. Then run `spikes.yml`'s `s2-3` job once on the new icons and record the run in S2.3. After that, delete the job and its scripts (`s2-3-check.sh`, `s2-3-menubar.swift`, `s2-3-tray.ps1`), and its part of §2's `spikes.yml` row.
4. **`bundle.yml`:** a weekly, keyless build of the exact release matrix. It also checks that the macOS bundle's `Info.plist` sets `LSMinimumSystemVersion` to 14.0 ([ADR 0003](adr/0003-webview-network.md)): no CI job builds a bundle before it.
5. **Stretch, the first thing to cut:** the global-shortcut recorder and `docs/wayland-shortcut.md`.
6. **`CONTRIBUTING.md`**, linking [manual-qa.md](manual-qa.md) for the QA checklist instead of copying it.
7. **macOS clipboard:** keep copies off Universal Clipboard. On macOS, `copy_text` writes through `NSPasteboard` (objc2-app-kit) instead of arboard: `prepareForNewContentsWithOptions(CurrentHostOnly)`, then the text and the ConcealedType marker. arboard's own `set` clears the pasteboard, which would drop that option.
8. **Wayland focus on a second launch.** The single-instance plugin forwards only argv and the working directory over D-Bus, so the launcher's `XDG_ACTIVATION_TOKEN` or `DESKTOP_STARTUP_ID` is lost. GNOME and KDE then refuse to raise or focus the window.
   - **Which windows:** on Wayland, a second launch or the tray's Open can find the window visible but unfocused, minimized, or hidden. Without a token the app can focus none of them, and xdg-shell has no request that restores a minimized window: only the compositor does, when it activates the window. In M2a, on Wayland, a second launch or the tray's Open only has to open what was asked for and show a hidden window again. This item brings focus to all three for a second launch. Tray-menu actions keep the limit after it (the known limitation below).
   - A second launch on Linux checks whether a running Navaja owns the plugin's D-Bus name. If one does, it makes the plugin's `ExecuteCallback` call itself, with the token as an extra argument, and exits with code 0. The plugin stays for the primary.
   - `args::parse` accepts the token only if it is printable ASCII and within a length cap.
   - The primary shows the window, then calls `set_startup_id` with the token on the main thread, instead of `set_focus`, which would mint a token the compositor refuses.
   - **Known limitation:** tray-menu actions cannot take focus, or restore a minimized window, on Wayland. The shell draws the menu (libappindicator), so the app never gets a token for the click.
9. **Single instance on macOS.** tauri-plugin-single-instance 2.5.2 hands off through a fixed `/tmp/<id>_si.sock` that every user shares, with no peer check. Another local user can squat it, and a second user on the same Mac loses single instance.
   - Fix it through a patched plugin (`[patch.crates-io]`): put the socket in the per-user temp dir (`_CS_DARWIN_USER_TEMP_DIR`), check the peer with `getpeereid` on connect and accept, and cap the read at a few KiB.
   - Report the issue upstream.

**Exit:**
- Close behaves correctly per OS, and every quit path works while the window is hidden.
- On GNOME and KDE Wayland, `--show`, `--toggle` and `--tool` raise and focus a window that is visible but unfocused, minimized or hidden.
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
3. **Views:** TransformView; KeyValue, Diagnostics and Binary outputs; ConfirmDialog in view-kit; Vitest and axe for each spec variant. Also F6 to cycle panes (architecture §6), cut from M2a.
4. **JSON:** `tools/json/format.rs`, with proptest against serde_json and fixtures for deep nesting, duplicate keys, huge numbers, surrogates and a BOM. Then **S3.2**, then `mod.rs` and its end-to-end spec.
5. **`tools/base64`, `url`, `hash` and `jwt`:** one PR each, with **nothing under `app/`**.
6. **Re-run S2.5.** Base64 becomes the worked example in `adding-a-tool.md`.
7. **Option values,** before the text tools add Text options. PR #11's first tool found both:
   - Nothing enforces Integer bounds or Text limits at run time. The view applies them only through HTML form validation, and each tool re-checks the bounds it relies on (adding-a-tool §4). Check them in `Registry::run` before `invoke`, or keep the per-tool check and say why in adding-a-tool.
   - `registry_test`'s option probe (`generic_views_inputs_are_accepted`) asserts only that each choice, toggle value and integer bound is not refused with `core.invalid_input`. Make it also fail on `core.invalid_output` and `core.panicked`, as its check of the defaults does.

**Exit:**
- All tools work on three OSes, with 10,000 proptest cases.
- Each tool PR passes `tool-gate` and touches only its folder plus one `lib.rs` line.
- A 10 MB single-line paste causes no long task over 1 s, and a 50 MB paste formats and copies.
- Hash output matches `certutil`, `sha256sum` and `shasum`.
- An expired JWT shows "expired" with the relative time.
- axe reports nothing, and a keyboard-only pass succeeds.

**Cut first:** editor markers; the JWE header view.

### M4 · Port inspector (≈5 wk)

1. **navaja-ports skeleton** (architecture §7). Its types reach the ports view only once `xtask bindings` exports the crate, which today exports `navaja-core` and the app crate (`CRATES` in `xtask/src/bindings.rs`). Adding it is a `host-change` PR, before the ports PR (found by PR #11's first tool).
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
10. **Retire S2.2's job:** once the ports view (`tools/ports/ui/`) and its end-to-end spec run in `ci.yml`, delete `spikes.yml`'s `s2-2` job and its scripts (`s2-2-probe.sh`, `s2-2-check.sh`, `hmr-check.mjs`). Update S2.2's entry, ADR 0002's re-run line and §2's `spikes.yml` row.

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
   - S2.7 again, now with the updater. Then `spikes.yml`'s `s2-7` job moves into the release checks (`release-build.yml`) or is deleted, with its scripts. To move, it needs the Exit's 10 minutes without consent (it idles 5 minutes) and a phase with consent;
   - the full manual matrix ([manual-qa.md](manual-qa.md));
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
| `ci.yml` checks (ubuntu-24.04) | rustfmt (`cargo fmt`, plus `rustfmt --check tools/*/mod.rs` for the tool modules it skips), ESLint and Prettier (`pnpm lint`), svelte-check, cargo-deny on 4 targets, `xtask check` (cargo-deny and every `cargo xtask` run with `--locked`, so a stale `Cargo.lock` fails the job), `bindings --check`, `tool-gate`, the JS licence allowlist (`xtask licenses`), gitleaks over fixtures and snapshots (`scripts/check-fixture-secrets.sh`) |
| `ci.yml` os, ×3 (windows-2025, ubuntu-24.04, macos-26) | clippy → nextest with live tests → doctests → `tauri build --debug --no-bundle` → the same build with `--features e2e` and `e2e.conf.json` → WebdriverIO, whose last spec quits the app and then runs the data-folder check and its negative control (`app/e2e/data-folder-check.mjs`). Linux runs the suite under the strace guard, inside `dbus-run-session -- xvfb-run` with `WEBKIT_DISABLE_DMABUF_RENDERER=1`. The job stops after 45 min, so a hung run fails instead of holding the runner for GitHub's 6 h. macOS also runs clippy, with the same bans and `-D warnings`, on the Intel slice of the universal build: `cargo clippy --workspace --all-targets --target x86_64-apple-darwin -- -D warnings`. That step keys on `runner.os` and fails if the runner is not arm64, since then nothing would lint the arm64 slice. After the builds, `scripts/check-macos-target.sh` reads the minimum macOS back from each file they produced, which must be 14.0 (ADR 0003) |
| `advisories.yml` | Daily at 04:23 UTC, and by hand: `cargo deny --all-features check advisories` on deny.toml's 4 targets, with cargo-deny installed the same way as in `ci.yml` (Renovate moves both install-action pins in one branch). A failed run on main, scheduled or by hand, opens an issue labelled `advisories`, or comments on the open one (`scripts/advisories/issue.sh`). A run by hand on another branch checks that branch and reports nothing. It never blocks a PR. A PR that changes it or `scripts/advisories/` runs the check, then the issue path without writing to any issue: `issue.sh` against a stub `gh` (`issue-test.sh`), the check against a fixture advisory for serde that it must fail on (`fixture-check.sh`), and `issue.sh` as a dry run against the repository's open issues. GitHub turns a scheduled workflow off after 60 days without activity in a public repository; the Actions tab turns it back on |
| `bundle.yml` | Weekly, keyless build of the release matrix: NSIS and zip, universal DMG, deb, rpm and AppImage in `container: ubuntu:22.04` |
| `spikes.yml` | Manual, and on a PR that changes it or `scripts/spikes/`. Spike steps that need no human. Each job's comment ends with a `Lifetime:` line, which says what deletes the job or moves it (spikes.md, Rules). Job `s2-1` (S2.1, the rust-analyzer half), on ubuntu-24.04: the rust-analyzer component of the pinned toolchain, driven over LSP (`scripts/spikes/s2-1-ra-probe.mjs`). It checks go to definition from the `register_tools!` list into `tools/uuid/mod.rs` and from there into `navaja-core`, completion through `crate::uuid::` and inside `mod.rs`, and no unlinked-file or unresolved-path diagnostics for `mod.rs`. Its negative controls are throwaway copies with the `uuid` entry removed (`mod.rs` must be unlinked) or misspelled (go to definition must fail). RustRover needs a person. Job `s2-2` (S2.2), on windows-2025, ubuntu-24.04 and macos-26: a throwaway tool with a custom view (`scripts/spikes/s2-2-probe.sh`) through `cargo test -p navaja-tools`, `pnpm check` (and a type error in a .ts and a .svelte file it must refuse), `pnpm lint` (and forbidden imports in a .ts and a .svelte file, and a misformatted file, it must refuse), `pnpm test`, `pnpm build`, the end-to-end build plus the probe's own spec, an HMR check against the Vite dev server (`hmr-check.mjs`), and an HMR check in the app's webview (a dev build without `custom-protocol` that loads the dev server, plus a second spec that edits the view). Job `s2-3` (S2.3), on macos-26 and windows-2025, runs the plain debug build that `ci.yml` also makes, whose tray code and icons are the release build's (`scripts/spikes/s2-3-check.sh`, `s2-3-tray.ps1`). On macOS it captures the menu bar before the app starts and in the light and the dark appearance, and checks that the template icon is drawn, monochrome and tinted like the clock; its negative controls are the capture from before the app started and a build whose icon is not a template, which must fail on its tint. On Windows it promotes the notification-area icon, finds it through UI Automation, captures it at the runner's 100 %, and checks that an icon is drawn in its button; the negative control is the same check on an empty stretch of the taskbar. The captures are the run's artifact. Job `exit-clipboard-history` (M2a exit: copy stays out of Windows clipboard history), on windows-2025 (`scripts/spikes/exit-clipboard-check.sh`, `clipboard-history.ps1`): Navaja's Copy button through a spec written at run time, the exclusion formats on the clipboard (an ordinary copy must fail that check), and the history checks with their controls. Windows Server keeps clipboard history off, so there the summary marks history off and those checks inconclusive, and the job leaves a warning; the history checks have never run. Job `s2-7` (S2.7), on the same three runners (`scripts/spikes/s2-7-egress.sh`): a 5 min baseline without the app, then the e2e build idles 5 min with its window shown, then `pnpm e2e` runs, each phase under a capture of the VM's traffic. Linux runs the app and WebdriverIO in a network namespace with NAT and a public resolver and captures its veth and loopback with tcpdump, and the host's uplink for comparison with the baseline; after each phase the namespace must still reach the network. macOS captures with tcpdump on pktap, which names each packet's process, and counts the daemons WebKit starts for the app (webprivacyd, the Safe Browsing service) as the app's when they start within 15 s after it. Windows reads the WFP connection, process creation, DNS client and BITS client logs, names the hosts from a pktmon capture, and counts WAM's sign-in service and Microsoft-account provider in the 15 s after an app start as the app's. Traffic of those daemons and services that an entry of `scripts/spikes/s2-7-disclosed.tsv` covers is reported as disclosed, in its own summary row, and fails nothing (ADR 0003); anything else still fails. A control request to github.com (and on Windows a BITS download) must be captured and attributed first, each phase must show the app's own WebDriver traffic, and nothing in the baseline may count as the app's, so an empty capture can't pass and the attribution can't blame the machine's own traffic on the app. Each later phase also lists, for review, the traffic outside the tree that the baseline lacks, and on Windows what follows each app start. The captures are uploaded as an artifact for 14 days. Each check is its own step, and the run summary gets a PASS/FAIL table per OS, plus a findings table per phase for S2.7 |
| `release.yml` + `release-build.yml` | Every push to main runs release-plz, which opens or updates the release PR. Merging that PR runs the build: tauri-action v1, then `verify-release`, attestations, `SHA256SUMS.minisig`, undraft, and bucket and tap updates. `release-build.yml` can also be run by hand on a tag |

**CI hygiene:**
- Runner labels are pinned, never `-latest`; `ubuntu-latest` moves to 26.04 in Oct-Nov 2026. A label moves by hand, in every workflow at once: Renovate does not propose runner labels (`renovate.json`).
- Required checks match by job name, and the os jobs are named after their runner label, such as `os (macos-26)`. So a runner label change in the os matrix must update branch protection's required checks in the same change, or every PR waits for a check that never reports.
- Renovate waits for approval on its Dependency Dashboard before it opens a PR for these: every major or otherwise breaking update, such as a Cargo 0.x minor (pnpm and Node majors included); every Rust release in `rust-toolchain.toml`, which Renovate never counts as a major; and the `wdio` group. Each rule in `renovate.json` says why. The hold applies only to a branch that does not exist yet: Renovate PRs opened before these rules came in ([#18](https://github.com/joaovitorpina/Navaja/pull/18)) stay open until closed by hand.
- Actions are pinned by SHA.
- A tool that no pinned action installs is downloaded at a pinned version and checked against its published SHA-256 before use (gitleaks). Renovate does not bump these; update the version and checksum together.
- Swatinem/rust-cache with a per-OS key, saved only on main.
- Release jobs restore no cache.

**Offline and privacy checks:**
- **On every PR:**
  - an egress canary: fetch, image, beacon, WebSocket, `window.open`, top-level navigation, and WebRTC in the page and in the iframes it creates, all blocked. Fetch, image, beacon and WebSocket must each raise their own CSP violation, since a request that fails for some other reason proves nothing;
  - a Linux strace guard: a `connect`, `sendto`, `sendmsg` or `sendmmsg` to any address other than 127.0.0.1 or ::1 fails the run (the 127.0.0.53 DNS stub included). So does an inet call whose address can't be read, or an app process still running after the suite. The suite never opens a URL or the logs folder: strace would follow the browser or file manager that starts (architecture §5, "Hand-offs to other programs");
  - a canary input at `NAVAJA_LOG=trace`, plus a panicking tool, must not appear in logs, crash files or stderr. The integration test `app/src-tauri/tests/privacy.rs`, run by nextest, checks logs and crash files at trace level; it does not capture stderr yet;
  - the clipboard privacy markers (architecture §5, "Clipboard"): `copy_carries_the_privacy_markers` (`app/src-tauri/tests/clipboard`), run by nextest, copies a random canary through `copy_text`'s own call and reads the markers back through the OS's own API, against an ordinary copy that must carry none. It checks the three Windows formats and macOS's `org.nspasteboard.ConcealedType`. Its Linux part (`x-kde-passwordManagerHint`, over X11, which is also how arboard copies on GNOME Wayland) needs an X display, which CI's nextest step does not have, so on CI it prints why it skipped. It runs only where a developer runs it with a display, e.g. under `xvfb-run` (local commands above);
  - the webview data folder holds no input text. The suite's last spec (`app/e2e/last/data-folder.e2e.ts`) enters a fresh random canary into every text field through WebDriver: the sidebar filter, the palette's search, and any text field on Home, Settings, About and each tool's page. It then quits the app through its `quit` command, and its next test runs `app/e2e/data-folder-check.mjs`, which searches every folder where the end-to-end build keeps data on that OS: `NAVAJA_APP_DIR`, Tauri's folders for the e2e identifier, and the webview's (WebView2's `EBWebView`, WebKit's and WebKitGTK's). The script says why it searches each one.
    - The embedded WebDriver sets each value from script and fires input and change events. No real keystrokes reach the engine, so the check does not cover anything the engine keeps only for trusted keyboard or IME input.
    - Every file is read as raw bytes, binary and compressed ones included. A hit is the canary in UTF-8 or UTF-16LE, or any 12 characters of it in a row, since the Snappy compression in Chromium's LevelDB files can split it. Gzip and zlib files are also searched decompressed.
    - It fails closed: a folder that must exist and is missing, a folder that always holds files and is empty, zero files searched, a file it can't read, or folders still changing a minute after the quit all fail it.
    - Its negative control, the spec's last test, plants seven files in each of those folders and must find the canary in every one before removing them. Together they hold each kind of hit: the whole canary and 12 characters of it, each in UTF-8 and in UTF-16LE, and the whole canary inside a gzip file (in both encodings) and a zlib file (in UTF-8).
    - It is part of the end-to-end suite, so every `pnpm e2e` runs it, as CI's end-to-end step does on each OS (on Linux, inside the strace guard's run).
- **In M2a and M6:**
  - a whole-process-tree NIC capture on each OS (S2.7), without opening the repository link or the logs folder, whose browser or file manager may count as part of the tree. The webview's web traffic goes to a dead proxy on every OS (architecture §5). What system services the webview wakes, and Navaja cannot stop, is on S2.7's disclosed list ([ADR 0003](adr/0003-webview-network.md)): reported apart, never a pass for anything else;
  - copied tokens are absent from clipboard history and Klipper:
    - the markers are checked on every PR on Windows and macOS (above). The Linux one is checked only when a developer runs the check with an X display;
    - a person checks that Windows clipboard history honours them (spikes.md, "M2a exit check");
    - a person checks Klipper during manual QA on KDE Plasma 6 Wayland (below). Its compositor offers data-control, so arboard copies over Wayland there, which no automated check reads;
    - nothing checks the macOS history apps that follow nspasteboard.org yet.

**Manual QA**, at each milestone end, on Windows 11, Ubuntu GNOME Wayland with AppIndicator, Ubuntu X11, Fedora GNOME (no tray), KDE Plasma 6 Wayland and macOS 26. [manual-qa.md](manual-qa.md) has the steps and the results:
- **Shell:** starts hidden; tray menu; close and quit paths; `--toggle` and `--tool`; keyboard-only use; About's repository link opens the browser and Settings' "Open logs folder" opens the file manager (no automated test opens them).
- **Clipboard history, M2a and M6** (Windows 11): Win+V does not list a UUID copied through Navaja, while an ordinary copy is listed (spikes.md, "M2a exit check", by a person).
- **Klipper, M2a and M6** (KDE Plasma 6 Wayland): a UUID copied with Navaja's Copy button, or with Ctrl+C on the output, pastes, and Klipper's history does not list it. An ordinary copy is listed, as the control.
- **Wayland focus, from M2b** (GNOME and KDE): `--show`, `--toggle` and `--tool` raise and focus a window that is visible but unfocused, minimized or hidden. Tray-menu actions may leave it unfocused, or minimized; that is a known limitation (M2b, item 8).
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
- shell commands in the palette, such as Settings and About, cut from M2a;
- more system tools;
- a CLI front end.
