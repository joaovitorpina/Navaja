import { execFile } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { $, browser, expect } from '@wdio/globals';
import { accepting, dataFolderRecord, port } from '../wdio.conf';
import { goHome } from '../support/app';

// Roadmap §2: the webview data folder holds no input text. This spec types a
// fresh canary into every text field the app has, then quits the app the
// normal way, so the webview writes what it keeps on exit. It records the
// canary for app/e2e/data-folder-check.mjs and runs that script twice, once
// the app is gone: it must find the canary in no folder where the end-to-end
// build keeps data, and then, as a negative control, find every copy it
// plants in those folders itself. The check is part of the suite, so every
// `pnpm e2e` runs it, on all three OSes in CI (on Linux, inside the strace
// guard's run).
//
// It quits the app, so it runs last: wdio.conf.ts lists it after the other
// specs, from a folder their glob does not reach.

/** The search, a script of its own so it can also be run alone after a suite. */
const CHECK = fileURLToPath(new URL('../data-folder-check.mjs', import.meta.url));

/**
 * How long the script may run. It waits up to a minute for the folders to
 * stop changing after the quit, then reads every file in them.
 */
const CHECK_MS = 150_000;

/** What a person can type text into. Number fields can't hold the canary. */
const TEXT_FIELDS = [
  'input:not([type])',
  'input[type="text"]',
  'input[type="search"]',
  'input[type="url"]',
  'input[type="email"]',
  'input[type="tel"]',
  'input[type="password"]',
  'textarea',
  '[contenteditable]:not([contenteditable="false"])',
].join(', ');

/** The sidebar's filter and its results, which replace the tool list. */
const FILTER = 'nav[aria-label="Tools"] input[type="search"]';
const FILTER_RESULTS = 'nav[aria-label="Tools"] ul[aria-label="Matching tools"]';

/** The command palette's search field. */
const PALETTE = '[role="dialog"] [role="combobox"]';

const NO_MATCH = 'No tools match.';

/**
 * 32 random lower-case letters and digits, new for each run, so the
 * repository can't hold it. URL and JSON escaping leave it as it is.
 */
function freshCanary(): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz0123456789';
  return Array.from(randomBytes(32), (byte) => alphabet[byte % alphabet.length]).join('');
}

/** Types `canary` into a field and checks that the field holds it. */
async function typeInto(selector: string, canary: string): Promise<void> {
  const field = $(selector);
  await field.waitForDisplayed();
  await field.setValue(canary);
  const editable = await field.getAttribute('contenteditable');
  if (editable !== null && editable !== 'false') {
    await expect(field).toHaveText(canary);
  } else {
    await expect(field).toHaveValue(canary);
  }
}

/**
 * Marks the text fields of the page's main area and names each one by its
 * label, placeholder or id. The sidebar filter and the palette, outside it,
 * are typed into on their own.
 */
function markMainFields(selector: string): Promise<string[]> {
  return browser.execute((fields: string) => {
    const name = (field: Element): string => {
      const labelled = (field as HTMLInputElement).labels?.[0]?.textContent?.trim();
      return (
        field.getAttribute('aria-label') ||
        labelled ||
        field.getAttribute('placeholder') ||
        field.id ||
        field.tagName.toLowerCase()
      );
    };
    return [...document.querySelectorAll(`main :is(${fields})`)].map((field, index) => {
      field.setAttribute('data-canary-field', String(index));
      return name(field);
    });
  }, selector);
}

/**
 * Polls `condition` until it holds, without WebdriverIO: once the session is
 * deleted, browser.waitUntil gives up at once.
 */
async function until(condition: () => Promise<boolean> | boolean, what: string): Promise<void> {
  const deadline = Date.now() + 30_000;
  while (!(await condition())) {
    if (Date.now() > deadline) throw new Error(`${what} within 30 s`);
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

/** The app's own log lines (NAVAJA_APP_DIR/logs), to see how it quit. */
function appLog(appDir: string): string {
  const logs = join(appDir, 'logs');
  try {
    return readdirSync(logs)
      .map((file) => readFileSync(join(logs, file), 'utf8'))
      .join('');
  } catch {
    return '';
  }
}

/**
 * Runs data-folder-check.mjs in `mode` and prints its report. Throws with the
 * problems it found when it fails, so they show in the test's failure.
 */
async function runCheck(mode: 'check' | 'control'): Promise<void> {
  let output: { stdout?: string; stderr?: string };
  let failure: unknown = null;
  try {
    output = await promisify(execFile)(process.execPath, [CHECK, mode], {
      encoding: 'utf8',
      timeout: CHECK_MS,
    });
  } catch (error) {
    // A non-zero exit or a timeout; the error carries what the script printed.
    output = error as { stdout?: string; stderr?: string };
    failure = error;
  }
  const report = `${output.stdout ?? ''}${output.stderr ?? ''}`;
  console.log(report.trimEnd());
  if (failure !== null) {
    const problems = report
      .split('\n')
      .filter((line) => line.startsWith('::error::'))
      .map((line) => line.slice('::error::'.length));
    throw new Error(
      problems.length > 0
        ? problems.join('\n')
        : `data-folder-check.mjs ${mode} failed: ${String(failure)}`,
    );
  }
}

describe('webview data folder', () => {
  /** Set once the first test has quit the app and recorded the canary. */
  let recorded = false;

  before(goHome);

  it('takes a canary in every text field, then quits the app', async () => {
    const appDir = process.env.NAVAJA_APP_DIR;
    if (!appDir) throw new Error('NAVAJA_APP_DIR is not set; wdio.conf.ts sets it for each run');
    // A record left by an earlier run names another canary and app folder.
    rmSync(dataFolderRecord, { force: true });

    const canary = freshCanary();
    const fields: string[] = [];

    // Every page the sidebar links to: Home, each tool, Settings and About.
    const routes = await browser.execute(() =>
      [...document.querySelectorAll('nav[aria-label="Tools"] a[href^="#/"]')].map(
        (link) => link.getAttribute('href') ?? '',
      ),
    );
    expect(routes).toContain('#/tool/uuid');
    for (const route of routes) {
      await browser.execute((hash: string) => {
        window.location.hash = hash;
      }, route);
      await browser.waitUntil(
        async () => (await browser.execute(() => window.location.hash)) === route,
        { timeoutMsg: `the page did not open ${route}` },
      );
      await $('main h1').waitForDisplayed();
      const names = await markMainFields(TEXT_FIELDS);
      for (const [index, name] of names.entries()) {
        await typeInto(`main [data-canary-field="${index}"]`, canary);
        fields.push(`${route} ${name}`);
      }
    }

    // The palette asks Rust's `search` for the canary.
    await goHome();
    await browser.keys([process.platform === 'darwin' ? 'Meta' : 'Control', 'k']);
    await typeInto(PALETTE, canary);
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining(NO_MATCH));
    fields.push('command palette search');
    await browser.keys('Escape');
    await expect($('[role="dialog"]')).not.toBeExisting();

    // So does the sidebar's filter, which keeps the canary until the app quits.
    await typeInto(FILTER, canary);
    await expect($(FILTER_RESULTS)).toHaveText(expect.stringContaining(NO_MATCH));
    fields.push('sidebar filter');

    // The `quit` command, the path the tray's Quit and the start-failure
    // view take. The page sends it a second from now, once the session
    // below has ended.
    await browser.execute(() => {
      const internals = (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (command: string) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__;
      setTimeout(() => void internals.invoke('quit'), 1_000);
    });
    // The runner deletes the session when a spec file ends, and that would
    // fail once the app has quit. So the session ends here, while the app
    // still answers, and the runner's browser (the global one; the import
    // above only reads through to it) is told there is none left.
    await browser.deleteSession();
    (globalThis as unknown as { browser: { sessionId?: string } }).browser.sessionId = undefined;

    // The WebDriver server lives in the app's process until that ends.
    await until(async () => !(await accepting(port)), `the app did not quit`);
    await until(
      () => appLog(appDir).includes('quit from="command"'),
      'the app did not log a quit through the command; something else may have stopped it',
    );

    mkdirSync(dirname(dataFolderRecord), { recursive: true });
    writeFileSync(
      dataFolderRecord,
      `${JSON.stringify(
        {
          canary,
          appDir,
          fields,
          platform: process.platform,
          // Where the app looked for its folders: the check computes them
          // from the same values. The app started with this environment.
          env: Object.fromEntries(
            [
              'HOME',
              'LOCALAPPDATA',
              'APPDATA',
              'WEBVIEW2_USER_DATA_FOLDER',
              'XDG_DATA_HOME',
              'XDG_CACHE_HOME',
              'XDG_CONFIG_HOME',
            ].map((name) => [name, process.env[name] ?? null]),
          ),
          quitAt: new Date().toISOString(),
        },
        null,
        2,
      )}\n`,
    );
    console.log(`data folder: typed a fresh canary into ${fields.join(', ')}, then quit the app.`);
    recorded = true;
  });

  it('finds the canary in no folder where the app keeps data', async () => {
    if (!recorded) throw new Error('the app did not quit with a recorded canary (test above)');
    await runCheck('check');
  }).timeout(CHECK_MS + 30_000);

  // The negative control: a search that finds nothing proves something only
  // if it would have found the canary there.
  it('finds a canary planted in each of those folders', async () => {
    if (!recorded) throw new Error('the app did not quit with a recorded canary (test above)');
    await runCheck('control');
  }).timeout(CHECK_MS + 30_000);
});
