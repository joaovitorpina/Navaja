# Spikes

A spike is a short, time-boxed experiment. It settles a question before the code that depends on it is written. [roadmap.md](roadmap.md) lists every planned spike.

## Rules

- Write PASS and FAIL before running anything.
- Keep to the time box. When it runs out, record what you have and decide.
- A gating spike runs before the work it gates.
- A fallback you take becomes an ADR in `docs/adr/`.
- Keep raw outputs (captures, logs, numbers) next to the entry or in `crates/*/tests/fixtures/`. Redact arguments and anything secret.

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
- **PASS if:** HMR, build, packaged app, svelte-check, ESLint and Vitest all work on 3 OSes
- **FAIL then:** tsconfig and Vitest aliases; otherwise `app/src/tools/<id>/` (needs sign-off)
- **Result:** not finished (2026-10-03). svelte-check, Vitest, `vite build`, ESLint and Prettier work on Windows, but only with the tsconfig, Vite, Tailwind, ESLint and Prettier settings below. HMR, the packaged app, macOS and Linux are not verified yet.
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
  | HMR, packaged app (`tauri build`), macOS, Linux | not run | not run |

  The "without" column for the type error, the missing import and the failing test comes from the PR #4 review, which ran the same probe on the PR head. For the ESLint rows, "without" means the config sits in `app/` alone, or a tool folder holds its own. The other rows were run here.

  The resolution failures come from pnpm's isolated layout: it installs packages under `app/node_modules` only, so bare imports from `tools/` find nothing. `svelte` and `$bindings/*` were not affected (vite-plugin-svelte dedupes `svelte`, and `$bindings` is an alias).

  ESLint 10 looks for a config from each linted file's folder upwards, so files in `tools/` never find one in `app/`. With `--config`, the base path is the working directory, and it skips files outside it with only a warning. Prettier also looks its config up from each file, and it loads plugins named as strings from the working directory.
- **Decision:** keep custom views in `tools/<id>/ui/`; the `app/src/tools/<id>/` fallback is not needed so far. The settings that make it work, all in `app/` apart from the two root config files that load ESLint's and Prettier's:
  - `tsconfig.json` includes `../tools/*/ui/**/*.ts` and `../tools/*/ui/**/*.svelte`, and maps `vitest`, `@testing-library/svelte` and `@tauri-apps/api/*` to `./node_modules/` through `paths`.
  - `vite.config.ts` adds `../tools/*/ui/**/*.test.ts` to Vitest's `include` and, under Vitest only, sets `resolve.dedupe` to those three packages so they resolve from `app/`. A view test that needs another package must be added to both lists.
  - `app.css` names the view files in its `@source` glob; a glob ending in a directory finds no classes.
  - `$lib/view-kit` exists and is what views import from the shell. ToolHost types the glob against its `ViewProps`.
  - ESLint's config is `app/eslint/config.js`, loaded by the root `eslint.config.mjs`, so its base path is the repository root and its patterns cover `app/` and `tools/*/ui/`. `pnpm lint` runs from the root with `--config eslint.config.mjs`, so a config file placed under `tools/` is never used. The import allowlist (architecture §4) is in place, and `app/eslint/config.test.ts` runs the real config through ESLint's Node API on a file under `tools/`, so a change that drops `tools/` fails a test.
  - Prettier's config is `app/prettier.config.js`, loaded by the root `prettier.config.mjs`. It resolves the Svelte plugin from `app/`.
  - Still open before S2.2 can pass: HMR and the packaged app on Windows, and the same checks on macOS and Linux in CI (CI runs `pnpm lint` on Linux only, in the checks job). Once it passes, this entry needs an ADR for the tsconfig and Vite settings, since the roadmap counts them as the first fallback.

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
