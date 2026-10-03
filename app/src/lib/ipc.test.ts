import { describe, expect, it } from 'vitest';
import { envelopeBytes } from '../test/fixtures';
import { decodeEnvelope } from './ipc';

describe('decodeEnvelope', () => {
  it('decodes raw JSON bytes from run_tool', () => {
    expect(decodeEnvelope(envelopeBytes({ ok: { uuids: 'a\nb' } }))).toEqual({
      ok: true,
      value: { uuids: 'a\nb' },
    });
  });

  it('decodes tool errors', () => {
    const error = { code: 'uuid.count_out_of_range', message: 'Choose between 1 and 10,000 UUIDs.', details: null };
    expect(decodeEnvelope(envelopeBytes({ err: error }))).toEqual({ ok: false, error });
  });

  it('accepts already-parsed envelopes', () => {
    expect(decodeEnvelope({ ok: { x: 1 } })).toEqual({ ok: true, value: { x: 1 } });
  });
});
