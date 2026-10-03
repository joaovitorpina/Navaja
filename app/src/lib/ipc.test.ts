import { describe, expect, it } from 'vitest';
import { envelopeBytes } from '../test/fixtures';
import { decodeEnvelope } from './ipc';

const malformed = {
  ok: false,
  error: { code: 'core.ipc', message: 'malformed run_tool response', details: null },
};

describe('decodeEnvelope', () => {
  it('decodes raw JSON bytes from run_tool', () => {
    expect(decodeEnvelope(envelopeBytes({ ok: { uuids: 'a\nb' } }))).toEqual({
      ok: true,
      value: { uuids: 'a\nb' },
    });
  });

  it('decodes tool errors', () => {
    const error = {
      code: 'uuid.count_out_of_range',
      message: 'Choose between 1 and 10,000 UUIDs.',
      details: null,
    };
    expect(decodeEnvelope(envelopeBytes({ err: error }))).toEqual({ ok: false, error });
  });

  it('accepts already-parsed envelopes', () => {
    expect(decodeEnvelope({ ok: { x: 1 } })).toEqual({ ok: true, value: { x: 1 } });
  });

  it('decodes the number array of the postMessage IPC fallback', () => {
    const bytes = Array.from(new Uint8Array(envelopeBytes({ ok: { uuids: 'a' } })));
    expect(decodeEnvelope(bytes)).toEqual({ ok: true, value: { uuids: 'a' } });
  });

  it('turns any other shape into an explicit error', () => {
    const notJson = new TextEncoder().encode('{"ok": secret').buffer;
    for (const raw of [[], {}, null, undefined, 'text', 42, { err: 'x' }, notJson]) {
      expect(decodeEnvelope(raw)).toEqual(malformed);
    }
  });

  it('passes any JSON value through for custom views', () => {
    for (const value of ['x', 42, null, [1, 2], { rows: [] }]) {
      expect(decodeEnvelope(envelopeBytes({ ok: value }))).toEqual({ ok: true, value });
    }
  });
});
