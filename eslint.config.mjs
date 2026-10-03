// The ESLint config lives in app/, where pnpm installs ESLint and its plugins;
// this file loads it. Its patterns are relative to the repository root, which
// is ESLint's base path both ways it runs, so they cover custom views in
// tools/<id>/ui/ as well as app/ (docs/spikes.md S2.2):
// - with config lookup (editors), the base path is the folder of the
//   eslint.config.* found above each file: this one;
// - with --config (`pnpm lint`), it is the working directory, so the "lint"
//   script runs from the root.
export { default } from './app/eslint/config.js';
