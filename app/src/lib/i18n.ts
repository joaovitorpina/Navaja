// Translation lookup. v1 ships English only: every string has an English
// fallback, from Rust (tool metadata) or from the call site (shell strings),
// so adding a tool never requires editing a catalog here.
//
// Key scheme (a flat map, so a key may also prefix another):
//   tool.<id>.name, tool.<id>.description, tool.<id>.action.<action>,
//   tool.<id>.option.<key>.label, tool.<id>.option.<key>.choice.<value>,
//   tool.<id>.output.<key>, category.<id>, error.<code>,
//   shell.<name> or shell.<area>.<name>.
// Choice values are free text, so they sit under their own `choice` segment
// and can't collide with an option's `label`.

const catalog: Record<string, string> = {};

export function t(key: string, fallback: string): string {
  return catalog[key] ?? fallback;
}
