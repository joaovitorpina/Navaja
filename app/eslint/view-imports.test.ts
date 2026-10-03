// @vitest-environment node
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { RuleTester } from 'eslint';
import * as svelteParser from 'svelte-eslint-parser';
import ts from 'typescript-eslint';
import { describe, expect, it } from 'vitest';
import rule, { SVELTE_ENTRIES } from './view-imports.js';

RuleTester.describe = describe;
RuleTester.it = it;
RuleTester.itOnly = it.only;

// Real paths for this checkout, so Windows runs see back slashes.
const root = fileURLToPath(new URL('../../', import.meta.url));
const ui = (file: string) => join(root, 'tools', 'demo', 'ui', file);
const VIEW = ui('View.svelte');
const TS = ui('label.ts');
const NESTED = ui(join('parts', 'Row.svelte'));
const NESTED_TS = ui(join('parts', 'row.ts'));
const TEST = ui('View.test.ts');
const SPEC = ui('View.spec.ts');

// Windows names, written out: a drive path, a UNC path (\\host\share\…) and a
// \\?\ path, each with a file in a subfolder.
const WIN = 'C:\\repo\\tools\\demo\\ui\\label.ts';
const WIN_NESTED = 'C:\\repo\\tools\\demo\\ui\\parts\\row.ts';
const UNC = '\\\\host\\share\\repo\\tools\\demo\\ui\\label.ts';
const UNC_NESTED = '\\\\host\\share\\repo\\tools\\demo\\ui\\parts\\row.ts';
const LONG = '\\\\?\\C:\\repo\\tools\\demo\\ui\\label.ts';
const LONG_NESTED = '\\\\?\\C:\\repo\\tools\\demo\\ui\\parts\\row.ts';

const svelte = {
  parser: svelteParser,
  parserOptions: { parser: ts.parser },
};

const tester = new RuleTester({ languageOptions: { parser: ts.parser } });

/** A .svelte case, with `script` in an instance script. */
const view = (script: string, file = VIEW) => ({
  code: `<script lang="ts">\n${script}\n</script>\n`,
  filename: file,
  languageOptions: svelte,
});

const notAllowed = (code: string, filename = TS) => ({
  code,
  filename,
  errors: [{ messageId: 'notAllowed' }],
});

/** A case in a view's test that reports `messageIds`, in order. */
const inTest = (code: string, ...messageIds: string[]) => ({
  code,
  filename: TEST,
  errors: messageIds.map((messageId) => ({ messageId })),
});

tester.run('view-imports', rule, {
  valid: [
    // Relative paths that stay inside tools/demo/ui/.
    { code: "import { label } from './label';", filename: TS },
    { code: "import Row from './parts/Row.svelte';", filename: TS },
    { code: "import { label } from './parts/../label';", filename: TS },
    { code: "import raw from './icon.svg?raw';", filename: TS },
    { code: "import { label } from './label#x';", filename: TS },
    { code: "const w = new URL('./worker.ts?worker#x', import.meta.url);", filename: TS },
    { code: "import index from '.';", filename: TS },
    view("  import { label } from '../label';", NESTED),
    { code: "import { label } from '../label';", filename: NESTED_TS },
    { code: "import { label } from '../ui/label';", filename: TS },
    // Windows names, so that every OS runs the back-slash handling.
    { code: "import { label } from './label';", filename: WIN },
    { code: "import { label } from '../label';", filename: WIN_NESTED },
    { code: "import { label } from './label';", filename: UNC },
    { code: "import { label } from '../label';", filename: UNC_NESTED },
    { code: "import { label } from './label';", filename: LONG },
    { code: "import { label } from '../label';", filename: LONG_NESTED },
    // svelte and its browser subpaths.
    { code: "import { onMount } from 'svelte';", filename: TS },
    { code: "import type { HTMLAttributes } from 'svelte/elements';", filename: TS },
    { code: "import { writable } from 'svelte/store';", filename: TS },
    { code: "import { fade } from 'svelte/transition';", filename: TS },
    { code: "import { innerWidth } from 'svelte/reactivity/window';", filename: TS },
    // The shell API and generated bindings.
    { code: "import { t, type ViewProps } from '$lib/view-kit';", filename: TS },
    { code: "import type { ToolMeta } from '$bindings/ToolMeta';", filename: TS },
    { code: "import type { JsonValue } from '$bindings/serde_json/JsonValue';", filename: TS },
    // Every import form, with allowed specifiers.
    { code: "import './view.css';", filename: TS },
    { code: "export { label } from './label';", filename: TS },
    { code: "export type { ToolMeta } from '$bindings/ToolMeta';", filename: TS },
    { code: "export * from './label';", filename: TS },
    { code: "const lazy = import('./lazy');", filename: TS },
    { code: 'const lazy = import(`./lazy`);', filename: TS },
    { code: "const x = require('./x');", filename: ui('x.js') },
    { code: "import x = require('./x');", filename: TS },
    { code: "type T = typeof import('./label');", filename: TS },
    { code: "const w = new URL('./worker.ts', import.meta.url);", filename: TS },
    // Not imports.
    { code: "const page = new URL('https://example.com/');", filename: TS },
    { code: '// A view has no JSX.\nexport const el = 1;\n', filename: ui('x.tsx') },
    { code: 'const dev = import.meta.env.DEV;', filename: TS },
    { code: 'export const label = "x";', filename: TS },
    // Both script blocks and the markup of a .svelte file.
    {
      code:
        '<script module lang="ts">\n  export { label } from \'./label\';\n</script>\n' +
        '<script lang="ts">\n  import { t } from \'$lib/view-kit\';\n</script>\n' +
        "{#await import('./lazy') then m}{t('a', 'b')}{m}{/await}\n",
      filename: VIEW,
      languageOptions: svelte,
    },
    // A view's tests may also import the three test packages.
    { code: "import { expect, it, vi } from 'vitest';", filename: TEST },
    { code: "import { render } from '@testing-library/svelte';", filename: TEST },
    { code: "import { mockIPC } from '@tauri-apps/api/mocks';", filename: SPEC },
    { code: "import View from './View.svelte';", filename: TEST },
    // Vitest's calls that take a specifier, with allowed ones.
    { code: "vi.mock('./label', () => ({ label: 'x' }));", filename: TEST },
    { code: "vitest.doMock('$lib/view-kit');", filename: TEST },
    { code: "const actual = await vi.importActual('./label');", filename: SPEC },
    { code: "vi.mock(import('./label'));", filename: TEST },
    { code: "vi?.mock('./label');", filename: TEST },
    { code: "vi.spyOn(console, 'log');", filename: TEST },
    // vi and vitest under their own names, read by member, and mock functions.
    {
      code:
        "import { expect, vi, vitest } from 'vitest';\n" +
        "vi.fn();\nvitest.spyOn(console, 'log');\nvi['useFakeTimers']();\n",
      filename: TEST,
    },
    {
      code:
        "import { expect, vi } from 'vitest';\nconst fn = vi.fn();\n" +
        'expect(fn.mock.calls).toEqual([]);\nexpect(vi.mocked(fn).mock.results).toEqual([]);\n',
      filename: TEST,
    },
    {
      code:
        "import { vi } from 'vitest';\nlet spy: ReturnType<typeof vi.fn>;\n" +
        "type Vi = typeof vi;\nimport type * as Vitest from 'vitest';\n",
      filename: SPEC,
    },
    // Outside tests, these names mean nothing.
    { code: 'const vi = { mock: 1 };\nexport const { mock } = vi;\n', filename: TS },
  ],
  invalid: [
    // Relative paths that leave tools/demo/ui/.
    notAllowed("import x from '../x';"),
    notAllowed("import x from './../x';"),
    notAllowed("import x from './sub/../../x';"),
    notAllowed("import x from '..';"),
    notAllowed("import x from '../../other/ui/View.svelte';"),
    notAllowed("import x from '../uix/y';"),
    notAllowed("import x from '../../x';", NESTED_TS),
    notAllowed("import x from '../../../app/src/lib/ipc';"),
    notAllowed("import x from '../x';", WIN),
    notAllowed("import x from '../../x';", WIN_NESTED),
    notAllowed("import x from '../x';", UNC),
    notAllowed("import x from '../../x';", UNC_NESTED),
    notAllowed("import x from '../x';", LONG),
    notAllowed("import x from '../../x';", LONG_NESTED),
    // Forms some resolvers read differently: back slashes, percent escapes.
    notAllowed("import x from './sub\\\\..\\\\..\\\\x';"),
    notAllowed("import x from './%2e%2e/x';"),
    // A ?query or #hash, which Vite strips: each leaves the folder only one way.
    notAllowed("import x from '../outside.js?/../ui/outside.js';"),
    notAllowed("import x from './b#/../../outside.js';"),
    notAllowed("const w = new URL('../outside.js?/../ui/outside.js', import.meta.url);"),
    notAllowed("const w = new URL('./b#/../../outside.js', import.meta.url);"),
    // Bare names and aliases off the list.
    notAllowed("import x from 'sub/x';"),
    notAllowed("import { invoke } from '@tauri-apps/api/core';"),
    notAllowed("import { mockIPC } from '@tauri-apps/api/mocks';"),
    notAllowed("import { it } from 'vitest';"),
    notAllowed("import { render } from '@testing-library/svelte';"),
    notAllowed("import View from '@tools/other/ui/View.svelte';"),
    // svelte/internal and anything under it, and Svelte's non-browser entries.
    notAllowed("import * as internal from 'svelte/internal';"),
    notAllowed("import * as client from 'svelte/internal/client';"),
    notAllowed("import 'svelte/internal/flags/legacy';"),
    notAllowed("import { compile } from 'svelte/compiler';"),
    notAllowed("import { render } from 'svelte/server';"),
    notAllowed("import pkg from 'svelte/package.json';"),
    notAllowed("import x from 'svelte/store/x';"),
    notAllowed("import x from 'svelte/../x';"),
    notAllowed("import x from 'svelte-x';"),
    // Only $lib/view-kit itself.
    notAllowed("import * as lib from '$lib';"),
    notAllowed("import kit from '$lib/view-kit/index';"),
    notAllowed("import { runTool } from '$lib/view-kit/../ipc';"),
    // $bindings/<name> without . or .. segments.
    notAllowed("import x from '$bindings/../ipc';"),
    notAllowed("import x from '$bindings/./ToolMeta';"),
    notAllowed("import x from '$bindings/serde_json/../../lib/ipc';"),
    notAllowed("import x from '$bindings';"),
    notAllowed("import x from '$bindings//ToolMeta';"),
    // Every import form.
    notAllowed("import type { Run } from '$lib/ipc';"),
    notAllowed("import '../view.css';"),
    notAllowed("export { runTool } from '$lib/ipc';"),
    notAllowed("export type { Run } from '$lib/ipc';"),
    notAllowed("export * from '../x';"),
    notAllowed("export * as ipc from '../x';"),
    notAllowed("const lazy = import('../x');"),
    notAllowed("const x = require('../x');", ui('x.js')),
    notAllowed("import x = require('../x');"),
    notAllowed("type T = typeof import('../x');"),
    notAllowed("const w = new URL('../worker.ts', import.meta.url);"),
    // Vite resolves any other literal here through its aliases and resolver.
    notAllowed(
      "const w = new Worker(new URL('$lib/ipc.ts', import.meta.url), { type: 'module' });",
    ),
    notAllowed("const w = new URL('sub/../../x', import.meta.url);"),
    notAllowed("const w = new URL('@tools/other/ui/View.svelte', import.meta.url);"),
    notAllowed("const w = new URL('/src/lib/ipc.ts', import.meta.url);"),
    notAllowed("const w = new URL('data:text/javascript,0', import.meta.url);"),
    {
      code: 'const name = "x"; const lazy = import(name);',
      filename: TS,
      errors: [{ messageId: 'nonLiteral' }],
    },
    {
      code: 'const name = "x"; const lazy = import(`./${name}.ts`);',
      filename: TS,
      errors: [{ messageId: 'nonLiteral' }],
    },
    {
      code: 'const name = "x"; const x = require(name);',
      filename: ui('x.js'),
      errors: [{ messageId: 'nonLiteral' }],
    },
    {
      code: 'const x = require();',
      filename: ui('x.js'),
      errors: [{ messageId: 'nonLiteral' }],
    },
    {
      code: 'const name = "x"; const w = new URL(name, import.meta.url);',
      filename: TS,
      errors: [{ messageId: 'nonLiteral' }],
    },
    // JSX makes the build import react/jsx-runtime, and a JSX import source
    // <source>/jsx-runtime.
    {
      code: 'export const el = <div />;\n',
      filename: ui('x.tsx'),
      errors: [{ messageId: 'jsx' }],
    },
    {
      code: 'export const el = <>\n  <b>{true && <i />}</b>\n</>;\nexport const f = <p />;\n',
      filename: ui('x.jsx'),
      errors: [
        { messageId: 'jsx', line: 1 },
        { messageId: 'jsx', line: 4 },
      ],
    },
    {
      code: '/** @jsxImportSource .. */\nexport const el = <div />;\n',
      filename: ui('x.tsx'),
      errors: [{ messageId: 'jsxImportSource' }, { messageId: 'jsx' }],
    },
    {
      code: '// @jsxImportSource preact\nexport const el = <div />;\n',
      filename: ui('x.jsx'),
      errors: [{ messageId: 'jsxImportSource' }, { messageId: 'jsx' }],
    },
    {
      code: '/* @jsxRuntime automatic @jsxImportSource ./ui */\nexport const el = 1;\n',
      filename: TS,
      errors: [{ messageId: 'jsxImportSource' }],
    },
    // Views may not glob.
    {
      code: "const views = import.meta.glob('./*.svelte');",
      filename: TS,
      errors: [{ messageId: 'glob' }],
    },
    {
      code: "const views = import.meta['glob']('./*.svelte');",
      filename: TS,
      errors: [{ messageId: 'glob' }],
    },
    // Both script blocks and the markup of a .svelte file.
    {
      code: '<script module lang="ts">\n  export { runTool } from \'$lib/ipc\';\n</script>\n',
      filename: VIEW,
      languageOptions: svelte,
      errors: [{ messageId: 'notAllowed' }],
    },
    { ...view("  import { t } from '$lib/i18n';"), errors: [{ messageId: 'notAllowed' }] },
    {
      code: '<script lang="ts">\n</script>\n{#await import(\'../x\') then m}{m}{/await}\n',
      filename: VIEW,
      languageOptions: svelte,
      errors: [{ messageId: 'notAllowed' }],
    },
    {
      ...view("  const views = import.meta.glob('../*/ui/View.svelte');"),
      errors: [{ messageId: 'glob' }],
    },
    // Tests get the three test packages, nothing more.
    notAllowed("import { invoke } from '@tauri-apps/api/core';", TEST),
    notAllowed("import * as api from '@tauri-apps/api';", SPEC),
    notAllowed("import matchers from '@testing-library/svelte/vitest';", TEST),
    notAllowed("import { it } from 'vitest';", ui('View.test.js')),
    notAllowed("import { it } from 'vitest';", ui('helpers.ts')),
    // Vitest resolves a mocked or imported specifier through the same aliases.
    ...['vi', 'vitest'].flatMap((object) =>
      ['mock', 'doMock', 'unmock', 'doUnmock', 'importActual', 'importMock'].map((method) =>
        notAllowed(`${object}.${method}('$lib/ipc');`, TEST),
      ),
    ),
    notAllowed("vi.mock('../x', () => ({}));", SPEC),
    notAllowed("vi['mock']('$lib/ipc');", TEST),
    notAllowed("vi.mock(import('$lib/ipc'));", TEST),
    {
      code: 'const name = "$lib/ipc"; vi.mock(name);',
      filename: TEST,
      errors: [{ messageId: 'nonLiteral' }],
    },
    {
      code: 'vi.importActual();',
      filename: TEST,
      errors: [{ messageId: 'nonLiteral' }],
    },
    // Any object's call counts: vi.resetModules() and the like return vi.
    notAllowed("vi.resetModules().importActual('$lib/ipc');", TEST),
    // vi under another name, where those calls would go unseen.
    inTest("import { vi as v } from 'vitest';", 'vitestImport'),
    inTest("import { vitest as v } from 'vitest';", 'vitestImport'),
    inTest("import { vi as vitest } from 'vitest';", 'vitestImport'),
    inTest("import * as vt from 'vitest';", 'vitestImport'),
    inTest("export { vi } from 'vitest';", 'vitestImport'),
    inTest("export * from 'vitest';", 'vitestImport'),
    inTest("const vt = await import('vitest');", 'vitestImport'),
    inTest("import vt = require('vitest');", 'vitestImport'),
    inTest("const vt = await vi.importActual('vitest');", 'vitestImport'),
    inTest(
      "import { expect, it, vi as v } from 'vitest';\nawait v.importActual('$lib/ipc');",
      'vitestImport',
      'notAllowed',
    ),
    inTest(
      "import * as vt from 'vitest';\nawait vt.vi.importActual('$lib/ipc');",
      'vitestImport',
      'notAllowed',
    ),
    inTest("import { vi } from 'vitest';\nconst v = vi;", 'vitestUse'),
    inTest("import { vi } from 'vitest';\nexport { vi };", 'vitestUse'),
    inTest('wrap(vi);', 'vitestUse'),
    inTest("const name = 'importActual';\nvi[name]('$lib/ipc');", 'vitestUse'),
    // Those calls taken apart.
    inTest(
      "import { vi } from 'vitest';\nconst { importActual } = vi;\n" +
        "await importActual('$lib/router.svelte');",
      'vitestUse',
      'vitestUse',
    ),
    inTest('const { mock: m } = vi.resetModules();', 'vitestUse'),
    inTest('const f = vi.importActual;', 'vitestUse'),
    inTest("(0, vi.mock)('$lib/ipc');", 'vitestUse'),
    inTest("vi.mock.apply(vi, ['$lib/ipc']);", 'vitestUse', 'vitestUse'),
    inTest("vi.resetModules().doMock.call(null, '$lib/ipc');", 'vitestUse'),
    // The message names the allowed set and the contract.
    {
      code: "import { t } from '$lib/i18n';",
      filename: TS,
      errors: [
        {
          message:
            "'$lib/i18n' is not allowed here. A custom view imports only relative paths inside " +
            'its own tools/<id>/ui/ folder, svelte and its browser subpaths (not ' +
            'svelte/internal, svelte/compiler or svelte/server), $lib/view-kit and ' +
            '$bindings/<name>. See docs/architecture.md §4.',
        },
      ],
    },
    {
      code: "import { JSDOM } from 'jsdom';",
      filename: TEST,
      errors: [
        {
          message:
            "'jsdom' is not allowed here. A custom view imports only relative paths inside its " +
            'own tools/<id>/ui/ folder, svelte and its browser subpaths (not svelte/internal, ' +
            'svelte/compiler or svelte/server), $lib/view-kit and ' +
            '$bindings/<name>; its *.test.ts and *.spec.ts files also vitest, ' +
            '@testing-library/svelte and @tauri-apps/api/mocks. See docs/architecture.md §4.',
        },
      ],
    },
  ],
});

describe('the svelte entry points', () => {
  // A Svelte update that adds an entry fails here, so someone decides
  // whether views may use it.
  it('are every export of the installed Svelte but the private and non-browser ones', () => {
    const pkg = createRequire(import.meta.url)('svelte/package.json') as {
      exports: Record<string, unknown>;
    };
    const refused = /^\.\/(package\.json|compiler|server|internal)(\/|$)/;
    const browser = Object.keys(pkg.exports)
      .filter((key) => !refused.test(key))
      .map((key) => `svelte${key.slice(1)}`);
    expect([...SVELTE_ENTRIES].sort()).toEqual(browser.sort());
  });
});
