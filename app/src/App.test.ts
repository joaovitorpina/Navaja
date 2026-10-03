import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import App from './App.svelte';
import { router } from './lib/router.svelte';
import { mockApp } from './test/fixtures';

describe('shell', () => {
  it('builds navigation from list_tools and reports ready', async () => {
    const calls = mockApp({});
    render(App);

    const nav = await screen.findByRole('navigation', { name: 'Tools' });
    expect(within(nav).getByRole('heading', { name: 'Encoders' })).toBeTruthy();
    expect(within(nav).getByRole('heading', { name: 'Generators' })).toBeTruthy();
    expect(within(nav).getByRole('link', { name: 'UUID generator' })).toBeTruthy();
    await vi.waitFor(() => expect(calls.map((c) => c.cmd)).toContain('shell_ready'));
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

  it('sends changed options and shows tool errors by code', async () => {
    const calls = mockApp({
      runResult: {
        err: { code: 'uuid.count_out_of_range', message: 'Choose between 1 and 10,000 UUIDs.', details: null },
      },
    });
    router.go({ kind: 'tool', id: 'uuid' });
    render(App);

    const version = await screen.findByLabelText('Version');
    await fireEvent.change(version, { target: { value: 'v7' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Generate' }));

    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      'Choose between 1 and 10,000 UUIDs.',
    );
    const runs = calls.filter((c) => c.cmd === 'run_tool');
    expect(runs.at(-1)?.args).toMatchObject({ input: { version: 'v7' } });
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

  it('shows an unknown tool as not found', async () => {
    mockApp({});
    router.go({ kind: 'tool', id: 'nope' });
    render(App);
    expect(await screen.findByText('No such tool.')).toBeTruthy();
  });
});
