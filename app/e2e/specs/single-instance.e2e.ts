import { spawnSync } from 'node:child_process';
import { browser, expect } from '@wdio/globals';
import { binary } from '../wdio.conf';

describe('single instance', () => {
  it('a second launch hands --tool to the running window and exits', async () => {
    await browser.execute(() => {
      window.location.hash = '#/';
    });

    const second = spawnSync(binary, ['--tool', 'uuid'], { env: process.env, timeout: 20_000 });
    expect(second.status).toBe(0);

    await browser.waitUntil(
      async () => (await browser.execute(() => window.location.hash)) === '#/tool/uuid',
      { timeoutMsg: 'the running window did not open the tool' },
    );
  });
});
