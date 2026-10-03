// Translation lookup. v1 ships English only: every string has an English
// fallback, from Rust (tool metadata) or from the call site (shell strings),
// so adding a tool never requires editing a catalog here.
//
// Key scheme: tool.<id>.name, tool.<id>.option.<key>.label, category.<id>,
// error.<code>, shell.<area>.<name>.

const catalog: Record<string, string> = {};

export function t(key: string, fallback: string): string {
  return catalog[key] ?? fallback;
}
