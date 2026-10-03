// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { parseIndex, RULE, viewFileProblems } from './view-files.js';

const file = (path: string) => ({ mode: '100644', path });

/** The paths `viewFileProblems` reports, in order. */
const refused = (...entries: { mode: string; path: string }[]) =>
  viewFileProblems(entries).map((problem) => problem.slice(0, problem.indexOf(': ')));

describe('the files under tools/', () => {
  it('may be any script ESLint lints in a view, or an allowed asset', () => {
    const scripts = ['js', 'mjs', 'cjs', 'jsx', 'ts', 'mts', 'cts', 'tsx', 'svelte'];
    const assets = ['css', 'svg', 'json', 'png', 'webp'];
    expect(
      refused(
        ...[...scripts, ...assets].map((extension) => file(`tools/demo/ui/x.${extension}`)),
        file('tools/demo/ui/View.svelte.ts'),
        file('tools/demo/ui/View.test.ts'),
        file('tools/demo/ui/i18n/en.ts'),
        file('tools/demo/ui/.hidden/x.ts'),
        { mode: '100755', path: 'tools/demo/ui/x.js' },
      ),
    ).toEqual([]);
  });

  it('may be anything outside a view folder', () => {
    expect(
      refused(
        file('tools/lib.rs'),
        file('tools/demo/mod.rs'),
        file('tools/demo/icon.svg'),
        file('tools/demo/README'),
        file('tools/demo/x.es6'),
        file('tools/demo/node_modules/x.es6'),
        file('tools/demo/sub/ui/x.es6'),
      ),
    ).toEqual([]);
  });

  it('are never links or submodules', () => {
    expect(
      refused(
        { mode: '120000', path: 'tools' },
        { mode: '120000', path: 'tools/demo/ui' },
        { mode: '120000', path: 'tools/demo/ui/label.ts' },
        { mode: '120000', path: 'tools/demo/mod.rs' },
        { mode: '160000', path: 'tools/demo/ui' },
      ),
    ).toEqual([
      'tools',
      'tools/demo/ui',
      'tools/demo/ui/label.ts',
      'tools/demo/mod.rs',
      'tools/demo/ui',
    ]);
  });

  it('in a view folder are never under node_modules, where ESLint does not look', () => {
    expect(
      refused(
        file('tools/demo/ui/node_modules/x.ts'),
        file('tools/demo/ui/sub/node_modules/pkg/index.js'),
        file('tools/demo/ui/node_modules_x/x.ts'),
      ),
    ).toEqual(['tools/demo/ui/node_modules/x.ts', 'tools/demo/ui/sub/node_modules/pkg/index.js']);
  });

  it('in a view folder have an extension ESLint lints or an allowed asset one, in lower case', () => {
    const names = [
      'label',
      '.eslintrc',
      'label.',
      'x.es6',
      'x.foo',
      'x.JS',
      'x.Ts',
      'x.SVG',
      'x.html',
    ];
    expect(refused(...names.map((name) => file(`tools/demo/ui/${name}`)))).toEqual(
      names.map((name) => `tools/demo/ui/${name}`),
    );
  });

  it('are each named in the problem, with the reason', () => {
    expect(
      viewFileProblems([
        { mode: '120000', path: 'tools/demo/ui' },
        { mode: '160000', path: 'tools/demo/ui/vendor' },
        file('tools/demo/ui/node_modules/x.ts'),
        file('tools/demo/ui/x.es6'),
      ]),
    ).toEqual([
      "tools/demo/ui: a symbolic link, which ESLint and Prettier don't follow and Vite does",
      "tools/demo/ui/vendor: a submodule, whose files this check can't see",
      'tools/demo/ui/node_modules/x.ts: in a node_modules folder, which ESLint skips',
      'tools/demo/ui/x.es6: not a script ESLint lints, nor an allowed asset',
    ]);
  });

  it('come with the rule they break', () => {
    expect(RULE).toBe(
      'A view folder (tools/<id>/ui/) holds only scripts ESLint lints ' +
        '(.js, .mjs, .cjs, .jsx, .ts, .mts, .cts, .tsx, .svelte) and assets ' +
        '(.css, .svg, .json, .png, .webp), with lower-case extensions and no node_modules ' +
        'folder, and nothing under tools/ is a symbolic link or a submodule. ' +
        'See docs/architecture.md §4.',
    );
  });
});

describe('parseIndex', () => {
  it('reads the mode and path of each `git ls-files -s -z` entry', () => {
    const blob = '0123456789abcdef0123456789abcdef01234567';
    expect(
      parseIndex(
        `100644 ${blob} 0\ttools/demo/ui/a b.ts\0` +
          `120000 ${blob} 0\ttools/demo/ui\0` +
          `100644 ${blob} 0\ttools/demo/ui/tab\there.ts\0`,
      ),
    ).toEqual([
      { mode: '100644', path: 'tools/demo/ui/a b.ts' },
      { mode: '120000', path: 'tools/demo/ui' },
      { mode: '100644', path: 'tools/demo/ui/tab\there.ts' },
    ]);
    expect(parseIndex('')).toEqual([]);
  });

  it('refuses anything else', () => {
    expect(() => parseIndex('tools/demo/ui/label.ts\0')).toThrow('Unexpected git ls-files entry');
  });
});
