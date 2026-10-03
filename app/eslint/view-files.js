// What may sit under tools/ (docs/architecture.md §4). The import allowlist
// sees only the files ESLint lints, but Vite bundles any file a view imports.
// So a view folder holds only files ESLint lints and a few assets. Nothing
// under tools/ is a link, which ESLint and Prettier don't follow and Vite does,
// a submodule, whose files git does not list here, or a file Vite reads to
// resolve imports. `pnpm lint` runs this before ESLint, over the files git
// tracks: what a PR can add.
import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * The script files ESLint lints in a view, by extension; config.js builds its
 * pattern from this list. Lower case only: ESLint matches the case, while Vite
 * reads a `.JS` file as JavaScript all the same.
 */
export const VIEW_EXTENSIONS = ['js', 'mjs', 'cjs', 'jsx', 'ts', 'mts', 'cts', 'tsx', 'svelte'];

/**
 * The other files a view may hold. Vite loads them as assets, not as scripts,
 * so none of them can import a script:
 * - css: the view's own styles (nothing checks what its imports and url()
 *   reach, §4);
 * - svg: icons, inline with `?raw` or as a URL;
 * - json: static data;
 * - png and webp: images.
 */
export const VIEW_ASSETS = ['css', 'svg', 'json', 'png', 'webp'];

/**
 * Files Vite reads to resolve a view's imports, taking the nearest one above
 * each file, so one anywhere under tools/ can send an allowed import
 * elsewhere: package.json (its browser field maps one file to another) and
 * tsconfig.json (its jsxImportSource makes each .jsx and .tsx file import
 * `<source>/jsx-runtime`). Compared in lower case, as a build on Windows or
 * macOS finds them.
 */
export const RESOLVER_FILES = ['package.json', 'tsconfig.json'];

/** What the problems break, printed after them. */
export const RULE =
  `A view folder (tools/<id>/ui/) holds only scripts ESLint lints (.${VIEW_EXTENSIONS.join(', .')}) ` +
  `and assets (.${VIEW_ASSETS.join(', .')}), with lower-case extensions and no node_modules ` +
  'folder, and nothing under tools/ is a symbolic link, a submodule, ' +
  `${RESOLVER_FILES.map((name) => `a ${name}`).join(' or ')}. See docs/architecture.md §4.`;

/**
 * Mode and path of each entry `git ls-files -s -z` prints.
 * @param {string} output
 * @returns {{ mode: string, path: string }[]}
 */
export function parseIndex(output) {
  return output
    .split('\0')
    .filter((entry) => entry !== '')
    .map((entry) => {
      const match = /^(\d{6}) [0-9a-f]+ \d\t(.+)$/s.exec(entry);
      if (!match?.[1] || !match[2]) throw new Error(`Unexpected git ls-files entry: ${entry}`);
      return { mode: match[1], path: match[2] };
    });
}

/**
 * One line for each entry that may not sit under tools/; none when all may.
 * @param {{ mode: string, path: string }[]} entries what git tracks under tools/
 * @returns {string[]}
 */
export function viewFileProblems(entries) {
  /** @type {string[]} */
  const problems = [];
  for (const { mode, path: file } of entries) {
    if (mode === '120000') {
      problems.push(
        `${file}: a symbolic link, which ESLint and Prettier don't follow and Vite does`,
      );
      continue;
    }
    if (mode === '160000') {
      problems.push(`${file}: a submodule, whose files this check can't see`);
      continue;
    }
    if (RESOLVER_FILES.includes(path.posix.basename(file).toLowerCase())) {
      problems.push(`${file}: Vite reads it to resolve imports, which it can send anywhere`);
      continue;
    }
    const inView = /^tools\/[^/]+\/ui\/(.+)$/s.exec(file)?.[1];
    if (inView === undefined) continue;
    if (inView.split('/').includes('node_modules')) {
      problems.push(`${file}: in a node_modules folder, which ESLint skips`);
      continue;
    }
    const extension = path.posix.extname(file).slice(1);
    if (!VIEW_EXTENSIONS.includes(extension) && !VIEW_ASSETS.includes(extension)) {
      problems.push(`${file}: not a script ESLint lints, nor an allowed asset`);
    }
  }
  return problems;
}

/** Checks what git tracks under tools/ and exits non-zero on any problem. */
function main() {
  const root = fileURLToPath(new URL('../../', import.meta.url));
  const output = execFileSync('git', ['ls-files', '-s', '-z', '--', 'tools'], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  const problems = viewFileProblems(parseIndex(output));
  if (problems.length > 0) {
    console.error(`Files that may not sit under tools/:\n${problems.join('\n')}\n${RULE}`);
    process.exitCode = 1;
  } else {
    console.log('Every file under tools/ may stay there.');
  }
}

// Node runs the real path of the script it is given, so this holds when the
// file is run (`pnpm lint`), not when a test imports it.
if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) main();
