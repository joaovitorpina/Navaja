import { $, browser, expect } from '@wdio/globals';

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

describe('shell', () => {
  it('opens the UUID generator from the palette and generates a UUID', async () => {
    await $('nav[aria-label="Tools"]').waitForDisplayed();

    await browser.keys([process.platform === 'darwin' ? 'Meta' : 'Control', 'k']);
    const input = $('[role="dialog"] [role="combobox"]');
    await input.waitForDisplayed();
    await input.setValue('uid');

    const first = $('[role="dialog"] [role="option"]');
    await expect(first).toHaveText(expect.stringContaining('UUID generator'));
    await browser.keys('Enter');

    await expect($('h1')).toHaveText('UUID generator');
    const output = $('section[aria-label="UUIDs"] pre');
    await browser.waitUntil(async () => UUID_V4.test(await output.getText()), {
      timeoutMsg: 'no v4 UUID appeared',
    });
  });
});
