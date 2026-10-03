# ADR 0002: Custom views outside `app/`

- **Status:** Accepted
- **Date:** 2026-10-03
- **Decider:** João Vitor Pina

## Context

Architecture §4 puts a tool's custom view next to its Rust code, in `tools/<id>/ui/`. `ToolHost` finds it with `import.meta.glob('@tools/*/ui/View.svelte')`, so adding a view never edits the shell.

Spike S2.2 asked whether such a view can be type-checked, linted, tested, built, hot-reloaded and packaged like code in `app/src`, on Windows, Linux and macOS. The roadmap sets the spike's fallbacks in order:
1. tsconfig and Vitest aliases;
2. otherwise `app/src/tools/<id>/`, which needs sign-off.

Two things stop the tools from working out of the box:
- **pnpm's isolated layout** installs packages under `app/node_modules` only, so a bare import from `tools/` (`vitest`, `@testing-library/svelte`) finds nothing.
- **ESLint 10 and Prettier 3** look up their config from each file's folder upwards, so files in `tools/` never find a config kept in `app/`.

Without the settings below, svelte-check missed a type error in the view, Vitest never ran its test, Tailwind dropped its classes, and ESLint skipped the file with only a warning (exit 0). [spikes.md](../spikes.md) S2.2 has the evidence.

## Decision

Keep custom views in `tools/<id>/ui/`, with the first fallback's settings:

- **TypeScript** (`app/tsconfig.json`): include `../tools/*/ui/**/*.ts` and `../tools/*/ui/**/*.svelte`, and map `vitest`, `@testing-library/svelte` and `@tauri-apps/api/*` to `./node_modules/` through `paths`.
- **Vitest** (`app/vite.config.ts`): add `../tools/*/ui/**/*.{test,spec}.ts` to `include`. Under Vitest only, `resolve.dedupe` resolves the same three packages from `app/`.
- **Tailwind** (`app/src/app.css`): `@source` names the view files (`../../tools/*/ui/**/*.{svelte,ts}`). A glob that ends in a folder finds no classes.
- **ESLint and Prettier:** their configs live in `app/` (`app/eslint/config.js`, `app/prettier.config.js`), where pnpm installs them. One-line re-exports at the repository root (`eslint.config.mjs`, `prettier.config.mjs`) make the root their base path. `pnpm lint` runs from the root with an explicit `--config`, so a config file planted under `tools/` is never used.

The `app/src/tools/<id>/` fallback is not taken.

## Evidence

The `spikes.yml` job `s2-2` builds a throwaway custom-view tool and checks it on windows-2025, ubuntu-24.04 and macos-26. Run [37146842128](https://github.com/joaovitorpina/Navaja/actions/runs/37146842128), on commit `dbf31d5` (2026-10-03), passed every check on all three:
- the registry accepts the tool;
- svelte-check passes, and catches a planted type error in a `.ts` file and in a `.svelte` file;
- ESLint and Prettier pass, and refuse a planted forbidden import and a misformatted file;
- Vitest runs the view's test;
- the build gives the view its own chunk and keeps its Tailwind class;
- the packaged app shows the view and an answer from Rust;
- HMR updates the view, both at the dev server and in the app's own webview, without a reload.

## Consequences

- A view's test that needs a package beyond those three must be added in three places: the tsconfig `paths`, Vitest's `dedupe`, and the test packages in `app/eslint/view-imports.js`. That is a `host-change` PR.
- The root holds two small config files that only re-export `app/`'s.
- Editors that use config lookup find the root configs, so they lint views like `pnpm lint` does. They do not ignore a config planted under `tools/` the way `--config` does; CI's `pnpm lint` does.
- The `spikes.yml` job can be re-run (`workflow_dispatch`) after a Vite, Vitest, svelte-check, ESLint, Prettier or Tailwind upgrade, to re-check these settings on all three OSes.
