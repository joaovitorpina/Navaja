// One tool order for Home and the sidebar. It matches the tie-breaks of
// Rust's search ranking (crates/navaja-core/src/search.rs), so browsing and an
// empty search agree: category order, then category id, then lower-cased name
// compared by code point (as Rust compares strings), then tool id.
import type { Catalog } from '$bindings/Catalog';
import type { CategoryInfo } from '$bindings/CategoryInfo';
import type { ToolMeta } from '$bindings/ToolMeta';

export interface ToolGroup {
  category: CategoryInfo;
  tools: ToolMeta[];
}

export function toolGroups(catalog: Catalog): ToolGroup[] {
  return [...catalog.categories]
    .sort((a, b) => a.order - b.order || byCodePoint(a.id, b.id))
    .map((category) => ({
      category,
      tools: catalog.tools
        .filter((tool) => tool.category === category.id)
        .sort(
          (a, b) =>
            byCodePoint(a.name.toLowerCase(), b.name.toLowerCase()) || byCodePoint(a.id, b.id),
        ),
    }));
}

/** Code point order, like Rust's `str` comparison (`<` compares UTF-16 units). */
function byCodePoint(a: string, b: string): number {
  const x = Array.from(a, (c) => c.codePointAt(0) ?? 0);
  const y = Array.from(b, (c) => c.codePointAt(0) ?? 0);
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d;
  }
  return x.length - y.length;
}
