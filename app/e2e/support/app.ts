import { $, browser, expect } from '@wdio/globals';

export const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

/**
 * Puts the shared app on Home with no dialog open. Every spec drives the same
 * process, so none may rely on the route an earlier spec left behind.
 */
export async function goHome(): Promise<void> {
  await browser.execute(() => {
    window.location.hash = '#/';
  });
  await expect($('h1')).toHaveText('Navaja');
  await expect($('[role="dialog"]')).not.toBeExisting();
}

/** The page's current route (its location hash). */
export function currentHash(): Promise<string> {
  return browser.execute(() => window.location.hash);
}
