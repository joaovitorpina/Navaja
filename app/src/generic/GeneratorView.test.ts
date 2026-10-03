import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { RunEnvelope } from '$bindings/RunEnvelope';
import { mockApp, uuidTool } from '../test/fixtures';
import GeneratorView from './GeneratorView.svelte';

/** `run_tool` answers that stay pending until the test resolves them. */
function deferredRuns() {
  const pending: ((envelope: RunEnvelope) => void)[] = [];
  const answer = () => new Promise<RunEnvelope>((resolve) => pending.push(resolve));
  const resolve = (index: number, uuids: string) => pending[index]?.({ ok: { uuids } });
  return { answer, resolve, count: () => pending.length };
}

const runIds = (calls: { cmd: string; args: Record<string, unknown> }[], cmd: string) =>
  calls.filter((c) => c.cmd === cmd).map((c) => c.args.runId);

describe('GeneratorView', () => {
  it('cancels the previous run and never lets its late result win', async () => {
    const runs = deferredRuns();
    const calls = mockApp({ runResult: runs.answer });
    render(GeneratorView, { meta: uuidTool, spec: uuidTool.ui });

    await vi.waitFor(() => expect(runs.count()).toBe(1)); // the run on open
    await fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    await vi.waitFor(() => expect(runs.count()).toBe(2));

    const [first, second] = runIds(calls, 'run_tool');
    expect(runIds(calls, 'cancel_run')).toEqual([first]);

    runs.resolve(1, 'B');
    expect(await screen.findByText('B')).toBeTruthy();
    runs.resolve(0, 'A');
    await new Promise((settle) => setTimeout(settle, 0));
    expect(screen.queryByText('A')).toBeNull();
    expect(screen.getByText('B')).toBeTruthy();
    expect(second).not.toBe(first);
  });

  it('cancels a pending run when the view goes away', async () => {
    const runs = deferredRuns();
    const calls = mockApp({ runResult: runs.answer });
    const { unmount } = render(GeneratorView, { meta: uuidTool, spec: uuidTool.ui });
    await vi.waitFor(() => expect(runs.count()).toBe(1));

    unmount();

    await vi.waitFor(() => expect(runIds(calls, 'cancel_run')).toEqual(runIds(calls, 'run_tool')));
  });

  it('never runs a destructive action without a confirmation', async () => {
    const meta = {
      ...uuidTool,
      actions: [{ id: 'generate', label: 'Generate', destructive: true }],
    };
    const calls = mockApp({});
    render(GeneratorView, { meta, spec: meta.ui });

    const button = screen.getByRole<HTMLButtonElement>('button', { name: 'Generate' });
    expect(button.disabled).toBe(true);
    expect(screen.getByText(/needs a confirmation step/)).toBeTruthy();
    await fireEvent.submit(button.form ?? button);
    expect(calls.some((c) => c.cmd === 'run_tool')).toBe(false);
  });
});
