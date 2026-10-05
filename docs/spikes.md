# Spikes

A spike is a short, time-boxed experiment. It settles a question before the code that depends on it is written. [roadmap.md](roadmap.md) lists every planned spike.

## Rules

- Write PASS and FAIL before running anything.
- Keep to the time box. When it runs out, record what you have and decide.
- A gating spike runs before the work it gates.
- A fallback you take becomes an ADR in `docs/adr/`.
- Keep raw outputs (captures, logs, numbers) next to the entry or in `crates/*/tests/fixtures/`, as UTF-8 text such as redacted dumps or JSON. Redact arguments and anything secret. gitleaks checks fixtures and snapshots for secrets, but skips binary captures, archives, UTF-16 text, symlinks and the paths it allowlists, such as an image, a `.bin` or a lockfile (roadmap §2, "Secrets in fixtures").

## Template

Copy this for each spike and fill it in.

```markdown
## Sx.y Title

- **Milestone / gates:** M?, which work it gates
- **Time box:** ? d
- **Question:** the one question this answers
- **Method:** what was run, on which OS, versions, and hardware
- **PASS if:** …
- **FAIL then:** the fallback
- **Result:** PASS / FAIL (date)
- **Numbers and evidence:** tables, links to fixtures
- **Decision:** what changes in the code or docs (ADR link if a fallback was taken)
```

## Results

## S2.1 IDE

- **Milestone / gates:** M2a, item 4: the macro registry. A tool registers with one line in `register_tools!` (`tools/lib.rs`), and the macro's `mod $id;` is the only declaration of the tool's module.
- **Time box:** none set in the roadmap
- **Question:** do RustRover and rust-analyzer navigate and complete through `register_tools!`? An IDE that does not expand the macro sees `tools/<id>/mod.rs` as a file outside the crate, with no navigation or completion in it.
- **Method:**
  - **rust-analyzer, automated** (`.github/workflows/spikes.yml`, job `s2-1`, on ubuntu-24.04): run by hand, and on a PR that changes the workflow or `scripts/spikes/`.
    - The job adds the `rust-analyzer` and `rust-src` components to the toolchain in `rust-toolchain.toml` (1.99.0) and fetches the crates (`cargo fetch --locked`).
    - Then it runs `scripts/spikes/s2-1-ra-probe.mjs` once per check, each in its own step. The probe starts rust-analyzer as an LSP server on the checkout, waits until it reports itself quiescent, asks one thing, and shuts it down. It finds each position by searching the file's text.
    - rust-analyzer runs without build scripts, proc macros, `cargo check` and cache priming. `register_tools!` is a `macro_rules!`, which rust-analyzer expands itself, and the build scripts would compile Tauri. Its experimental diagnostics are on: rust-analyzer 1.99.0 reports an unresolved import (E0432) or name (E0425) only with them.
    - a. Go to definition on `uuid` in the `register_tools!` list lands in `tools/uuid/mod.rs`, and nowhere else.
    - b. In `tools/uuid/mod.rs`, go to definition on `Tool` (`impl Tool for UuidGenerator`) lands on `pub trait Tool` in `crates/navaja-core/src/tool.rs`, and on `ToolMeta` (`fn meta(&self) -> ToolMeta`) on `pub struct ToolMeta` in `meta.rs`.
    - c. Completion after `crate::uuid::` in `tools/lib.rs` offers `TOOL` and `UuidGenerator`. Completion after `navaja_core::` in `tools/uuid/mod.rs` offers `Tool`, `ToolMeta` and `Registry`. The probe asks each inside a function it appends to the file, sent as an unsaved buffer; nothing on disk changes.
    - d. For `tools/uuid/mod.rs` as it is on disk, rust-analyzer reports no unlinked file (`unlinked-file`) and no unresolved import (E0432), module (E0583), extern crate (`unresolved-extern-crate`) or name (E0425). The probe asks for the file's diagnostics (LSP pull diagnostics), since rust-analyzer publishes nothing for a file with none.
    - Two negative controls, each on a throwaway copy of the checkout with `tools/lib.rs` edited. The copy is deleted afterwards.
      - Each first checks that rust-analyzer loaded the copy's workspace: go to definition on `Registry` in `tools/registry_test.rs`, which `lib.rs` declares outside the macro, must land in `crates/navaja-core`. A rust-analyzer that loaded nothing would also fail the checks below, so without this a control could pass on nothing.
      - The `uuid` entry removed (`register_tools! {}`): `tools/uuid/mod.rs` must be reported as an unlinked file, and b and c must fail.
      - The entry misspelled `uiud`: a must fail.
    - The last steps check that the tree is clean, and write a PASS/FAIL table and rust-analyzer's version to the run summary.
  - **RustRover: needs a person.** The steps are below.
- **PASS if:** RustRover and rust-analyzer navigate and complete through `register_tools!`
- **FAIL then:** the two-line form, `mod x;` plus a list entry (needs sign-off)
- **Result:** not finished (2026-10-05).
  - rust-analyzer: PASS. `spikes.yml` run [37254205824](https://github.com/joaovitorpina/Navaja/actions/runs/37254205824), on commit `1b754fe`: checks a to d passed, and both controls failed the checks they should. Every other job of the run passed too.
  - RustRover: not run yet. It needs a person.
- **Numbers and evidence:** run 37254205824, job `s2-1`, on ubuntu-24.04 (Ubuntu 24.04.5, runner image 20260927.320.1), with rust-analyzer 1.99.0 (b940084 2026-09-28) from the 1.99.0 toolchain. The job took 44 s: 3 to 4 s per check, with rust-analyzer quiescent about 1 s after it started. The `s2-1` jobs of runs [37253656398](https://github.com/joaovitorpina/Navaja/actions/runs/37253656398) on `7b2b934` and [37253862965](https://github.com/joaovitorpina/Navaja/actions/runs/37253862965) on `caf86db` got the same answers; a later push cancelled those runs' other jobs. `caf86db` and `1b754fe` have the same probe; `7b2b934` matched the definitions' names as substrings.

  | Check | What rust-analyzer answered |
  |---|---|
  | a | `tools/uuid/mod.rs`, line 1 |
  | b | `Tool`: `crates/navaja-core/src/tool.rs` line 14, `pub trait Tool: Send + Sync + 'static {`. `ToolMeta`: `meta.rs` line 13, `pub struct ToolMeta {` |
  | c | after `crate::uuid::`: 2 items, `TOOL` and `UuidGenerator`; the module's private items are not offered. After `navaja_core::`: 39 items, `Tool`, `ToolMeta` and `Registry` among them |
  | d | 4 diagnostics, none of those d looks for: a `macro-error`, "proc-macro expansion is disabled", at each of the 3 `#[derive(…, Deserialize)]` (lines 17, 25 and 35), and E0277 at line 124, "the trait bound `Input: Deserialize<'?0.0>` is not satisfied", which follows from the same disabled derive |
  | Control, entry removed | `Registry` lands on `registry.rs` line 62. `tools/uuid/mod.rs` has one diagnostic, `unlinked-file`: "This file is not included anywhere in the module tree, so rust-analyzer can't offer IDE services." b finds no definitions, and c no completions |
  | Control, entry misspelled `uiud` | `Registry` lands on `registry.rs` line 62. a's definition of `uiud` is the list entry itself, `tools/lib.rs` line 23, and `lib.rs` gets E0583, "unresolved module, can't find module file: uiud.rs, or uiud/mod.rs" |

  - Locally, on Windows 11 Pro 10.0.26300 with rust-analyzer 1.99.0 (b940084d 2026-09-28), the probe gave the same answers, in about 10 s per check.
  - Each check was also run locally with the property it guards broken, then put back:
    - `tools/uuid/` moved to `tools/uuid_moved/`: a fails.
    - The `uuid` entry removed in the checkout itself: b, c and d each fail, d on `unlinked-file`.
    - `use navaja_core::NoSuchItem;` added to `tools/uuid/mod.rs`: d fails on E0432. With experimental diagnostics off, d passes on the same file, which is why the probe turns them on.
    - `#[path = "uuid/mod.rs"] mod uuid_direct;` added to `tools/lib.rs`, so that `mod.rs` no longer depends on the macro: the removed-entry control fails.
    - Check a weakened to accept any target: the misspelled-entry control fails.
    - `mod registry_test;` commented out: both controls fail their first check. With a line that is not TOML added to `Cargo.toml`, rust-analyzer reports "Failed to load workspaces", and both controls stop with exit code 2.
  - Not covered: rust-analyzer with its defaults (build scripts and proc macros on), as an editor runs it. The registry needs neither, but the RustRover steps below use the IDE's defaults.

  **RustRover, by a person.** On any OS, in a clean checkout of the commit to test:
  1. Open the repository's root folder in RustRover (File > Open), with the default settings. Wait until the Cargo sync and indexing finish. Note RustRover's version (Help > About) and the OS.
  2. a. In `tools/lib.rs`, put the caret on `uuid` in `register_tools! { uuid, }` (line 23) and use Navigate > Declaration or Usages (Ctrl+B; Cmd+B on macOS). PASS: `tools/uuid/mod.rs` opens.
  3. b. In `tools/uuid/mod.rs`, use the same action on `Tool` in `impl Tool for UuidGenerator` (line 52), then on `ToolMeta` in `fn meta(&self) -> ToolMeta` (line 53). PASS: the first opens `crates/navaja-core/src/tool.rs` at `pub trait Tool` (line 14), the second `crates/navaja-core/src/meta.rs` at `pub struct ToolMeta` (line 13).
  4. c. At the end of `tools/lib.rs`, type `fn s21() { let _ = crate::uuid::` and invoke completion (Ctrl+Space) right after the last `::`. PASS: the list offers `TOOL` and `UuidGenerator`. Undo the edit. Then type `fn s21() { let _ = navaja_core::` at the end of `tools/uuid/mod.rs`. PASS: the list offers `Tool`, `ToolMeta` and `Registry`. Undo the edit.
  5. d. In `tools/uuid/mod.rs`, PASS when the editor shows no "File is not included in module tree" banner, and Navigate > Declaration on `navaja_core` in `use navaja_core::{` (line 3) opens `crates/navaja-core/src/lib.rs`.
  6. Negative control. In `tools/lib.rs`, change `uuid,` to `uiud,` and wait for the analysis. PASS: `tools/uuid/mod.rs` shows the "not included in module tree" banner, step 3's action on `Tool` no longer opens `tool.rs`, and the action on `uiud` does not open `tools/uuid/mod.rs`. Put the line back (`git checkout tools/lib.rs`), wait, and check that step 3 works again.
  7. Record here the RustRover version, the OS, and PASS or FAIL for steps 2 to 6, with a screenshot of any failure.

  Optionally, the same steps in VS Code with the rust-analyzer extension at its defaults (F12 goes to the definition, Ctrl+Space completes; its "unlinked file" warning is the banner) cover what the job leaves out.
- **Decision:** pending until both halves are recorded here.
  - If RustRover passes steps 2 to 6, `register_tools!` stays as it is.
  - If either IDE fails, the fallback, `mod x;` plus a list entry, needs the maintainer's sign-off and becomes an ADR (Rules).
  - **The job's lifetime:** `spikes.yml`'s `s2-1` job is deleted once S2.1 is recorded here, RustRover half included, together with `scripts/spikes/s2-1-ra-probe.mjs`.

## S2.2 Views outside `app/`

- **Milestone / gates:** M2a, custom-view discovery (`import.meta.glob('@tools/*/ui/View.svelte')` in ToolHost) and the custom views that follow it
- **Time box:** ½ d
- **Question:** can a custom view and its tests live in `tools/<id>/ui/`, outside `app/`, and still be type-checked, tested, built, hot-reloaded and packaged like code in `app/src`?
- **Method:** Windows 11 Pro 10.0.26300, Node 24.15.0, pnpm 11.28.3, Vite 8.3.2, Vitest 5.0.3, svelte-check 4.7.6, Svelte 5.57.1. A throwaway `tools/probe/ui/` held `View.svelte` (importing `$lib/view-kit`, `$bindings/Category` and `./label.ts`), `label.ts`, and `View.test.ts` (importing `vitest`, `@testing-library/svelte` and `@tauri-apps/api/mocks`). Runs: a view with a type error plus a test that fails on purpose; a view importing a missing `./` module; a correct view with a passing test, then `vite build`; a view using a Tailwind class found nowhere else, then `vite build`. Each resolution setting below was also removed once to confirm it is needed. The probe was deleted afterwards.
  - **ESLint and Prettier** (later on 2026-10-03; ESLint 10.11.0, typescript-eslint 8.71.0, eslint-plugin-svelte 3.23.0, Prettier 3.9.9, prettier-plugin-svelte 4.1.1): a second throwaway `tools/probe/ui/` held `View.svelte`, `label.ts`, `View.test.ts` and `sub/helper.js`, with one allowlist violation of each kind plus `/* eslint-disable */`, `// eslint-disable-next-line` and `<!-- eslint-disable -->` comments. The real `pnpm lint` ran on it, then the Prettier half of the script on its own, since the ESLint half stops the script first. The layouts that fail were tried too: the config only in `app/`, and a config file planted in `tools/probe/ui/`. That probe was deleted as well.
  - **Automated run** (`.github/workflows/spikes.yml`, job `s2-2`): run by hand, and on a PR that changes the workflow or `scripts/spikes/`, on windows-2025, ubuntu-24.04 and macos-26.
    - `scripts/spikes/s2-2-probe.sh create` writes a throwaway system tool with a custom view, `tools/probe/`, laid out as [adding-a-tool.md](adding-a-tool.md) describes, and adds `probe,` to `tools/lib.rs`. Nothing commits it.
    - Its view imports `$lib/view-kit`, a `$bindings` type and its sibling `label.ts`. On mount it runs its one action through `runTool` and shows the fixed answer. It uses one Tailwind class found nowhere else in the repository, `tracking-[0.4242em]`.
    - The script also writes two end-to-end specs to `app/e2e/spikes/`, `s2-2-probe.e2e.ts` and `s2-2-hmr.e2e.ts`. `pnpm e2e` runs only `app/e2e/specs/`, so each runs only when named with `--spec`.
    - Then one step per check, in this order. A failed check does not stop the ones after it.
      - Registry: `cargo test -p navaja-tools`, whose output must include the probe's own test.
      - `pnpm check` passes. Then its negative control: `s2-2-probe.sh typecheck` adds `tools/probe/ui/typed.ts` and `Typed.svelte`, each with `const n: number = 'x'`, and svelte-check alone (`--output machine`, from `app/`) must exit non-zero with exactly 2 errors in its `COMPLETED` line, an `ERROR` line naming each file, and "not assignable". Without the control, a clean svelte-check would read the same if the tsconfig's `../tools/*/ui` includes matched nothing.
      - `pnpm lint` passes (ESLint, then Prettier). Then two negative controls. `s2-2-probe.sh forbid` adds `forbidden.ts` and `Forbidden.svelte`, each importing `$lib/ipc`, and `pnpm lint` must fail with exactly those two errors, both from `navaja/view-imports`. `s2-2-probe.sh misformat` adds `misformatted.ts`, with double quotes and extra spaces that ESLint accepts, and `pnpm lint` must fail in Prettier, which runs only once ESLint has passed: a `[warn]` line naming that file and no other, and "Code style issues found".
      - Each negative control removes its files on exit, pass or fail. It prints the output it reads behind a `  | ` prefix, so that setup-node's ESLint problem matcher does not turn the deliberate errors into annotations on a green run.
      - `pnpm test`, whose JSON report must list `tools/probe/ui/View.test.ts` as passed.
      - `pnpm build`: the view's code must sit in a chunk other than the entry, and its class in the emitted CSS.
      - The end-to-end build (`pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/e2e.conf.json`), then the probe's spec alone. The spec opens `#/tool/probe` and expects the label, the answer from Rust and the class's letter spacing. On Linux it runs inside `dbus-run-session` and `xvfb-run`, as in `ci.yml`, but without the strace guard, which runs only the whole suite.
      - HMR at the dev server: `scripts/spikes/hmr-check.mjs` starts the app's Vite dev server on a free port. It requests `ToolHost.svelte`, the view its glob names and the view's `label.ts`, and connects to the HMR WebSocket. It edits `View.svelte`, then `label.ts`, and waits up to 20 s for an `update` for each, then fetches the updated modules. No webview runs, so nothing re-renders the view.
      - HMR in the app's webview. First a dev build: `TAURI_CONFIG="$(cat app/src-tauri/e2e.conf.json)" cargo build -p navaja --features e2e`, the same app and e2e overlay as above, without Tauri's `custom-protocol` feature, which `tauri build` always turns on. It writes the same `target/debug/navaja`, so it runs after the packaged check. Without that feature `tauri::is_dev()` is true (tauri 2.12.1, `lib.rs`): the app loads `build.devUrl`, `http://localhost:1420`; the guard allows that origin (`guard.rs`); and on Linux the window gets no dead proxy (`window.rs`). Tauri would pick `devCsp` then, but it sets a CSP only on the pages it serves itself (`manager/mod.rs`, `get_asset`), and on desktop the dev server's page comes straight from Vite, so no CSP applies there.
      - Then `s2-2-check.sh hmr-app` starts `app/`'s own Vite on port 1420 (`strictPort`, from `vite.config.ts`), fails if something else answers there first, and runs `s2-2-hmr.e2e.ts` through the same WebdriverIO harness, with `NAVAJA_E2E_BINARY` naming the dev build (on Linux inside `dbus-run-session` and `xvfb-run`). The spec checks that the page's origin is `http://localhost:1420` and that the probe view shows the answer from Rust. Then it edits `label.ts` from Node, then `View.svelte` (a new `<p>`), and waits up to 20 s for the page to show each edit. Before each edit it sets a mark on the page's `window`, which a reload replaces, and a `MutationObserver` that notes when the edit shows; the mark must still be there afterwards. After the `View.svelte` edit the view must show the answer from Rust again, from its own new `run_tool` call. The spec and the script's exit both put the files back, and the script stops Vite.
    - The last step writes a PASS/FAIL table per OS to the run summary.
    - `scripts/spikes/s2-2-check.sh` holds the checks that take more than one command, so a local run does what CI does. It keeps every log in one temporary folder per run, removed on exit.
- **PASS if:** HMR, build, packaged app, svelte-check, ESLint and Vitest all work on 3 OSes
- **FAIL then:** tsconfig and Vitest aliases; otherwise `app/src/tools/<id>/` (needs sign-off)
- **Result:** PASS (2026-10-03), through the first fallback: the tsconfig, Vite and Tailwind settings below, plus ESLint's and Prettier's root configs. [ADR 0002](adr/0002-views-outside-app.md) records them. `spikes.yml` run [37146842128](https://github.com/joaovitorpina/Navaja/actions/runs/37146842128), on commit `dbf31d5`, passed every check on windows-2025, ubuntu-24.04 and macos-26, the negative controls and the HMR check in the app's webview included.
- **Numbers and evidence:**

  | Check (Windows) | Without settings (PR #4 head) | With settings |
  |---|---|---|
  | svelte-check, type error in the view | missed: `tsconfig` included only `src/**` | caught: `Type 'string' is not assignable to type 'number'` |
  | svelte-check, missing `./` import | missed | caught: `Cannot find module './does-not-exist'` |
  | svelte-check, test importing `vitest` or `@tauri-apps/api/mocks` | `Cannot find module` | 0 errors, 0 warnings |
  | Vitest, failing test | not run: `include` was `src/**` only | runs and fails |
  | Vitest, test importing `@testing-library/svelte` | `Failed to resolve import` | passes (render, click, `copy_text` mocked) |
  | `vite build`, the view | builds (these settings don't affect it) | its own lazy chunk (`View-*.js`) |
  | `vite build`, a Tailwind class used only in the view | missing from the CSS: `@source '../../tools/*/ui'` matched nothing | in the CSS with `@source '../../tools/*/ui/**/*.{svelte,ts}'` |
  | ESLint, a view file, config only in `app/`, run from `app/` with `--config` | `File ignored because outside of base path`, a warning: exit 0 | — |
  | ESLint, the same, run from `app/` with config lookup | `ESLint couldn't find an eslint.config.* file`: exit 2 | — |
  | ESLint, every violation in the probe (`pnpm lint`) | — | all reported: 13 `navaja/view-imports` errors in 4 files, plus 3 from the base rules (`svelte/no-svelte-internal`, and `no-require-imports` and `no-undef` on `require`); the allowed imports in the same files pass; exit 1 |
  | ESLint, `/* … */` and `// eslint-disable…` in a view | — | ignored; ESLint warns each "has no effect because you have 'noInlineConfig' setting", which `--max-warnings 0` fails |
  | ESLint, `<!-- eslint-disable -->` above an `import()` in markup | — | ignored: the import is still reported |
  | ESLint, `export default []` planted as `tools/probe/ui/eslint.config.js` | with config lookup it wins: the view's files come out as `File ignored because no matching configuration was supplied` | `--config eslint.config.mjs` ignores it; the violations are reported |
  | Prettier, the probe files | — | the shared config applies, through `--config` and through lookup of the root `prettier.config.mjs` alike (the single-quoted `.ts` files pass), and `.svelte` goes through the plugin: the over-long line in `View.svelte` fails the check, exit 1 |
  | `spikes.yml` scripts, run locally: registry, `pnpm check`, `pnpm lint`, `pnpm test`, `pnpm build` | — | all pass. `cargo test -p navaja-tools`: 15 passed, the probe's own test among them. svelte-check: `COMPLETED 663 FILES 0 ERRORS 0 WARNINGS`; a type error added to `label.ts` and one added to the end-to-end spec were caught (svelte-check and `tsc -p e2e/tsconfig.json`). Vitest: 240 of 240 tests in 12 files, `View.test.ts` among them. Build: the view in `View-Dm_h5rJo.js` (1,090 bytes; the entry, `index-FKn6RcoJ.js`, is 144.04 kB), its class in the CSS. Without the probe the build has neither, although the docs and `scripts/spikes/` name the class |
  | `spikes.yml`, svelte-check's negative control (`const n: number = 'x'` in `tools/probe/ui/typed.ts` and in `Typed.svelte`) | with the tsconfig's two `../tools/*/ui` includes removed: exit 0, `COMPLETED 659 FILES 0 ERRORS`, so the control fails | exit 1, `COMPLETED 665 FILES 2 ERRORS 0 WARNINGS 2 FILES_WITH_PROBLEMS`: `Type 'string' is not assignable to type 'number'` once in `..\\tools\\probe\\ui\\typed.ts` and once in `..\\tools\\probe\\ui\\Typed.svelte` (machine output escapes the back slashes) |
  | `spikes.yml`, the forbidden imports (`$lib/ipc` in `tools/probe/ui/forbidden.ts` and `Forbidden.svelte`) | — | `pnpm lint` exits 1 with `2 problems (2 errors, 0 warnings)`, both from `navaja/view-imports`, naming `'$lib/ipc'` |
  | `spikes.yml`, Prettier's negative control (`export const misformatted   =   "double quotes";` in `tools/probe/ui/misformatted.ts`) | — | ESLint passes; then Prettier lists `[warn] tools/probe/ui/misformatted.ts` and no other file, then `Code style issues found in the above file`, and `pnpm lint` exits 1. Prettier colours `[warn]` whenever `FORCE_COLOR` is set, even to 0, and always on Windows, so the check sets `NO_COLOR=1` too |
  | Packaged app: the end-to-end build, then the probe's spec (WebdriverIO reports msedge 154.0.0.0) | — | passes: the view shows its label, "Answer from Rust" from its own `run_tool` call, and a non-zero letter spacing from its class. The same spec expecting another answer fails with `Received: "Answer from Rust"`. A one-off probe in the spec: the page, `http://tauri.localhost`, blocks an inline `<script>` added to it (CSP `script-src 'self'`) |
  | HMR at the dev server, `View.svelte` edited (`hmr-check.mjs`, no webview) | — | an `update` (`js-update`) for the view 10-20 ms after the write, in each of 9 runs; the module at that update's timestamp holds the edit |
  | HMR at the dev server, `label.ts` edited | — | an `update` for `View.svelte`, the module that accepts it, 1-2 ms after the write; the view at that timestamp imports `label.ts?t=…`, which holds the edit. With a 5 ms timeout the check fails and restores both files |
  | HMR in the app's webview (`s2-2-hmr.e2e.ts`, the dev build; WebdriverIO reports msedge 154.0.0.0) | — | passes in each of 4 runs. The page's origin is `http://localhost:1420`, and the view shows "Answer from Rust". The `label.ts` edit shows 65-85 ms after the write and the `View.svelte` edit 70-80 ms after it, by the page's own clock (a `MutationObserver`); the `window` mark survives both, and after the `View.svelte` edit the view shows "Answer from Rust" again. For each edit Vite logs an `hmr update` for `View.svelte` and `/src/app.css` (Tailwind rescans the view), and no reload. A one-off probe: an inline `<script>` added to the page runs, so no CSP applies to the dev server's page |
  | HMR in the app's webview, negative control (local only) | — | the same spec with `setTimeout(() => location.reload(), 0)` run right after the `label.ts` edit fails that test: `Expected: "label-…"`, `Received: null`. The new text still shows, from the reloaded page, so only the mark tells a reload from HMR |

  The "without" column for the type error, the missing import and the failing test comes from the PR #4 review, which ran the same probe on the PR head. For the ESLint rows, "without" means the config sits in `app/` alone, or a tool folder holds its own; for svelte-check's negative control, it means the tsconfig without the `tools/` includes. The `spikes.yml` rows come from its scripts, run locally on the Windows machine above (Node 24.15.0, pnpm 11.28.3, Tauri 2.12.1, WebdriverIO 9.32.0, `@wdio/tauri-service` 1.4.0, vite-plugin-svelte 7.3.1), not from CI. The other rows were run here.

  The last three local runs went through the job's steps in order, as `spikes.yml` runs them, in 85, 78 and 79 s, the first with Vite's dependency cache cleared, as on CI: Vitest 16-17 s, the incremental end-to-end build 8-12 s and its spec 8 s, the in-app HMR step 12 s (Vite's start and the app's included), svelte-check 6 s, each negative control 4-5 s, the dev build 2 s (Cargo already held both builds of Tauri; a cold runner compiles Tauri and the crates above it once more), and 5 s or less for each of the others. Every step passed each time, and after `s2-2-probe.sh remove`, `git status` was clean.

  The dev-server HMR times run from the write to the update message, so they cover Vite's file watcher and its HMR pass, not a re-render. Vite names the module that accepts an update, not always the file that changed: `label.ts` accepts no updates itself, so its edit updates the view that imports it. The in-app times run from the write to the edited text in the page's DOM, so they also cover the Vite client's fetch of the new module and Svelte's re-render; Node and the webview read the same clock.

  The first CI run, [37143449997](https://github.com/joaovitorpina/Navaja/actions/runs/37143449997), on commit `ad7b612` (2026-10-03), passed on all three OSes. It ran the steps as they were before the negative controls for svelte-check and Prettier and before the in-app HMR check, so it has no numbers for those:

  | Runner | Result | Webview, as WebdriverIO reports it | Job | End-to-end build | Probe spec | HMR at the dev server: `View.svelte`, `label.ts` |
  |---|---|---|---|---|---|---|
  | windows-2025 | pass | msedge 153.0.0.0 (WebView2) | 4 min 47 s | 1 min 23 s | 121 ms | 13 ms, 1 ms |
  | ubuntu-24.04 | pass | WebKitGTK 605.1.15 (apt installed libwebkit2gtk-4.1-0 2.52.6) | 3 min 9 s | 15 s | 119 ms | 11 ms, 1 ms |
  | macos-26 | pass | webkit 605.1.15 | 2 min 45 s | 28 s | 136 ms | 139 ms, 59 ms |

  On all three: `cargo test -p navaja-tools` passed 15 tests, the probe's own among them; svelte-check found 0 errors and 0 warnings; the forbidden imports made `pnpm lint` exit 1 with `2 problems (2 errors, 0 warnings)`; Vitest passed 240 of 240 tests in 12 files, `View.test.ts` among them; and the build put the view in `View-Dm_h5rJo.js` (1,090 bytes; entry `index-FKn6RcoJ.js`, 144.04 kB) and its class in `index-deLtt2pA.css`, the same files on every OS. 605.1.15 is the version WebKit puts in its user agent, frozen there, so it does not identify the build. The deliberate lint failure printed ESLint's output as it was, and setup-node's problem matcher turned it into 6 error annotations on the green run, 2 per OS; the checks now print it behind a prefix.

  The run with every check, [37146842128](https://github.com/joaovitorpina/Navaja/actions/runs/37146842128), on commit `dbf31d5` (2026-10-03), passed every step on all three OSes:

  | Runner | Result | Webview | Job | Probe spec | HMR at the dev server: `View.svelte`, `label.ts` | HMR in the app's webview: `label.ts`, `View.svelte` |
  |---|---|---|---|---|---|---|
  | windows-2025 | pass | msedge 153.0.0.0 (WebView2) | 5 min 26 s | 111 ms | 13 ms, 1 ms | 71 ms, 74 ms |
  | ubuntu-24.04 | pass | WebKitGTK 605.1.15 | 3 min 10 s | 109 ms | 11 ms, 1 ms | 29 ms, 28 ms |
  | macos-26 | pass | webkit 605.1.15 | 2 min 54 s | 105 ms | 127 ms, 116 ms | 151 ms, 34 ms |

  On all three:
  - svelte-check's control reported `COMPLETED 665 FILES 2 ERRORS`;
  - the forbidden imports gave `2 problems (2 errors, 0 warnings)`;
  - Prettier's control named only `misformatted.ts`;
  - Vitest passed 240 of 240 tests;
  - the view was built into `View-Dm_h5rJo.js` (1,090 bytes);
  - the in-app spec passed its 3 tests, with the `window` mark kept through both edits.

  The resolution failures come from pnpm's isolated layout: it installs packages under `app/node_modules` only, so bare imports from `tools/` find nothing. `svelte` and `$bindings/*` were not affected (vite-plugin-svelte dedupes `svelte`, and `$bindings` is an alias).

  ESLint 10 looks for a config from each linted file's folder upwards, so files in `tools/` never find one in `app/`. With `--config`, the base path is the working directory, and it skips files outside it with only a warning. Prettier also looks its config up from each file, and it loads plugins named as strings from the working directory.
- **Decision:** keep custom views in `tools/<id>/ui/`; the `app/src/tools/<id>/` fallback is not needed so far. The settings that make it work, all in `app/` apart from the two root config files that load ESLint's and Prettier's:
  - `tsconfig.json` includes `../tools/*/ui/**/*.ts` and `../tools/*/ui/**/*.svelte`, and maps `vitest`, `@testing-library/svelte` and `@tauri-apps/api/*` to `./node_modules/` through `paths`.
  - `vite.config.ts` adds `../tools/*/ui/**/*.{test,spec}.ts` to Vitest's `include` and, under Vitest only, sets `resolve.dedupe` to those three packages so they resolve from `app/`. A view test that needs another package must be added to both lists, and to the allowlist's test packages in `app/eslint/view-imports.js`.
  - `app.css` names the view files in its `@source` glob; a glob ending in a directory finds no classes.
  - `$lib/view-kit` exists and is what views import from the shell. ToolHost types the glob against its `ViewProps`.
  - ESLint's config is `app/eslint/config.js`, loaded by the root `eslint.config.mjs`. Its patterns are relative to the repository root and cover `app/` and `tools/*/ui/`, so the root must be ESLint's base path. `pnpm lint` runs from the root with `--config eslint.config.mjs`: with `--config` the base path is the working directory, and a config file placed under `tools/` is never used. Editors use config lookup instead, which finds the root `eslint.config.mjs` and takes its folder as the base path. The import allowlist (architecture §4) is in place, and `app/eslint/config.test.ts` runs the real config through ESLint's Node API on a file under `tools/`, so a change that drops `tools/` fails a test.
  - Prettier's config is `app/prettier.config.js`, loaded by the root `prettier.config.mjs`. It resolves the Svelte plugin from `app/`.
  - S2.2 passed on all three OSes, so these settings are the decision: [ADR 0002](adr/0002-views-outside-app.md). No CI job outside `spikes.yml` checks a custom view: the repository holds none, and `ci.yml` runs `pnpm lint` on Linux only. Re-run `spikes.yml` by hand after upgrading Vite, Vitest, svelte-check, ESLint, Prettier or Tailwind.

## S2.3 Tray icon

- **Milestone / gates:** M2a, the tray icon format per OS
- **Time box:** ½ d
- **Question:** which icon files keep the tray icon crisp at every scale, and does the macOS template icon work?
- **Method:** the minimal tray from roadmap M2a, item 9 (`tray.rs`), with its placeholder icons: `icons/tray/template.png` (64 × 64, black on transparent) as a template image on macOS, and `icons/tray/color-32.png` (32 × 32, colour) elsewhere.
  - **Automated** (`.github/workflows/spikes.yml`, job `s2-3`, on macos-26 and windows-2025): run by hand, and on a PR that changes the workflow or `scripts/spikes/`. Each runner builds the app as shipped, `pnpm tauri build --debug --no-bundle`, without the end-to-end overlay, and starts `target/debug/navaja` with its own `NAVAJA_APP_DIR`. The app creates its window hidden and shows it once the front end is ready (`window.rs`), so the window is on screen too; the checks look only at the tray.
  - **macOS** (`scripts/spikes/s2-3-check.sh`, with `s2-3-menubar.swift`, compiled once with `swiftc`), one step per check:
    - The helper prints the display's size and scale. It finds the menu bar's status items through `CGWindowListCopyWindowInfo`: on-screen windows at the status-window level (layer 25), at the top of the display, at most 50 points tall and narrower than 400 points. The Window Server's own windows up there, at layer 2147483630, are not status items. They come and go between steps (in run 37153145013, two before the app started and none by the control build), so the count in the next step leaves them out. The helper's `items` listing in the log prints them too, each with its layer.
    - It sets the light appearance through System Events and reads it back from `defaults read -g AppleInterfaceStyle`. Then it captures the whole display with `screencapture -x` before the app starts.
    - It starts the app and waits up to 60 s for its status item. On macOS 26 Control Center owns every status item window, the app's too, so the owner's pid cannot pick it out. A new item goes to the left of the others, so the app's is the one that raises the count by one and sits left of the leftmost item from before. The helper waits until that item's rectangle has held for 1 s, since it changes size once its image is set.
    - It captures the display in the light appearance, then in the dark one, reading each back. It saves crops: the menu bar's right half, and the icon eight times larger without smoothing.
    - Presence: in the item's rectangle, the background is the median of its outermost ring of pixels. Ink is any pixel at least 48 of 255 away from it in some channel. The check passes when ink covers at least 3 % of the rectangle and the strongest contrast is at least 96. It runs on the light and the dark capture.
    - Presence, negative control: the same check, at the same rectangle, on the capture taken before the app started, must fail.
    - Tint: the core of the ink (pixels at 60 % or more of the strongest contrast) is compared with the core of a system item's, the rightmost other status item (the clock). In each appearance the icon must be monochrome: its core's saturation (the mean of max − min over its channels) at most 24, or at most 12 above the clock's own core where that allows more, since the menu bar's vibrancy may tint both. It must also be within 48 luma of the clock's text, and on the same side of the background. Its luma must also change by at least 100 between the appearances.
    - Tint, negative control: the app is quit, and its status item must go with it. Then the same app is built again with `icon_as_template(false)` in `tray.rs`, which the script then puts back from a saved copy. It deletes the copy once `tray.rs` matches it. That build's icon goes through the same captures, and the tint check must fail, on the icon's tint only. The helper ends a failed check with one error line that lists its reasons. Each must be about the icon's luma against the clock's, its side of the background, no icon in the dark capture, or too small a change between the appearances, and at least one must be about the dark appearance or that change. Any other reason fails the control: no icon in the light capture, a reference item that shows nothing, an icon that is not monochrome, or a capture it cannot read.
    - The last step quits the app, puts the appearance back, and checks that the tree is clean. If the control build was killed with its edit in place, that step also puts `tray.rs` back, but only while the file is exactly the saved copy with that one edit. Any other change to `tray.rs` is left alone.
  - **Windows** (`scripts/spikes/s2-3-tray.ps1`, Windows PowerShell 5.1):
    - Before the app starts, UI Automation must find no notification-area button named Navaja (`AutomationId` `NotifyItemIcon`). It must find other taskbar buttons, so that the check does not pass on nothing.
    - The app starts. Windows 11 puts a new icon in the overflow, behind the chevron. The script waits for Explorer's record of the icon under `HKCU\Control Panel\NotifyIconSettings` and sets `IsPromoted = 1` there, as the Settings page does. Then UI Automation must find the button within 30 s. If it does not, the step prints the app's output.
    - The record is the one whose `ExecutablePath` is this checkout's `target\debug\navaja.exe`. The comparison ignores case, and a known-folder GUID at the start of the path counts as its folder. Where two records match, the one that did not exist before the app started wins. The script saves the record's previous `IsPromoted` value, and the last step puts it back. Another Navaja's record is never touched.
    - The script captures the display (`Graphics.CopyFromScreen`, the process DPI-aware), the taskbar's right end, and the icon eight times larger. It prints the display's DPI. It does not change the display's scale.
    - Presence: the macOS rule, inside Navaja's button, 4 pixels in from each side. The button is as tall as the taskbar, and the inset leaves out the taskbar's 1-pixel top edge, a grey 48 levels from the background.
    - Presence, negative control: the same check, on the same capture, at a stretch of the taskbar as large as the button where UI Automation lists nothing, must fail. The script takes the leftmost such stretch, ignoring elements half the taskbar's width or wider, which are containers. A capture from before the app starts cannot serve: there the chevron sits where the icon appears later.
  - The run's artifact, `s2-3-<runner>`, is kept for 30 days. It holds every capture, the rectangles the checks measured (`*.rect`) and the app's output. Windows' `present` and `present-refuses` read only the capture and the rectangles, so they can run again on a downloaded artifact, with `S23_DIR` set to its folder. Runs up to 37153145013 uploaded only the `.png`, `.txt` and `.log` files.
  - **By a person:** the Windows scales from 100 to 200 %, below.
- **PASS if:** crisp at 100-200 % on Windows; the macOS template icon works
- **FAIL then:** per-scale PNGs, or `with_inner_tray_icon` with an .ico
- **Result:** not finished (2026-10-03).
  - macOS template icon: works on the macos-26 runner, in the light and the dark appearance, at its 1× scale. Retina (2×) was not tested.
  - Windows: the icon shows in the notification area at the runner's 100 %. UI Automation lists its button, and a pixel check finds an icon drawn in it. No check measures crispness: whether the icon is crisp at 100-200 % needs a person.
- **Numbers and evidence:** `spikes.yml` run [37150170804](https://github.com/joaovitorpina/Navaja/actions/runs/37150170804), on commit `78efc60`. Every step passed on both runners. Runs [37150853141](https://github.com/joaovitorpina/Navaja/actions/runs/37150853141) on `40707c0` and [37151163157](https://github.com/joaovitorpina/Navaja/actions/runs/37151163157) on `b6c57a9`, which changed only these docs, gave the same rectangles and icon measurements. The clock's differ a little from run to run, since its text changes: in those three runs its core was `#3b3b3b` to `#3c3c3c` in the light menu bar and `#cfcfcf` to `#d1d1d1` in the dark one.
  - Run [37152739410](https://github.com/joaovitorpina/Navaja/actions/runs/37152739410), on `913ef25` (commit `dd2f566` merged with main), added the Windows pixel check and its negative control, and the stricter reading of the tint control. Every step passed on both runners again, with the same rectangles and icon measurements. The clock's core was `#3a3a3a` in the light menu bar and `#d1d1d1` in the dark one. Jobs: 2 min 34 s on macos-26, 2 min 40 s on windows-2025.
  - Run [37153145013](https://github.com/joaovitorpina/Navaja/actions/runs/37153145013), on `18920bc`, which changed only these docs, gave the same again; the clock's core was `#3c3c3c` and `#d1d1d1`.
  - Run [37161147668](https://github.com/joaovitorpina/Navaja/actions/runs/37161147668), on `e4cb742`, ran the scripts with the fixes from review: the control build's short-lived copy of `tray.rs`, the exact match and restore of Explorer's record, and the larger artifact. Every step passed on both runners. Jobs: 3 min 41 s on macos-26, 2 min 43 s on windows-2025.
    - macOS: the clock was 148 points wide, 8 more than before, so every item sat 8 points further left, the app's at x = 769. The icon's measurements were the same. The clock's core was `#3e3e3e` in the light menu bar and `#cfcfcf` in the dark one. The tint control failed for the same four reasons.
    - Windows: the button, the pixel check and the empty stretch were the same as in run 37152739410. Explorer's matching record was new, with no `IsPromoted` value, and the last step removed the value again. The artifact held the 4 captures, `icon.rect`, `empty.rect`, `navaja.out` and `navaja.err`.
    - The commit that records this run changes only these docs.

  | Runner | System | Display | Job |
  |---|---|---|---|
  | macos-26 | macOS 26.6.2 (25G83) | 1024 × 768 points, 1024 × 768 pixels: backing scale 1.0, not Retina. NSStatusBar reports 22 points; the status item windows are 30 points tall | 2 min 4 s |
  | windows-2025 | Windows Server 2025 Datacenter, build 26100; session 2, interactive, with Explorer | 1024 × 768 pixels, AppliedDPI 96: 100 % | 3 min 27 s |

  macOS, from the helper's measurements:

  | Capture | Menu bar background | Icon: ink, glyph size | Icon core | Clock's text core |
  |---|---|---|---|---|
  | Before the app starts, at the icon's later rectangle | `#dadada` | none: 0 % ink, contrast 0 | — | — |
  | Shipped build, light | `#dadada` | 10.3 % of a 34 × 30 pixel item, 13 × 13 pixels | `#2f2f2f`, saturation 0 | `#3b3b3b`, saturation 0 |
  | Shipped build, dark | `#202020` | 10.3 %, 13 × 13 pixels | `#d9d9d9`, saturation 0 | `#d1d1d1`, saturation 0 |
  | Not-template build, light | `#dadada` | 9.5 %, 13 × 12 pixels | `#111111` | `#3b3b3b` |
  | Not-template build, dark | `#202020` | none: contrast 32, under the 48 that counts as ink | — | `#d1d1d1` |

  - The appearance read back as Light, then Dark. The menu bar's background followed it, from `#dadada` to `#202020`.
  - The app's item appeared at x = 777 points (769 in run 37161147668, with a wider clock), left of Spotlight, Control Center and the clock, which stayed where they were. It was gone within 10 s of quitting the app, for both builds.
  - The tint check failed on the not-template build as it should, on the dark appearance: no ink in the dark capture (contrast 32), so no icon drawn. With no ink there is no core to measure, and the helper then reports a core of `#000000`, luma 0. Its three other reasons use that stand-in value, not a measurement: in the dark, the luma against the clock's 209 and the side of the background; and the change of 17 between the appearances. In run 37152739410 the control also read the helper's reasons: all four were about the icon's tint.
  - What the captures show (looked at by eye, the crops at 8×): in the light menu bar the icon is a dark grey silhouette of the knife, the same grey as the clock and the other icons. In the dark one it is the same silhouette in light grey, again matching the clock. It is one flat tone, with no colour. At this 1× scale the 64-pixel glyph is drawn 13 pixels high. Its outline is clean, with one pixel of grey at the edges and no blur. The blades still show as three points at the top, but merge into one shape lower down. The not-template build's icon is black in both appearances, so in the dark menu bar it all but disappears.

  Windows, at 100 % on the runner:
  - Explorer recorded the icon with no `IsPromoted` value, which Windows 11 shows in the overflow. After the script set `IsPromoted = 1`, UI Automation found the button named "Navaja" at 797, 720, 32 × 48 pixels, between the chevron and the other icons.
  - The 32-pixel PNG is drawn in about 11 × 12 pixels (the PNG has a transparent margin), on the taskbar's `#eeeeee`.
  - The pixel check (run 37152739410): inside the button, 24 × 40 pixels, the background is `#eeeeee`. 48 pixels are ink, 5.0 % (it needs 3 %), in an 11 × 10 box, and the strongest contrast is 192 (it needs 96). The empty stretch the script picked, at 8, 720, left of the centred taskbar buttons, has no ink and contrast 0; its crop shows plain taskbar.
  - The margin is not large. Run by hand on run 37151163157's capture, the same check gives the chevron's button 2.1 % ink, under the 3 %. The placeholder icon has about 1.7 times the minimum, mostly from its handle and pivot; its light blades are not ink.
  - The handle (`#57606a`) and the orange pivot are solid. Their edges change colour within one or two pixels, more along the handle's slanted top.
  - The blades are light grey in the source (`#e6e8eb` and `#d0d7de`). They come out within about 30 levels of the taskbar's grey, so they barely show. That is the placeholder art's colour, not scaling; the final art is roadmap M2b, item 3.

  **Windows scales, by a person.** On Windows 11 (not Server), with a display that offers 125, 150, 175 and 200 %:
  1. Build the app as shipped (`pnpm tauri build --debug --no-bundle`) and start `target\debug\navaja.exe`. If the icon is behind the chevron, turn Navaja on under Settings > Personalization > Taskbar > Other system tray icons.
  2. For each of 100, 125, 150, 175 and 200 % (Settings > System > Display > Scale): quit Navaja from its tray menu, set the scale, start it again, and capture the notification area (PrtScn). At one scale, also change the scale while Navaja runs, since Windows may then rescale the icon it already has.
  3. Open each capture in Paint, zoom to 800 % and look at the icon. Windows draws a small icon at 16 pixels at 100 %, 20 at 125 %, 24 at 150 %, 28 at 175 % and 32 at 200 %. Only 200 % uses the 32-pixel PNG one to one.
  4. "Crisp" means the edges of the handle and pivot change colour within one or two pixels, as they do at 100 % on the runner: not smeared over three or more, and not blocky with doubled pixels. Compare with the 200 % capture.
  5. Check it on the light and on the dark taskbar (Settings > Personalization > Colors).
  6. Record a table here: scale, the icon's size in pixels, crisp or not, and a note. Attach the crops to the PR.
  7. If any scale is not crisp, take the fallback: per-scale PNGs (16, 20, 24, 28 and 32 pixels), or `with_inner_tray_icon` with an .ico holding those five sizes. Either way, load the icon at the small-icon size for the display's DPI, `GetSystemMetricsForDpi(SM_CXSMICON, dpi)`. tray-icon's `Icon::from_path` and `from_resource` with no size load it with `LR_DEFAULTSIZE`, which picks the large-icon size (32 at 100 %), and Windows then scales that down again.

  Optionally, on a Retina Mac, start the app and look at the icon at 2×, in both appearances: the runner only covers 1×.
- **Decision:** none yet.
  - The tray shipped before this spike, against the rule that a gating spike runs first, so the spike could run on a working tray.
  - On macOS the template icon behaves as a template: the system tints it like its own items in both appearances, and a plain image does not pass the same check. No change is needed there.
  - On Windows, the per-scale decision waits for the steps above. Final art is roadmap M2b, item 3.
  - **The job's lifetime:** `spikes.yml`'s `s2-3` job runs once more on the final brand icons (roadmap M2b, item 3), and the result is recorded here. Then the job is deleted from `spikes.yml`.

## S2.6 WebdriverIO

- **Milestone / gates:** M2a, whether the end-to-end tests gate CI on all three OSes
- **Time box:** none set in the roadmap
- **Question:** does WebdriverIO drive the real app reliably and quickly enough on Windows, Linux and macOS for the end-to-end tests to be a required check?
- **Method:** the `os` job in `ci.yml` on GitHub-hosted windows-2025, ubuntu-24.04 and macos-26. A debug build with `--no-bundle --features e2e --config src-tauri/e2e.conf.json` (Tauri 2.12.1), driven by WebdriverIO 9.32.0 and `@wdio/tauri-service` 1.4.0 in embedded mode: the app's own `tauri-plugin-wdio-webdriver` 1.4.0 (git rev `fb4a544`, see `app/src-tauri/Cargo.toml`) serves WebDriver, with no external driver. Linux runs the suite through `app/e2e/strace-guard.sh`, inside `dbus-run-session -- xvfb-run`.
- **PASS if:** 10 of 10 runs per OS, under 10 min
- **FAIL then:** macOS end-to-end tests become non-gating, plus a manual smoke test
- **Result:** not finished (2026-10-03). The first run passed on all three OSes; the 10-of-10 criterion is not measured yet.
- **Numbers and evidence:** PR #7's first CI run, [37090426812](https://github.com/joaovitorpina/Navaja/actions/runs/37090426812), on commit `78f4ce0` (2026-10-03):

  | Runner | Result | e2e build | e2e suite | Whole `os` job |
  |---|---|---|---|---|
  | windows-2025 | pass | 1 min 53 s | 25 s | 7 min 41 s |
  | ubuntu-24.04 (strace guard) | pass | 53 s | 15 s | 6 min 0 s |
  | macos-26 | pass | 2 min 1 s | 18 s | 5 min 55 s |

  That run tested the suite as first submitted, before the PR #7 review fixes: the smoke spec's route reset, the stronger egress canary, the first-launch `--tool` spec, and the strace guard's signal handling and per-address check. It also still loaded WebdriverIO's front-end bridge (`VITE_NAVAJA_E2E=1`) and used the wider e2e overlay (`withGlobalTauri`, `core:default`), without the later harness changes (the window pin in `before()`, the msedgedriver patch, the env-gated WebDriver server). The Method above describes the setup from the next recorded run on. It is one run per OS, not ten.
- **Decision:** none yet. S2.6 stays open until 10 runs per OS are recorded here.

## M2a exit check: copy stays out of Windows clipboard history

Not a spike: one of M2a's exit criteria (roadmap M2a, Exit), checked the same way.

- **Question:** does a copy made through Navaja stay out of Windows clipboard history?
- **Method:** `clipboard.rs` copies through arboard with three formats Windows documents for this: `CanIncludeInClipboardHistory` and `CanUploadToCloudClipboard`, each a DWORD 0, and `ExcludeClipboardContentFromMonitorProcessing`. Job `exit-clipboard-history` in `spikes.yml`, on windows-2025, runs one step per check (`scripts/spikes/exit-clipboard-check.sh`, with `clipboard-history.ps1` in Windows PowerShell 5.1):
  - The end-to-end build (`--features e2e --config src-tauri/e2e.conf.json`).
  - Clipboard history on: `HKCU\Software\Microsoft\Clipboard\EnableClipboardHistory = 1`, then a restart of the per-user clipboard service (`cbdhsvc_*`). If `Clipboard.IsHistoryEnabled()` (WinRT) is still false, in this process and in a fresh one, the step also sets the `AllowClipboardHistory = 1` policy and restarts the service again. It sets the step output `enabled`, and fails only on an error.
  - Control, when history is on: a random text copied the ordinary way (`Set-Clipboard`) must appear in `Clipboard.GetHistoryItemsAsync()` within 10 s.
  - Navaja's copy: a spec written at run time, `app/e2e/spikes/exit-clipboard.e2e.ts`, opens the UUID tool, reads the UUID it generated and presses the output's Copy button, which goes through `copy_text`. That UUID is the canary. `Get-Clipboard` must then return it.
  - Formats: the clipboard, still holding Navaja's copy, must carry the three formats above, with the two DWORDs 0 (Win32 `EnumClipboardFormats` and `GetClipboardData`).
  - When history is on: the canary must still be on the clipboard, and 10 s later it must not be in the history. The clipboard is checked once, before that wait: history records an item when it is copied, so a second look at the clipboard would add nothing. Then the same canary, copied the ordinary way, must reach the history: the negative control for the lookup.
  - Formats, negative control: the same canary copied the ordinary way must fail the formats check, which must name `CanIncludeInClipboardHistory` as missing.
  - When Windows keeps history off, the summary marks the history row "off: inconclusive" and the three steps that need history "inconclusive", and the enable step leaves a warning on the run, so that the green job does not read as a pass. The spec is removed at the end.
  - **Untested:** the three steps that need history (the control, absent, and its negative control), and the code that reads the history's items (`GetHistoryItemsAsync()`, then each item's `GetTextAsync()` through .NET's `AsTask`), have never run with history on, on CI or anywhere else. They fail closed: the absence check needs the history's status to be `Success`, and its negative control must find the same text once it is copied the ordinary way. If a runner ever turns history on, those steps will gate the job on code that has not been run before.
- **PASS if:** an ordinary copy reaches clipboard history and Navaja's copy of a fresh value does not.
- **Result:** inconclusive on CI (2026-10-03). Windows Server keeps clipboard history off, so the runner cannot answer the question. What it does show: Navaja's copy reaches the clipboard with all three exclusion formats.
- **Evidence:** `spikes.yml` run [37150170804](https://github.com/joaovitorpina/Navaja/actions/runs/37150170804), on commit `78efc60`; the job passed in 4 min 6 s.
  - The runner is Windows Server 2025 Datacenter, build 26100, session 2, interactive, with Explorer running. Neither value existed before. With `EnableClipboardHistory = 1`, then the policy as well, and the service restarted after each, `IsHistoryEnabled()` stayed false for 15 s each time, in this process and in a fresh one. `GetHistoryItemsAsync()` returned `ClipboardHistoryDisabled`. A [Microsoft Q&A answer](https://learn.microsoft.com/en-us/answers/questions/91159/clipboard-history-on-windows-server) from 2020 says Windows Server did not have clipboard history then; Server 2025 on the runner behaves the same.
  - The Copy button copied the canary `9ca74a9d-315a-4698-8e21-233d18ed5f9a`, and `Get-Clipboard` returned it. The clipboard then held 7 formats: `CF_UNICODETEXT` (13), `CF_LOCALE` (16), `CF_TEXT` (1), `CF_OEMTEXT` (7), and `ExcludeClipboardContentFromMonitorProcessing`, `CanUploadToCloudClipboard` and `CanIncludeInClipboardHistory`, each `00-00-00-00`.
  - The same text through `Set-Clipboard` held 6 formats (`DataObject`, `Ole Private Data` and the four text ones), none of the three, and the formats check failed on it, naming all three.
  - Runs 37149634725 and 37150853141 gave the same results with their own canaries. Run 37148575144 found history off in the same way; its job failed, because the enable step then still failed when history stayed off.
  - Run [37152739410](https://github.com/joaovitorpina/Navaja/actions/runs/37152739410), on `913ef25`, gave the same results with the canary `74b7f23d-3e38-4ed4-8bb8-5a90f1c81689`, in 3 min 24 s. Its summary step got `enabled=false`, for which it writes the history row as "off: inconclusive", and the job carries the enable step's warning.
  - Run [37161147668](https://github.com/joaovitorpina/Navaja/actions/runs/37161147668), on `e4cb742`, gave the same results again with the canary `c20f0605-3573-4261-8d68-f1643319caee`, in 3 min 34 s. The three history steps were skipped, with `enabled=false`.
- **By a person**, on Windows 11:
  1. Turn on Settings > System > Clipboard > Clipboard history.
  2. Control: copy a word in Notepad, press Win+V, and check that it is listed.
  3. In Navaja, open the UUID generator and press Copy. Paste in Notepad: the UUID must appear. Press Win+V: the UUID must not be listed.
  4. Select the UUID in the output and press Ctrl+C, which also goes through `copy_text`. Win+V must not list it either.
  5. Record the Windows build and the result here.
- **The job's lifetime:** `exit-clipboard-history` moves into `ci.yml` only if a GitHub-hosted runner ever has clipboard history, so that its history checks can gate. Otherwise it is deleted from `spikes.yml` once the check by a person above is recorded.
