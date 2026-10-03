import { describe, expect, it } from 'vitest';
import type { Catalog } from '$bindings/Catalog';
import { uuidTool } from '../test/fixtures';
import { toolGroups } from './order';

const tool = (id: string, name: string, category: string) => ({ ...uuidTool, id, name, category });

describe('toolGroups', () => {
  it('orders like Rust: category order, then lower-cased name by code point, then id', () => {
    const catalog: Catalog = {
      tools: [
        tool('fudge', 'Fudge', 'generators'),
        tool('eclair', 'Éclair', 'generators'),
        tool('b', 'Same', 'generators'),
        tool('a', 'same', 'generators'),
        tool('base64', 'Base64', 'encoders'),
      ],
      categories: [
        { id: 'generators', label: 'Generators', order: 30 },
        { id: 'encoders', label: 'Encoders', order: 10 },
      ],
    };

    const groups = toolGroups(catalog).map((g) => [g.category.id, g.tools.map((t) => t.id)]);

    // "é" (U+00E9) sorts after "f" and "s" by code point, as in Rust; a
    // locale-aware compare would put Éclair first.
    expect(groups).toEqual([
      ['encoders', ['base64']],
      ['generators', ['fudge', 'a', 'b', 'eclair']],
    ]);
  });
});
