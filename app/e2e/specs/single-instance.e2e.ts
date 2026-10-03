import { spawnSync } from 'node:child_process';
import { browser, expect } from '@wdio/globals';
import { binary } from '../wdio.conf';
import { currentHash, goHome } from '../support/app';

describe('single instance', () => {
  beforeEach(goHome);

  it('a second launch hands --tool to the running window and exits', async () => {
    // A second instance that failed to hand over would never exit on its own.
    // SIGKILL ends it at the timeout (under the Linux guard it ends strace,
    // which takes the app down with it) instead of leaving the run hanging.
    const second = spawnSync(binary, ['--tool', 'uuid'], {
      env: process.env,
      timeout: 20_000,
      killSignal: 'SIGKILL',
    });
    expect(second.status).toBe(0);

    await browser.waitUntil(async () => (await currentHash()) === '#/tool/uuid', {
      timeoutMsg: 'the running window did not open the tool',
    });
  });
});
