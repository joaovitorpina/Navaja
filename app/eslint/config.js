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
import { VIEW_EXTENSIONS } from './view-files.js';
import viewImports from './view-imports.js';

// view-files.js holds the list, so that `pnpm lint` can refuse any other file
// in a view folder without loading ESLint's plugins first.
export { VIEW_EXTENSIONS };

/** Every script file a custom view could hold, so none skips the allowlist. */
const VIEW_FILES = `tools/*/ui/**/*.{${VIEW_EXTENSIONS.join(',')}}`;

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

  // Custom views, last so no later entry can loosen it.
  {
    files: [VIEW_FILES],
    // A view must not switch the allowlist off with a comment.
    linterOptions: { noInlineConfig: true },
    plugins: { navaja: { meta: { name: 'navaja' }, rules: { 'view-imports': viewImports } } },
    rules: {
      'navaja/view-imports': 'error',
      // Applies <!-- eslint-disable --> comments in markup, which
      // noInlineConfig does not reach.
      'svelte/comment-directive': 'off',
    },
  },
);
