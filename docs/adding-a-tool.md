# Adding a tool

A new text tool is **one new folder plus one registration line**. It needs no TypeScript and no changes to the shell. This page covers the folder layout, the rules a tool follows, and what a tool PR may touch. The design behind it is in [architecture.md](architecture.md) §3-§4.

## 1. Create the folder

```
tools/<id>/
├── mod.rs      # the tool: metadata + logic
├── icon.svg    # 24×24, currentColor strokes (assets/brand/GUIDELINES.md)
└── tests.rs    # unit tests (declared with `#[cfg(test)] mod tests;` in mod.rs)
```

**Choosing `<id>`:**
- It must match `[a-z][a-z0-9_]*`.
- It equals the folder name.
- It is permanent: settings and translations are keyed by it.
- Ids containing `__` are reserved for extensions.

## 2. Register it

Add one line to `register_tools!` in [`tools/lib.rs`](../tools/lib.rs), keeping the list sorted:

```rust
register_tools! {
    base64,
    my_tool,   // ← the one line
    uuid,
}
```

`registry_test.rs` fails if a folder with a `mod.rs` is missing from the list, or if the list is not sorted.

## 3. Write `mod.rs`

[`tools/uuid/mod.rs`](../tools/uuid/mod.rs) is the reference generator. The shape:

```rust
use navaja_core::{ActionMeta, Category, Ctx, SPEC_VERSION, Tool, ToolError, ToolId, ToolMeta, UiSpec, Value, typed};
use serde::Deserialize;
use serde_json::json;

pub(crate) const TOOL: MyTool = MyTool;
pub(crate) struct MyTool;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]          // unknown keys are an error, not ignored
struct Input { input: String, #[serde(default)] uppercase: bool }

impl Tool for MyTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            spec_version: SPEC_VERSION,
            id: ToolId::from_static("my_tool"),
            name: "My tool".into(),
            description: "One sentence on what it does.".into(),
            category: Category::ENCODERS,
            keywords: vec!["lower".into(), "case".into()],
            icon: include_str!("icon.svg").into(),
            capabilities: vec![],
            actions: vec![ActionMeta::new("encode", "Encode")],
            tray: false,
            ui: UiSpec::Transform(/* modes, input, options, outputs */),
        }
    }

    fn invoke(&self, action: &str, input: Value, ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        let input: Input = typed(input)?;   // never echoes the input in errors
        ctx.check()?;                       // before any side effect; in long loops too
        Ok(json!({ "text": /* … */ }))      // keys = declared output keys
    }
}

#[cfg(test)]
mod tests;
```

### Choosing the UI

| `UiSpec` | Use it for | What you write |
|---|---|---|
| `Transform` | Input → outputs, with modes (encode/decode, format/minify) | Nothing in TypeScript |
| `Generator` | Options → output, no input (UUID) | Nothing in TypeScript |
| `Custom { view }` | Interactive tools the generic views can't express (the port inspector) | `ui/View.svelte` and `ui/i18n/en.ts` in the tool folder; `view` equals the id |

Prefer `Transform` or `Generator`. A custom view needs a reason in the PR. A `Generator` action can't be `destructive`; the registry rejects it, because the generator view has no confirmation step.

### Writing a custom view

The shell loads `tools/<id>/ui/View.svelte` and passes it `ViewProps` from `$lib/view-kit`. A view may import only (architecture §4):
- its own files, through relative paths that stay inside `tools/<id>/ui/`;
- `svelte` and its browser subpaths, but not `svelte/internal`, `svelte/compiler` or `svelte/server`;
- `$lib/view-kit`, the shell's API for views: `t`, `runTool`, `copyText`, `CopyButton`, `ToolIcon`, `OUTPUT_ATTRIBUTE` and their types;
- generated types from `$bindings/<name>`.

Put `OUTPUT_ATTRIBUTE` (`data-output`) on every element that shows tool output, so the shell sends a native copy (Ctrl/Cmd+C or the context menu) whose selection touches it through Rust, like `copyText`, with the markers that keep it out of clipboard history (architecture §5, Clipboard). A copy from a text field stays native, so don't show output in one.

Its tests (`*.test.ts` and `*.spec.ts` files in `ui/`, which Vitest runs) may also import `vitest`, `@testing-library/svelte` and `@tauri-apps/api/mocks`. They import `vi` by that name (`import { vi } from 'vitest'`), use it only as `vi.<name>`, and call `vi.mock`, `vi.importActual` and Vitest's other calls that take a module directly, so the check can read the module each one takes.

`ui/` holds only scripts ESLint lints (`.js`, `.mjs`, `.cjs`, `.jsx`, `.ts`, `.mts`, `.cts`, `.tsx`, `.svelte`) and the assets `.css`, `.svg`, `.json`, `.png` and `.webp`, with lower-case extensions. It has no `node_modules` folder, and nothing in the tool folder is a symbolic link, a submodule, a `package.json` or a `tsconfig.json`. ESLint would never see any other file, but Vite would still bundle it, and Vite reads those two to resolve imports.

`pnpm lint` must pass. It checks the files in `ui/`, these imports and the Prettier formatting of `ui/`, CSS and JSON included (`pnpm format` fixes the formatting). A comment can't turn the check off, and `import.meta.glob`, a computed `import()`, JSX or a `@jsxImportSource` comment is an error. If a view needs something else from the shell, add it to `$lib/view-kit` in a `host-change` PR first.

CSS is not checked: ESLint reads neither `<style>` blocks nor `.css` files. A view's CSS `@import` and `url()` must also stay inside `tools/<id>/ui/`, and review checks them.

### Outputs

Each output in the spec has a `key` and a format. `invoke` returns one JSON object containing every declared key, with no extra keys. Use `null` when there is nothing to show. With debug assertions (the default for `cargo test`), the registry rejects any other shape with `core.invalid_output`; each value must round-trip exactly through its payload type.

| Format | Value |
|---|---|
| `Text`, `Code { lang }` | string |
| `KeyValue` | `[{ key, value, note?, secret }]` (`KeyValueRow`) |
| `Diagnostics` | `[{ severity, code, message, line, column, details }]` (`Diagnostic`) |
| `Binary` | `{ len, hex }` (`BinaryValue::from_bytes`) |

## 4. Rules every tool follows

These are enforced by lints, cargo-deny, `registry_test.rs` and review:

- **Pure and synchronous.**
  - Never print, prompt, start an async runtime or open a connection.
  - Never take a file path; v1 text tools work on pasted text.
- **Errors carry stable codes:** `<id>.<snake_case>`, or `core.invalid_input` (from `typed()`) and `core.cancelled` (from `ctx.check()`). Diagnostics use `<id>.*` codes.
  - The message is an English fallback that **never repeats the input**: no "invalid token `eyJ…`".
  - Put structured facts (ranges, positions) in `details`.
- **Parse input with `typed()`** on a `#[serde(deny_unknown_fields)]` struct. Never use maps keyed by user content.
- **Call `ctx.check()` before any side effect.** `registry_test.rs` calls every action with a pre-cancelled context and a junk input, and expects `core.invalid_input` or `core.cancelled`. It also sends each option's default, every choice and both integer bounds, as the generic views do.
- **No secrets in logs.** Tools don't log. The shell logs only the tool id, action, duration and error code.
- **Dependencies:**
  - **Allowed:** small, maintained crates with a licence on the `deny.toml` allowlist.
  - **Not allowed:** Tauri, tokio, HTTP clients and socket crates. `cargo xtask check` and cargo-deny reject them.

## 5. Test it

```sh
cargo test -p navaja-tools            # your tests + the registry checks
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask check                     # dependency rules
cargo xtask tool-gate origin/main     # what your PR touches
pnpm lint && pnpm test                # custom views only: the import allowlist, Prettier, Vitest
```

In tests, call your tool through `navaja_core::run_single(TOOL, "action", input)`, not `invoke`. That way the output and error-code checks apply, as in `tools/uuid/tests.rs`.

Test the behaviour, not the plumbing:
- each mode on typical and edge input (empty, huge, non-ASCII, malformed);
- every error code;
- the property that matters, such as round trips (`decode(encode(x)) == x`) with proptest.

To try the tool in the app, run `pnpm dev` from the repository root. Quit any other Navaja first, an installed one included. Navaja runs as a single instance, so a dev or debug build that finds another one running hands its arguments to it and exits with code 0, and you see the other app's window. End-to-end builds use their own identifier, so they are not affected.

## 6. What a tool PR may touch

| Change | May touch | Never touches |
|---|---|---|
| **Text tool** | `tools/<id>/**`, one line in `tools/lib.rs`, `tools/Cargo.toml`, `Cargo.lock` | anything else |
| **System tool** | The text-tool set, plus target-specific dependencies, an optional new `crates/navaja-<x>/`, generated `app/src/bindings/**`, and `tools/<id>/ui/**` | `app/src/{shell,generic,lib}`, `app/src-tauri`, `navaja-core`, `xtask` |

If your tool needs something the host doesn't offer, such as a new output format or capability, open a separate `host-change` PR first. Examples are a new `OutputKind`, a new `Capability` or a new host service.

Tauri commands and front-end code stay out of a tool PR.
