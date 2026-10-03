import { createEvent, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { KeyValueRow } from '$bindings/KeyValueRow';
import type { OutputSpec } from '$bindings/OutputSpec';
import { routeOutputCopies } from '$lib/copy';
import { mockApp } from '../test/fixtures';
import OutputView from './OutputView.svelte';

const keyValue: OutputSpec = { key: 'claims', label: 'Claims', format: { kind: 'key_value' } };
const text: OutputSpec = { key: 'uuids', label: 'UUIDs', format: { kind: 'text' } };

const secretRow = (value: string): KeyValueRow[] => [{ key: 'token', value, secret: true }];

function select(start: Node, end: Node = start) {
  const range = document.createRange();
  range.setStartBefore(start);
  range.setEndAfter(end);
  window.getSelection()?.removeAllRanges();
  window.getSelection()?.addRange(range);
}

const copiedTexts = (calls: { cmd: string; args: unknown }[]) =>
  calls.filter((c) => c.cmd === 'copy_text').map((c) => c.args);

describe('OutputView', () => {
  let unroute: () => void;
  beforeEach(() => (unroute = routeOutputCopies()));
  afterEach(() => {
    unroute();
    window.getSelection()?.removeAllRanges();
  });

  it('masks secrets again when a new result arrives', async () => {
    mockApp({});
    const view = render(OutputView, { toolId: 'jwt', spec: keyValue, value: secretRow('first') });
    expect(screen.queryByText('first')).toBeNull();
    // Unselectable, so secrets never reach the X11/Wayland primary selection.
    expect(screen.getByLabelText('Hidden value').parentElement?.classList).toContain('select-none');

    await fireEvent.click(screen.getByRole('button', { name: 'Reveal' }));
    expect(screen.getByText('first')).toBeTruthy();
    expect(screen.getByText('first').classList).toContain('select-none');

    await view.rerender({ value: secretRow('second') });
    expect(screen.queryByText('second')).toBeNull();
    expect(screen.getByLabelText('Hidden value')).toBeTruthy();
  });

  it('copies a secret row without revealing it', async () => {
    const calls = mockApp({});
    render(OutputView, { toolId: 'jwt', spec: keyValue, value: secretRow('s3cret') });

    await fireEvent.click(screen.getByRole('button', { name: 'Copy: token' }));

    await vi.waitFor(() => expect(copiedTexts(calls)).toEqual([{ text: 's3cret' }]));
    expect(screen.queryByText('s3cret')).toBeNull();
  });

  it('routes a native copy of the output through copy_text', async () => {
    const calls = mockApp({});
    render(OutputView, { toolId: 'uuid', spec: text, value: 'abc-123' });

    const output = screen.getByText('abc-123');
    select(output);
    const copy = createEvent.copy(output);
    await fireEvent(output, copy);

    expect(copy.defaultPrevented).toBe(true);
    expect(copiedTexts(calls)).toEqual([{ text: 'abc-123' }]);
  });

  it('routes a copy whose selection starts outside the output', async () => {
    const calls = mockApp({});
    const before = document.createElement('nav');
    before.textContent = 'Tools';
    document.body.prepend(before);
    render(OutputView, { toolId: 'uuid', spec: text, value: 'abc-123' });

    // Like Ctrl+A on a tool page: the event fires where the selection starts.
    select(before, screen.getByText('abc-123'));
    const copy = createEvent.copy(before);
    await fireEvent(before, copy);

    expect(copy.defaultPrevented).toBe(true);
    expect(copiedTexts(calls)).toHaveLength(1);
    before.remove();
  });

  it('leaves copies that do not touch any output native', async () => {
    const calls = mockApp({});
    const elsewhere = document.createElement('p');
    elsewhere.textContent = 'not output';
    document.body.append(elsewhere);
    render(OutputView, { toolId: 'uuid', spec: text, value: 'abc-123' });

    select(elsewhere);
    const copy = createEvent.copy(elsewhere);
    await fireEvent(elsewhere, copy);

    expect(copy.defaultPrevented).toBe(false);
    expect(copiedTexts(calls)).toEqual([]);
    elsewhere.remove();
  });
});
