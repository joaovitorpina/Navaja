// Prettier for the code ESLint lints (the "lint" and "format" scripts). It keeps
// the style the code was written in.
import { fileURLToPath } from 'node:url';

/** @type {import('prettier').Config} */
export default {
  singleQuote: true,
  printWidth: 100,
  // Resolved from here, where pnpm installs it: Prettier would otherwise look
  // from the working directory, and files in tools/ load this config through
  // the root prettier.config.mjs.
  plugins: [fileURLToPath(import.meta.resolve('prettier-plugin-svelte'))],
};
