import { createEvent, fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import type { KeyValueRow } from '$bindings/KeyValueRow';
import type { OutputSpec } from '$bindings/OutputSpec';
import { mockApp } from '../test/fixtures';
import OutputView from './OutputView.svelte';

const keyValue: OutputSpec = { key: 'claims', label: 'Claims', format: { kind: 'key_value' } };
const text: OutputSpec = { key: 'uuids', label: 'UUIDs', format: { kind: 'text' } };

const secretRow = (value: string): KeyValueRow[] => [{ key: 'token', value, secret: true }];

describe('OutputView', () => {
  it('masks secrets again when a new result arrives', async () => {
    mockApp({});
    const view = render(OutputView, { toolId: 'jwt', spec: keyValue, value: secretRow('first') });
    expect(screen.queryByText('first')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Reveal' }));
    expect(screen.getByText('first')).toBeTruthy();

    await view.rerender({ value: secretRow('second') });
    expect(screen.queryByText('second')).toBeNull();
    expect(screen.getByLabelText('Hidden value')).toBeTruthy();
  });

  it('copies a secret row without revealing it', async () => {
    const calls = mockApp({});
    render(OutputView, { toolId: 'jwt', spec: keyValue, value: secretRow('s3cret') });

    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));

    await vi.waitFor(() =>
      expect(calls.filter((c) => c.cmd === 'copy_text').map((c) => c.args)).toEqual([
        { text: 's3cret' },
      ]),
    );
    expect(screen.queryByText('s3cret')).toBeNull();
  });

  it('routes a native copy of the output through copy_text', async () => {
    const calls = mockApp({});
    render(OutputView, { toolId: 'uuid', spec: text, value: 'abc-123' });

    const output = screen.getByText('abc-123');
    const range = document.createRange();
    range.selectNodeContents(output);
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);

    const copy = createEvent.copy(output);
    await fireEvent(output, copy);

    expect(copy.defaultPrevented).toBe(true);
    expect(calls.filter((c) => c.cmd === 'copy_text').map((c) => c.args)).toEqual([
      { text: 'abc-123' },
    ]);
    window.getSelection()?.removeAllRanges();
  });
});
