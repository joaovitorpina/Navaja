// End-to-end tests: WebdriverIO drives the real app through Tauri's embedded
// WebDriver (tauri-plugin-wdio-webdriver), on Windows, Linux and macOS.
//
// Build first:  VITE_NAVAJA_E2E=1 pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/e2e.conf.json
// Then run:     pnpm e2e
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const exe = process.platform === 'win32' ? 'navaja.exe' : 'navaja';

/** The app under test, overridable (the Linux network guard wraps it with strace). */
export const binary = process.env.NAVAJA_E2E_BINARY ?? join(repo, 'target', 'debug', exe);

// A fresh settings/log directory, so tests never touch the user's own.
process.env.NAVAJA_APP_DIR ??= mkdtempSync(join(tmpdir(), 'navaja-e2e-'));

export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.e2e.ts'],
  maxInstances: 1,
  services: [
    [
      '@wdio/tauri-service',
      {
        driverProvider: 'embedded',
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
  logLevel: 'warn',
  framework: 'mocha',
  mochaOpts: { timeout: 60_000 },
  reporters: ['spec'],
  waitforTimeout: 10_000,
};
