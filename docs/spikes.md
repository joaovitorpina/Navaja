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
- **PASS if:** HMR, build, packaged app, svelte-check, ESLint and Vitest all work on 3 OSes
- **FAIL then:** tsconfig and Vitest aliases; otherwise `app/src/tools/<id>/` (needs sign-off)
- **Result:** not finished (2026-10-03). svelte-check, Vitest and `vite build` work on Windows, but only with the tsconfig, Vite and Tailwind settings below. HMR, the packaged app, ESLint, macOS and Linux are not verified yet.
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
  | HMR, packaged app (`tauri build`), ESLint, macOS, Linux | not run | not run |

  The "without" column for the type error, the missing import and the failing test comes from the PR #4 review, which ran the same probe on the PR head. The other rows were run here.

  The resolution failures come from pnpm's isolated layout: it installs packages under `app/node_modules` only, so bare imports from `tools/` find nothing. `svelte` and `$bindings/*` were not affected (vite-plugin-svelte dedupes `svelte`, and `$bindings` is an alias).
- **Decision:** keep custom views in `tools/<id>/ui/`; the `app/src/tools/<id>/` fallback is not needed so far. The settings that make it work, all in `app/`:
  - `tsconfig.json` includes `../tools/*/ui/**/*.ts` and `../tools/*/ui/**/*.svelte`, and maps `vitest`, `@testing-library/svelte` and `@tauri-apps/api/*` to `./node_modules/` through `paths`.
  - `vite.config.ts` adds `../tools/*/ui/**/*.test.ts` to Vitest's `include` and, under Vitest only, sets `resolve.dedupe` to those three packages so they resolve from `app/`. A view test that needs another package must be added to both lists.
  - `app.css` names the view files in its `@source` glob; a glob ending in a directory finds no classes.
  - `$lib/view-kit` exists and is what views import from the shell. ToolHost types the glob against its `ViewProps`.
  - Still open before S2.2 can pass: HMR and the packaged app on Windows, the same checks on macOS and Linux in CI, and the ESLint import allowlist (tracked in roadmap M2a, item 7). Once it passes, this entry needs an ADR for the tsconfig and Vite settings, since the roadmap counts them as the first fallback.

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
