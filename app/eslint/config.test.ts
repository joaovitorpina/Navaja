// @vitest-environment node
// The real config, run the way `pnpm lint` runs it. Custom views sit outside
// app/, where a config change can make ESLint skip them without a word
// ("File ignored because outside of base path"), so this checks that a view's
// violations still come out.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint } from 'eslint';
import { beforeAll, describe, expect, it } from 'vitest';
import { VIEW_EXTENSIONS } from './config.js';

const root = fileURLToPath(new URL('../../', import.meta.url));
const eslint = new ESLint({ cwd: root, overrideConfigFile: 'eslint.config.mjs' });

/** Lints `code` as if it were the file at `path` (relative to the repository root). */
async function lint(code: string, path: string) {
  const [result] = await eslint.lintText(code, { filePath: join(root, path) });
  return (result?.messages ?? []).map(({ ruleId, message }) => ({ ruleId, message }));
}

const VIEW = 'tools/probe/ui/View.svelte';
const RULE = 'navaja/view-imports';

describe('the ESLint config', () => {
  // Loading the config and its plugins takes seconds, and on a loaded Windows
  // machine more than a test's default 5 s, so it happens once, here.
  beforeAll(() => eslint.calculateConfigForFile(join(root, VIEW)), 60_000);

  it('lints custom views in tools/<id>/ui/ with the import allowlist', async () => {
    const messages = await lint(
      "<script lang=\"ts\">\n  import { invoke } from '@tauri-apps/api/core';\n  invoke('x');\n</script>\n",
      VIEW,
    );
    expect(messages).toEqual([
      expect.objectContaining({ ruleId: RULE, message: expect.stringContaining('§4') }),
    ]);
  });

  it('lints every script file a view can hold', async () => {
    // Written out, not read from config.js, so a change to the list fails
    // here; view-files.js lets a view hold scripts with only these extensions.
    const extensions = ['js', 'mjs', 'cjs', 'jsx', 'ts', 'mts', 'cts', 'tsx', 'svelte'];
    expect(VIEW_EXTENSIONS).toEqual(extensions);
    const files = [
      ...extensions.map((extension) => `x.${extension}`),
      'x.svelte.ts',
      'View.test.ts',
      'sub/helper.js',
    ];
    for (const file of files) {
      // A dynamic import parses in a script and a module alike (.cjs is a script).
      const code = "void import('../../../app/src/lib/ipc');\n";
      const messages = await lint(
        file.endsWith('.svelte') ? `<script lang="ts">\n${code}</script>\n` : code,
        `tools/probe/ui/${file}`,
      );
      expect(
        messages.map((m) => m.ruleId),
        file,
      ).toEqual([RULE]);
    }
  });

  it('allows what architecture §4 allows', async () => {
    const messages = await lint(
      '<script lang="ts">\n' +
        "  import { onMount } from 'svelte';\n" +
        "  import { t, type ViewProps } from '$lib/view-kit';\n" +
        "  import type { ToolMeta } from '$bindings/ToolMeta';\n" +
        "  import { label } from './label';\n" +
        '  let { meta }: ViewProps = $props();\n' +
        '  const tool: ToolMeta = meta;\n' +
        '  onMount(() => {});\n' +
        '</script>\n' +
        "<p>{t('tool.probe.label', label)} {tool.id}</p>\n",
      VIEW,
    );
    expect(messages).toEqual([]);
  });

  it('ignores comments that try to switch the allowlist off', async () => {
    const messages = await lint(
      '<script lang="ts">\n' +
        '  /* eslint-disable */\n' +
        "  // eslint-disable-next-line navaja/view-imports\n  import { t } from '$lib/i18n';\n" +
        '</script>\n' +
        '<!-- eslint-disable -->\n' +
        "{#await import('../../../app/src/lib/router.svelte') then m}{t('a', 'b')}{m}{/await}\n",
      VIEW,
    );
    expect(messages.filter((m) => m.ruleId === RULE)).toHaveLength(2);
    // ESLint's own notes that the comments do nothing; `pnpm lint` fails on these too.
    expect(messages.filter((m) => m.ruleId === null)).toHaveLength(2);
  });

  it('leaves the app itself to the base rules', async () => {
    const messages = await lint(
      "import { tools } from '../shell/order';\nvoid tools;\n",
      'app/src/lib/x.ts',
    );
    // Linted (an ignored file would carry a warning) and clean.
    expect(messages).toEqual([]);
  });

  it('is what `pnpm lint` runs, from the repository root, over app/ and tools/*/ui/', () => {
    // view-files.js first refuses any file in a view folder ESLint would skip.
    // `--config` keeps ESLint from picking up a config file placed under tools/.
    const pkg = JSON.parse(readFileSync(join(root, 'app', 'package.json'), 'utf8'));
    expect(pkg.scripts.lint).toMatch(
      /^cd \.\. && node app\/eslint\/view-files\.js && eslint --config eslint\.config\.mjs [^&]* app "tools\/\*\/ui\/\*\*" && /,
    );
  });
});
