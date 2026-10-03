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
- **Method:** not run yet. It runs on the minimal tray from roadmap M2a, item 9: Windows 11 at 100, 125, 150 and 200 %, and macOS 26 with a light and a dark menu bar.
- **PASS if:** crisp at 100-200 % on Windows; the macOS template icon works
- **FAIL then:** per-scale PNGs, or `with_inner_tray_icon` with an .ico
- **Result:** pending. Nothing is recorded as passed.
- **Numbers and evidence:** none yet.
- **Decision:** none yet. The tray shipped before this spike, against the rule that a gating spike runs first, so the spike could run on a working tray. Its icons are placeholders, picked by one `include_bytes!` constant in `tray.rs`: `icons/tray/template.png` on macOS and `icons/tray/color-32.png` elsewhere. Final art is roadmap M2b, item 3.

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
