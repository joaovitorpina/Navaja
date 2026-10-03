// The ESLint config lives in app/, where pnpm installs ESLint and its plugins.
// It is loaded from here so that its base path is the repository root and it
// covers custom views in tools/<id>/ui/ as well as app/ (docs/spikes.md S2.2).
export { default } from './app/eslint/config.js';
