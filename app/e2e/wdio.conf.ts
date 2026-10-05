// End-to-end tests: WebdriverIO drives the real app through Tauri's embedded
// WebDriver (tauri-plugin-wdio-webdriver), on Windows, Linux and macOS.
//
// Build first:  pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/e2e.conf.json
// Then run:     pnpm e2e
//
// The service launches the app once, with `--tool uuid`, and every spec drives
// that same process. launch.e2e.ts checks the window the app opened on, so it
// runs first; every other spec resets the route itself. last/data-folder.e2e.ts
// quits the app, so it runs last.
import { mkdtempSync } from 'node:fs';
import { connect } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const exe = process.platform === 'win32' ? 'navaja.exe' : 'navaja';

/** The app under test, overridable (the Linux network guard wraps it with strace). */
export const binary = process.env.NAVAJA_E2E_BINARY ?? join(repo, 'target', 'debug', exe);

/**
 * Where last/data-folder.e2e.ts records its canary for
 * app/e2e/data-folder-check.mjs, which reads the same path.
 */
export const dataFolderRecord = join(repo, 'app', 'e2e', 'output', 'data-folder.json');

/** The embedded WebDriver port, resolved the way @wdio/tauri-service does. */
export const port = Number.parseInt(process.env.TAURI_WEBDRIVER_PORT ?? '', 10) || 4445;

/** Whether something already accepts connections on 127.0.0.1:`port`. */
export function accepting(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = connect({ host: '127.0.0.1', port });
    socket.setTimeout(5_000);
    socket.once('connect', () => {
      socket.destroy();
      resolve(true);
    });
    socket.once('timeout', () => {
      socket.destroy();
      resolve(false);
    });
    socket.once('error', () => resolve(false));
  });
}

// This file loads in the launcher and again in every worker. Workers inherit
// the launcher's environment, so only the launcher prepares the run.
if (!process.env.WDIO_WORKER_ID) {
  // The service drives the first WebDriver server that answers on the port,
  // which could be a leftover app from an aborted run rather than this build.
  if (await accepting(port)) {
    throw new Error(
      `127.0.0.1:${port} already accepts connections, so the end-to-end run would not drive the ` +
        'app it starts. Close whatever holds the port (often a navaja left over from an aborted ' +
        'run), or set TAURI_WEBDRIVER_PORT to a free port.',
    );
  }

  // A fresh settings/log directory, even over one the developer exported, so
  // tests never touch real ones.
  process.env.NAVAJA_APP_DIR = mkdtempSync(join(tmpdir(), 'navaja-e2e-'));
}

export const config: WebdriverIO.Config = {
  runner: 'local',
  // launch.e2e.ts runs first, and the glob adds the rest of specs/ once each.
  // The data-folder spec quits the app, so it runs last; it lives outside
  // specs/, where the glob can't place it earlier.
  specs: ['./specs/launch.e2e.ts', './specs/**/*.e2e.ts', './last/data-folder.e2e.ts'],
  maxInstances: 1,
  services: [
    [
      '@wdio/tauri-service',
      {
        driverProvider: 'embedded',
        // The first launch opens the tool named here (launch.e2e.ts). The
        // embedded provider passes appArgs; it ignores tauri:options.args.
        appArgs: ['--tool', 'uuid'],
        startTimeout: 60_000,
        captureBackendLogs: false,
        captureFrontendLogs: false,
        // No invoke mocking in these tests; skip the per-session mock cleanup.
        clearMocks: false,
        resetMocks: false,
        restoreMocks: false,
      },
    ],
  ],
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': { application: binary },
    } as WebdriverIO.Capabilities,
  ],
  // Navaja has one window: pin each session to it. An explicit switch also
  // stops the service from probing window focus before every element lookup.
  // That probe needs WebdriverIO's front-end bridge (@wdio/tauri-plugin), which
  // these builds leave out, and it would wait 5 s for it each time.
  before: async (_capabilities, _specs, browser) => {
    await browser.switchToWindow(await browser.getWindowHandle());
  },
  logLevel: 'warn',
  framework: 'mocha',
  mochaOpts: { timeout: 60_000 },
  reporters: ['spec'],
  waitforTimeout: 10_000,
};
