import { $, browser, expect } from '@wdio/globals';
import { currentHash, goHome, UUID_V4 } from '../support/app';

describe('shell', () => {
  beforeEach(goHome);

  it('opens the UUID generator from the palette and generates a UUID', async () => {
    await $('nav[aria-label="Tools"]').waitForDisplayed();

    await browser.keys([process.platform === 'darwin' ? 'Meta' : 'Control', 'k']);
    const input = $('[role="dialog"] [role="combobox"]');
    await input.waitForDisplayed();
    await input.setValue('uid');

    const first = $('[role="dialog"] [role="option"]');
    await expect(first).toHaveText(expect.stringContaining('UUID generator'));
    await browser.keys('Enter');

    // The test started on Home, so only the palette can have opened the tool.
    await browser.waitUntil(async () => (await currentHash()) === '#/tool/uuid', {
      timeoutMsg: 'choosing the palette entry did not open the tool',
    });
    await expect($('[role="dialog"]')).not.toBeExisting();
    await expect($('h1')).toHaveText('UUID generator');
    const output = $('section[aria-label="UUIDs"] pre');
    await browser.waitUntil(async () => UUID_V4.test(await output.getText()), {
      timeoutMsg: 'no v4 UUID appeared',
    });
  });
});
