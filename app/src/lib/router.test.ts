import { describe, expect, it } from 'vitest';
import { href, parseRoute } from './router.svelte';

describe('router', () => {
  it('parses known routes', () => {
    expect(parseRoute('')).toEqual({ kind: 'home' });
    expect(parseRoute('#/')).toEqual({ kind: 'home' });
    expect(parseRoute('#/tool/uuid')).toEqual({ kind: 'tool', id: 'uuid' });
    expect(parseRoute('#/settings')).toEqual({ kind: 'settings' });
    expect(parseRoute('#/about')).toEqual({ kind: 'about' });
  });

  it('rejects malformed tool ids', () => {
    for (const hash of ['#/tool/', '#/tool/UUID', '#/tool/../x', '#/tool/a%2Fb', '#/tool/a b']) {
      expect(parseRoute(hash)).toEqual({ kind: 'home' });
    }
  });

  it('round-trips through href', () => {
    for (const route of [
      { kind: 'home' },
      { kind: 'tool', id: 'base64' },
      { kind: 'settings' },
      { kind: 'about' },
    ] as const) {
      expect(parseRoute(href(route))).toEqual(route);
    }
  });
});
