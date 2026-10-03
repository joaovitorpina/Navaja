// ESLint for the app and for custom views. The root eslint.config.mjs loads
// this file, so patterns here are relative to the repository root: custom views
// live in tools/<id>/ui/, outside app/, and ESLint skips files outside the
// folder of the config it uses (docs/spikes.md S2.2).
import js from '@eslint/js';
import prettier from 'eslint-config-prettier/flat';
import svelte from 'eslint-plugin-svelte';
import { defineConfig, globalIgnores } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';

export default defineConfig(
  globalIgnores([
    // ts-rs output; `cargo xtask bindings --check` owns it.
    'app/src/bindings/',
    'app/dist/',
    'app/e2e/output/',
    'app/src-tauri/',
  ]),
  { linterOptions: { reportUnusedDisableDirectives: 'error' } },

  js.configs.recommended,
  ts.configs.recommended,
  svelte.configs.recommended,
  prettier,
  svelte.configs.prettier,

  {
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: { parserOptions: { parser: ts.parser, extraFileExtensions: ['.svelte'] } },
  },
  {
    files: ['app/src/**', 'tools/*/ui/**'],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ['app/e2e/**'],
    languageOptions: { globals: { ...globals.node, ...globals.mocha } },
  },
  {
    files: ['app/*.{js,ts}', 'app/eslint/**'],
    languageOptions: { globals: globals.node },
  },
);
