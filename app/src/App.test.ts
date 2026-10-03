import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import { router } from './lib/router.svelte';
import { mockApp } from './test/fixtures';

const countError = {
  err: { code: 'uuid.count_out_of_range', message: 'Choose between 1 and 10,000 UUIDs.', details: null },
};

describe('shell', () => {
  it('builds navigation from list_tools and reports ready after the first frame', async () => {
    let navShown: boolean | undefined;
    const calls = mockApp({
      commands: {
        shell_ready: () => {
          navShown = screen.queryByRole('navigation', { name: 'Tools' }) !== null;
          return null;
        },
      },
    });
    render(App);

    const nav = await screen.findByRole('navigation', { name: 'Tools' });
    expect(within(nav).getByRole('heading', { name: 'Encoders' })).toBeTruthy();
    expect(within(nav).getByRole('heading', { name: 'Generators' })).toBeTruthy();
    expect(within(nav).getByRole('link', { name: 'UUID generator' })).toBeTruthy();
    await vi.waitFor(() => expect(calls.map((c) => c.cmd)).toContain('shell_ready'));
    expect(navShown).toBe(true);
    // Not elevated: no banner.
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('shows the start failure before reporting ready', async () => {
    let alertShown: boolean | undefined;
    const calls = mockApp({
      commands: {
        list_tools: () => {
          throw new Error('registry unavailable');
        },
        shell_ready: () => {
          alertShown = screen.queryByRole('alert') !== null;
          return null;
        },
      },
    });
    render(App);

    expect((await screen.findByRole('alert')).textContent).toContain(
      'Navaja could not load its tools.',
    );
    await vi.waitFor(() => expect(calls.map((c) => c.cmd)).toContain('shell_ready'));
    expect(alertShown).toBe(true);
  });

  it('runs a generator on open and shows its output', async () => {
    const calls = mockApp({ runResult: { ok: { uuids: '0190e2b4-aaaa-7bbb-8ccc-ddddeeeeffff' } } });
    router.go({ kind: 'tool', id: 'uuid' });
    render(App);

    expect(await screen.findByRole('heading', { name: 'UUID generator', level: 1 })).toBeTruthy();
    expect(await screen.findByText('0190e2b4-aaaa-7bbb-8ccc-ddddeeeeffff')).toBeTruthy();

    const run = calls.find((c) => c.cmd === 'run_tool');
    expect(run?.args).toMatchObject({
      tool: 'uuid',
      action: 'generate',
      input: { version: 'v4', count: 1, uppercase: false },
    });
  });

  it('sends changed options and shows the error of the run the user started', async () => {
    // The run on open succeeds, so the alert can only come from the click.
    const calls = mockApp({ runResult: (i) => (i === 0 ? { ok: { uuids: 'A' } } : countError) });
    router.go({ kind: 'tool', id: 'uuid' });
    render(App);

    expect(await screen.findByText('A')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();

    await fireEvent.change(screen.getByLabelText('Version'), { target: { value: 'v7' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Generate' }));

    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      'Choose between 1 and 10,000 UUIDs.',
    );
    const runs = calls.filter((c) => c.cmd === 'run_tool');
    expect(runs).toHaveLength(2);
    expect(runs[1]?.args).toMatchObject({ input: { version: 'v7' } });
  });

  it('replaces the output with the result of the run the user started', async () => {
    mockApp({ runResult: (i) => ({ ok: { uuids: i === 0 ? 'A' : 'B' } }) });
    router.go({ kind: 'tool', id: 'uuid' });
    render(App);

    expect(await screen.findByText('A')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Generate' }));

    expect(await screen.findByText('B')).toBeTruthy();
    expect(screen.queryByText('A')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('does not generate while How many is blank or not a number', async () => {
    const calls = mockApp({ runResult: { ok: { uuids: 'A' } } });
    router.go({ kind: 'tool', id: 'uuid' });
    render(App);
    await screen.findByText('A');

    const count = screen.getByLabelText<HTMLInputElement>('How many');
    for (const typed of ['', 'abc']) {
      await fireEvent.input(count, { target: { value: typed } });
      expect(count.validity.valid).toBe(false);
      await fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    }
    expect(calls.filter((c) => c.cmd === 'run_tool')).toHaveLength(1);

    await fireEvent.input(count, { target: { value: '3' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    await vi.waitFor(() => expect(calls.filter((c) => c.cmd === 'run_tool')).toHaveLength(2));
    expect(calls.filter((c) => c.cmd === 'run_tool')[1]?.args).toMatchObject({
      input: { count: 3 },
    });
  });

  it('copies output through copy_text', async () => {
    const calls = mockApp({ runResult: { ok: { uuids: 'X' } } });
    router.go({ kind: 'tool', id: 'uuid' });
    render(App);
    await screen.findByText('X');

    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));

    expect(await screen.findByRole('button', { name: 'Copied' })).toBeTruthy();
    expect(calls.filter((c) => c.cmd === 'copy_text').map((c) => c.args)).toEqual([{ text: 'X' }]);
  });

  it('says so when a copy fails', async () => {
    mockApp({
      runResult: { ok: { uuids: 'X' } },
      commands: {
        copy_text: () => {
          throw new Error('clipboard unavailable');
        },
      },
    });
    router.go({ kind: 'tool', id: 'uuid' });
    render(App);
    await screen.findByText('X');

    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));

    expect(await screen.findByRole('button', { name: 'Copy failed' })).toBeTruthy();
  });

  it('filters the sidebar through Rust search', async () => {
    const calls = mockApp({ search: (q) => (q === 'uid' ? [{ id: 'uuid', score: 23 }] : []) });
    router.go({ kind: 'home' });
    render(App);

    const filter = await screen.findByRole('searchbox', { name: 'Filter tools' });
    await fireEvent.input(filter, { target: { value: 'uid' } });

    const results = await screen.findByRole('list', { name: 'Matching tools' });
    expect(within(results).getAllByRole('link').map((a) => a.textContent?.trim())).toEqual([
      'UUID generator',
    ]);
    expect(calls.some((c) => c.cmd === 'search' && c.args.query === 'uid')).toBe(true);
  });

  it('warns when running elevated', async () => {
    mockApp({ elevated: true });
    render(App);
    expect(await screen.findByRole('status')).toHaveProperty(
      'textContent',
      expect.stringContaining('running as administrator'),
    );
  });

  it('opens the palette when the tray asks', async () => {
    mockApp({});
    render(App);
    await screen.findByRole('navigation', { name: 'Tools' });
    window.dispatchEvent(new Event('navaja:palette'));
    expect(await screen.findByRole('dialog')).toBeTruthy();
  });

  it('exposes the highlighted palette result to assistive technology', async () => {
    mockApp({
      search: () => [
        { id: 'other', score: 0 },
        { id: 'uuid', score: 0 },
      ],
    });
    router.go({ kind: 'home' });
    render(App);

    await fireEvent.click(await screen.findByRole('button', { name: 'Search all tools' }));
    const input = await screen.findByRole('combobox');
    const options = await screen.findAllByRole('option');
    expect(options.map((o) => o.textContent?.trim())).toEqual([
      expect.stringContaining('Other encoder'),
      expect.stringContaining('UUID generator'),
    ]);

    await vi.waitFor(() => expect(input.getAttribute('aria-activedescendant')).toBe(options[0]?.id));
    const controls = input.getAttribute('aria-controls');
    expect(controls).toBeTruthy();
    expect(document.getElementById(controls ?? '')?.contains(options[0] ?? null)).toBe(true);
  });

  it('puts the theme back when saving it fails', async () => {
    const calls = mockApp({
      commands: {
        settings_set: () => {
          throw new Error('settings file is read-only');
        },
      },
    });
    router.go({ kind: 'settings' });
    render(App);

    const theme = await screen.findByLabelText<HTMLSelectElement>('Theme');
    expect(theme.value).toBe('system');
    await fireEvent.change(theme, { target: { value: 'dark' } });

    expect((await screen.findByRole('alert')).textContent).toContain('settings file is read-only');
    expect(theme.value).toBe('system');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    expect(calls.find((c) => c.cmd === 'settings_set')?.args).toEqual({
      settings: { version: 1, theme: 'dark' },
    });
  });

  it('shows an unknown tool as not found', async () => {
    mockApp({});
    router.go({ kind: 'tool', id: 'nope' });
    render(App);
    expect(await screen.findByText('No such tool.')).toBeTruthy();
  });
});
