import { $, browser, expect } from '@wdio/globals';
import { currentHash, goHome, UUID_V4 } from '../support/app';

describe('shell', () => {
  beforeEach(goHome);

  it('opens the UUID generator from the palette and generates a UUID', async () => {
    await $('nav[aria-label="Tools"]').waitForDisplayed();

    await browser.keys([process.platform === 'darwin' ? 'Meta' : 'Control', 'k']);
    const input = $('[role="dialog"] [role="combobox"]');
    await input.waitForDisplayed();

    // The palette lists every tool as it opens, so a check on the first option
    // could pass on that list before the "uid" results arrive. Empty the list
    // first with a query that matches nothing; then one Backspace makes the
    // query "uid", and the next list can only be its results.
    const option = '[role="dialog"] [role="option"]';
    await input.setValue('uidx');
    await expect($('[role="dialog"]')).toHaveText(expect.stringContaining('No tools match.'));
    await expect($(option)).not.toBeExisting();
    await browser.keys('Backspace');
    await expect(input).toHaveValue('uid');

    // Look the option up only now. `$()` queries as soon as it is called, and an
    // option found in the opening list is removed when the list empties. Linux
    // and macOS report that element as stale, and WebdriverIO finds it again.
    // On Windows, WebView2 turns the embedded WebDriver's stale-element error
    // into a null result, so the text reads "null" and nothing finds it again
    // (run 37253449385). An option not found yet is fine: WebdriverIO waits
    // for it.
    const first = $(option);
    await expect(first).toHaveText(expect.stringContaining('UUID generator'));
    await expect(first).toHaveAttribute('aria-selected', 'true');
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
