import { $, browser, expect } from '@wdio/globals';
import { currentHash, UUID_V4 } from '../support/app';

// The harness starts the app with `--tool uuid` (wdio.conf.ts), as a desktop
// shortcut would. On a first launch the route comes from an initialization
// script that runs before the page (window.rs, route_script), so the window
// must already show the tool before this test touches anything. This spec
// runs first; the others move the route.
describe('first launch', () => {
  it('opens the tool named by --tool', async () => {
    await expect($('h1')).toHaveText('UUID generator');
    expect(await currentHash()).toBe('#/tool/uuid');

    const output = $('section[aria-label="UUIDs"] pre');
    await browser.waitUntil(async () => UUID_V4.test(await output.getText()), {
      timeoutMsg: 'no v4 UUID appeared',
    });
  });
});
