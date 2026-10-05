# Adding a tool

A new text tool is **one new folder plus one registration line**. It needs no TypeScript and no changes to the shell. This page covers the folder layout, the rules a tool follows, and what a tool PR may touch. The design behind it is in [architecture.md](architecture.md) §3-§4.

## 1. Create the folder

Copy the reference generator, [`tools/uuid/`](../tools/uuid/), to `tools/<id>/`, or start from the `Transform` example in §3.

```
tools/<id>/
├── mod.rs      # the tool: metadata + logic
├── icon.svg    # 24×24, currentColor strokes
└── tests.rs    # unit tests (declared with `#[cfg(test)] mod tests;` in mod.rs)
```

**Choosing `<id>`:**
- It must match `[a-z][a-z0-9_]*` and be at most 64 bytes.
- It equals the folder name.
- It is permanent: settings and translations are keyed by it.
- Ids containing `__` are reserved for extensions, and `core` is reserved for the host.
- It can't be a Rust keyword or reserved word, such as `type`, `match`, `ref`, `box`, `try` or `gen`. `register_tools!` declares `mod <id>;`, which doesn't compile for a keyword, and the raw form `r#type` doesn't match the folder name.

**`icon.svg`** follows "Tool icons" in [GUIDELINES.md](../assets/brand/GUIDELINES.md#tool-icons-toolsidiconsvg). The registry rejects an icon that breaks these rules ([`icon.rs`](../crates/navaja-core/src/icon.rs)):
- The root is `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">`.
- **Elements:** `svg`, `g`, `path`, `circle`, `ellipse`, `rect`, `line`, `polyline`, `polygon`.
- **Attributes:** only the geometry and paint attributes listed in `icon.rs`, such as `d`, `points`, `fill`, `stroke-width`, `opacity` and `transform`. `fill` and `stroke` are `none` or `currentColor`. No `style`, `class`, `href` or event attributes.
- No text and no processing instructions; at most 8 KiB and 128 nodes.

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
  <path d="M4 5h16M11 10h9M11 14h9M4 19h16M4 9.5l3 2.5-3 2.5"/>
</svg>
```

## 2. Register it

Add one line, `<id>,`, to `register_tools!` in [`tools/lib.rs`](../tools/lib.rs). Today the list holds only `uuid`, so adding `my_tool` gives:

```rust
register_tools! {
    my_tool,
    uuid,
}
```

- **Order:** byte by byte, as Rust sorts `&str`. A prefix comes first, then digits, then `_`, then letters: `json`, `json5`, `json_path`, `jsonl`.
- [`registry_test.rs`](../tools/registry_test.rs) fails if a folder with a `mod.rs` is missing from the list, or if the list is out of order.
- `tool-gate` (§5) accepts only added lines that read exactly `<id>,`, one per new tool. Don't add a comment to the line or edit any other line.

## 3. Write `mod.rs`

A complete `Transform` tool with two modes and one option. With the icon above and the `tests.rs` from §5, it compiles as is in `tools/my_tool/`:

```rust
//! My tool: indents every line of pasted text, or strips the spaces and tabs
//! that start each line.

use navaja_core::{
    ActionMeta, Category, Control, Ctx, ErrorCode, InputSpec, OptionSpec, OutputKind, OutputSpec,
    SPEC_VERSION, Tool, ToolError, ToolId, ToolMeta, TransformSpec, UiSpec, Value, typed,
};
use serde::Deserialize;
use serde_json::json;

pub(crate) const TOOL: MyTool = MyTool;

pub(crate) struct MyTool;

// One const per value, shared by the spec, serde and the bounds check.
const MIN_WIDTH: u32 = 0;
const MAX_WIDTH: u32 = 16;
const DEFAULT_WIDTH: u32 = 2;
const WIDTH_OUT_OF_RANGE: ErrorCode = ErrorCode::from_static("my_tool.width_out_of_range");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)] // unknown keys are an error, not ignored
struct Input {
    input: String, // the pasted text
    #[serde(default = "default_width")]
    width: u32, // the option whose key is "width"
}

fn default_width() -> u32 {
    DEFAULT_WIDTH
}

impl Tool for MyTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            spec_version: SPEC_VERSION,
            id: ToolId::from_static("my_tool"),
            name: "My tool".into(),
            description: "Indents every line, or strips its leading spaces and tabs.".into(),
            category: Category::FORMATTERS,
            keywords: vec!["indent".into(), "dedent".into()],
            icon: include_str!("icon.svg").into(),
            capabilities: vec![],
            actions: vec![
                ActionMeta::new("indent", "Indent"),
                ActionMeta::new("dedent", "Dedent"),
            ],
            tray: false,
            ui: UiSpec::Transform(TransformSpec {
                modes: vec!["indent".into(), "dedent".into()], // the first is the default
                input: InputSpec {
                    lang: None,
                    placeholder: Some("Paste text".into()),
                },
                options: vec![OptionSpec {
                    key: "width".into(),
                    label: "Width".into(),
                    control: Control::Integer {
                        min: i64::from(MIN_WIDTH),
                        max: i64::from(MAX_WIDTH),
                        default: i64::from(DEFAULT_WIDTH),
                    },
                    modes: vec!["indent".into()], // empty means every mode
                }],
                outputs: vec![OutputSpec {
                    key: "text".into(),
                    label: "Result".into(),
                    format: OutputKind::Text,
                }],
                live: true,
            }),
        }
    }

    fn invoke(&self, action: &str, input: Value, ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        let input: Input = typed(input)?; // never echoes the input in errors
        if !(MIN_WIDTH..=MAX_WIDTH).contains(&input.width) {
            let message = format!("Choose a width from {MIN_WIDTH} to {MAX_WIDTH}.");
            return Err(ToolError::new(WIDTH_OUT_OF_RANGE, message)
                .with_details(json!({ "min": MIN_WIDTH, "max": MAX_WIDTH })));
        }
        ctx.check()?; // before any side effect; in long loops too
        let text = match action {
            "indent" => indent(&input.input, input.width),
            "dedent" => dedent(&input.input),
            _ => unreachable!("the registry passes only declared actions"),
        };
        Ok(json!({ "text": text })) // every declared output key, no others
    }
}

// `split_inclusive` keeps each line's ending, so `\r\n` and a final newline
// come back unchanged; `lines()` would drop both.
fn indent(text: &str, width: u32) -> String {
    let pad = " ".repeat(width as usize);
    text.split_inclusive('\n')
        .map(|line| format!("{pad}{line}"))
        .collect()
}

// Strips every leading space and tab, not just the indent the lines share.
fn dedent(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| line.trim_start_matches([' ', '\t']))
        .collect()
}

#[cfg(test)]
mod tests;
```

For a `Generator`, start from [`tools/uuid/mod.rs`](../tools/uuid/mod.rs). It imports `GeneratorSpec` instead of `TransformSpec` and `InputSpec`, and `Choice` for its choice options.

**In `invoke`:**
- **Order:** `typed()` first, then checks that need no work, then `ctx.check()`, then the work.
- **Actions:** `Registry::run` answers an undeclared action with `core.unknown_action` and never calls `invoke`, so `action` is always a declared id. A one-action tool can ignore it, as uuid does. A `match` needs its fallback arm only for the compiler.

### Metadata

The top-level `ToolMeta` fields. `Registry::new` rejects a tool that breaks a rule below ([`registry.rs`](../crates/navaja-core/src/registry.rs)).

| Field | Meaning |
|---|---|
| `spec_version`, `id` | `SPEC_VERSION`, and the id from §1. |
| `name`, `description` | English fallbacks, not empty. The app shows `tool.<id>.name` and `tool.<id>.description` when a translation exists. |
| `category` | `Category::ENCODERS`, `FORMATTERS`, `GENERATORS` or `SYSTEM`. Any other `[a-z][a-z0-9_]*` id is accepted, but the app shows it as a group labelled with the raw id, after the built-in ones. Adding a category is a `host-change` PR. |
| `keywords` | Extra search terms, each non-empty and lower case. Search already matches the name, the id and the category label, so add other words, like uuid's `guid`. `registry_test.rs` checks that searching "uid" still ranks uuid first, so a name or keyword that contains "uid" can fail it. |
| `capabilities` | The host services the tool uses, such as `Capability::PROCESS_INSPECT`. Empty for a text tool. |
| `tray` | `true` lists the tool in the tray menu, labelled with `name`. |
| `icon` | `include_str!("icon.svg").into()`, checked as in §1. |
| `actions` | `ActionMeta::new(id, label)`, plus `.destructive()` when the shell must ask before running it. At least one; ids follow the id grammar and are unique; each must be reachable from `ui`. |
| `ui` | See below. |

### Choosing the UI

| `UiSpec` | Use it for | What you write |
|---|---|---|
| `Transform` | Input → outputs, with modes (encode/decode, format/minify) | Nothing in TypeScript |
| `Generator` | Options → output, no input (UUID) | Nothing in TypeScript |
| `Custom { view }` | Interactive tools the generic views can't express (the port inspector) | `ui/View.svelte` and `ui/i18n/en.ts` in the tool folder; `view` equals the id |

Prefer `Transform` or `Generator`. A custom view needs a reason in the PR.

The registry accepts and runs `Transform` tools today, and their tests pass, but the app has no `TransformView` until M3 ([roadmap](roadmap.md), M3 item 3). Until then it shows such a tool as needing a newer version of Navaja.

Besides `options` and `outputs` (below), the specs have these fields:

| Field | Meaning |
|---|---|
| `modes` (Transform) | Action ids, shown as modes; the first is the default. Every declared action must be a mode. |
| `input` (Transform) | `InputSpec { lang, placeholder }`: an editor language hint such as `json` (`None` for plain text) and the placeholder. The pasted text reaches `invoke` as a string under the key `input`. |
| `live` (Transform) | Re-run, debounced, as the input changes. A live mode can't be `destructive`. |
| `action` (Generator) | The tool's one action. It can't be `destructive`, because the generator view has no confirmation step. |
| `run_on_open` (Generator) | Run once with the default options when the tool opens. |

### Options

Each `OptionSpec` becomes one key of the input object:
- **`key`:** `[a-z][a-z0-9_]*`, unique, never `input` or `file` (reserved). It must equal the serde field name in your `Input` struct.
- **`label`:** the English fallback.
- **`control`:** one of the controls below.
- **`modes`:** for a `Transform`, the modes the option applies to; empty means all. Leave it empty for a `Generator`; the registry rejects anything else.

| `Control` | Value sent | The registry requires |
|---|---|---|
| `Toggle { default }` | bool | nothing more |
| `Choice { choices, default }` | the `value` of one `Choice::new(value, label)` | non-empty, unique values that include `default` |
| `Integer { min, max, default }` | an integer (`i64` in the spec) | `min <= default <= max`, all within ±(2^53-1) |
| `Text { default, limit }` | a string; `limit` is its maximum length in characters | `default` within `limit` |

- **What the view sends:** every option that applies to the current mode, starting at its default ([`options.ts`](../app/src/generic/options.ts)). Options for other modes are left out.
- **Nothing clamps:** the registry checks the spec, never the values, and the view applies `min`, `max` and `limit` through HTML form validation only. `invoke` re-checks any bound it relies on (§4).
- **Integer type:** deserialize into any integer type that holds `min..=max` (uuid uses `u32`). A number that doesn't fit the type fails `typed()` with `core.invalid_input`.
- **Defaults:** each default lives twice, in the `Control` (what the view sends) and in the serde default (used when the key is missing: options outside the current mode, and tests that send `{}`). Nothing checks that the two agree, so tie them together:
  - `Integer`: one const for both, like `DEFAULT_WIDTH` above.
  - `Choice`: deserialize into an enum with `#[serde(rename_all = "snake_case")]`, whose `#[default]` variant is the `Control`'s default, and give the field `#[serde(default)]`. uuid's `Version` does this: `V4` is the default variant, and `"v4"` the default choice.
  - `Toggle`: `#[serde(default)]` gives `false`. A default of `true` needs `#[serde(default = "...")]` with a function that returns `true`.
  - `Text`: `#[serde(default)]` gives an empty string. Any other default needs a const and `#[serde(default = "...")]`, as for an `Integer`.

### Writing a custom view

The shell loads `tools/<id>/ui/View.svelte` and passes it `ViewProps` from `$lib/view-kit`. A view may import only (architecture §4):
- its own files, through relative paths that stay inside `tools/<id>/ui/`;
- `svelte` and its browser subpaths, but not `svelte/internal`, `svelte/compiler` or `svelte/server`;
- `$lib/view-kit`, the shell's API for views: `t`, `runTool`, `copyText`, `CopyButton`, `ToolIcon`, `OUTPUT_ATTRIBUTE` and their types;
- generated types from `$bindings/<name>`.

Put `OUTPUT_ATTRIBUTE` (`data-output`) on every element that shows tool output, so the shell sends a native copy (Ctrl/Cmd+C or the context menu) whose selection touches it through Rust, like `copyText`, with the markers that keep it out of clipboard history (architecture §5, Clipboard). A copy from a text field stays native, so don't show output in one.

Its tests (`*.test.ts` and `*.spec.ts` files in `ui/`, which Vitest runs) may also import `vitest`, `@testing-library/svelte` and `@tauri-apps/api/mocks`. They import `vi` by that name (`import { vi } from 'vitest'`), use it only as `vi.<name>`, and call `vi.mock`, `vi.importActual` and Vitest's other calls that take a module directly, so the check can read the module each one takes.

`ui/` holds only scripts ESLint lints (`.js`, `.mjs`, `.cjs`, `.jsx`, `.ts`, `.mts`, `.cts`, `.tsx`, `.svelte`) and the assets `.css`, `.svg`, `.json`, `.png` and `.webp`, with lower-case extensions. It has no `node_modules` folder, and nothing in the tool folder is a symbolic link, a submodule, a `package.json` or a `tsconfig.json`. Paths keep the exact case of `tools/` and `ui/`. ESLint would never see any other file, but Vite would still bundle it, and Vite reads those two to resolve imports.

`pnpm check`, `pnpm lint` and `pnpm test` must pass (§5). `pnpm check` runs svelte-check over `ui/`, with warnings as errors. `pnpm lint` checks the files in `ui/`, these imports and the Prettier formatting of `ui/`, CSS and JSON included (`pnpm format` fixes the formatting). A comment can't turn the check off, and `import.meta.glob`, a computed `import()`, JSX or a `@jsxImportSource` comment is an error. If a view needs something else from the shell, add it to `$lib/view-kit` in a `host-change` PR first.

CSS is not checked: ESLint reads neither `<style>` blocks nor `.css` files. A view's CSS `@import` and `url()` must also stay inside `tools/<id>/ui/`, and so must Tailwind's `@reference`, `@plugin` and `@config`, which the app's Tailwind build reads. Review checks them. `@plugin` and `@config` load JavaScript and run it in Node at build time, so review reads that code too.

### Outputs

Each output in the spec has a `key` and a format. `invoke` returns one JSON object containing every declared key, with no extra keys. Use `null` when there is nothing to show. With debug assertions (the default for `cargo test`), the registry rejects any other shape with `core.invalid_output`; each value must round-trip exactly through its payload type.

| Format | Value |
|---|---|
| `Text`, `Code { lang }` | string |
| `KeyValue` | an array of `navaja_core::KeyValueRow`: `key` and `value` strings; `secret`, a bool, always present; `note`, a string, left out when there is none, never `null` |
| `Diagnostics` | an array of `navaja_core::Diagnostic`: `severity` (`"error"`, `"warning"` or `"info"`); `code`, one of your `<id>.*` codes; `message`, a string; `line` and `column` (1-based, the column in UTF-16 code units) and `details`, each always present, `null` when unknown |
| `Binary` | `{ len, hex }` (`BinaryValue::from_bytes`) |

- **Build these values from the types,** such as a `Vec<KeyValueRow>` passed to `serde_json::to_value`, rather than writing the JSON by hand. The registry checks that each value survives a round trip through its type unchanged, so a missing field, an extra one or `"note": null` fails.
- **`null`** stands for a whole output with nothing to show, never for one field of a row.
- **A wrong shape** fails with `core.invalid_output` and "output `<key>` does not match its format". The message names the output, not the field, and only debug builds check it.

## 4. Rules every tool follows

These are enforced by lints, cargo-deny, `registry_test.rs` and review:

- **Pure and synchronous.**
  - Never print, prompt, start an async runtime or open a connection.
  - Never take a file path; v1 text tools work on pasted text.
- **Errors carry stable codes:** `<id>.<snake_case>`, or `core.invalid_input` (from `typed()`) and `core.cancelled` (from `ctx.check()`). Diagnostics use `<id>.*` codes.
  - Write each code as an `ErrorCode::from_static("<id>.<name>")` literal, spelled exactly so, in a const like `WIDTH_OUT_OF_RANGE` above. `registry_test.rs` finds codes by searching your folder for that text; a code built any other way is checked only when it comes back through the registry in a debug build (`run_single` in a test, or `pnpm dev`).
  - Build the error with `ToolError::new(CODE, message)`, adding `.with_details(json!({ … }))` for structured facts such as ranges and positions.
  - The message is an English fallback that **never repeats the input**: no "invalid token `eyJ…`".
- **Parse input with `typed()`** on a `#[serde(deny_unknown_fields)]` struct. Never use maps keyed by user content.
- **Re-check option bounds in `invoke`.** An integer outside `min..=max` or a text longer than `limit` can still arrive. Return your own code with the bounds in `details`, as uuid does with `uuid.count_out_of_range` and `{ min, max }`.
- **Call `ctx.check()` before any side effect** and periodically in long loops (uuid checks every 1,024 items).
- **`registry_test.rs` probes every action:**
  - **Junk input:** it calls `invoke` directly with a cancelled `Ctx` and `{"__navaja_probe__": true}`, and expects `core.invalid_input` or `core.cancelled`. With `deny_unknown_fields`, `typed()` refuses the probe first, so checks that return `<id>.*` codes may come before `ctx.check()`, as in uuid.
  - **View inputs:** through the registry, it sends every applicable option at its default, plus `input: ""` for a `Transform`. The defaults may not fail with `core.invalid_input`, `core.invalid_output` or `core.panicked`, and a `Generator` without capabilities must succeed with them.
  - **Option values:** then, one at a time, each choice, both toggle values and both integer bounds. None may fail with `core.invalid_input`.
- **No secrets in logs.** Tools don't log. The shell logs only the tool id, action, duration and error code.
- **Dependencies** go in [`tools/Cargo.toml`](../tools/Cargo.toml), never in the root `Cargo.toml`: `tool-gate` rejects any change to it.
  - **Already there:** `navaja-core`, `serde`, `serde_json`, `uuid`.
  - **In the root `[workspace.dependencies]`:** write `<crate> = { workspace = true }`. Available: `proptest` (under `[dev-dependencies]`), `roxmltree`, `serde_path_to_error`, `ts-rs`. The `tauri` entries are for the app only. `tools/Cargo.toml` has no `[dev-dependencies]` table yet; the first tool that needs one adds it.
  - **`Cargo.lock`:** any change to `tools/Cargo.toml` changes it too. The first tool to use proptest adds about a dozen packages. Commit the lockfile with the tool (§5).
  - **Anything else:** write its version in `tools/Cargo.toml`, such as `<crate> = "1.2.3"`; cargo-deny rejects `*`. Moving it into the workspace table is a separate `host-change` PR.
  - **A new crate in a system-tool PR:** `crates/navaja-<x>/` is a workspace member already (`crates/*`). Write `navaja-<x> = { path = "../crates/navaja-<x>" }` in `tools/Cargo.toml`, with no version: `deny.toml` sets `allow-wildcard-paths`. The crate's own dependencies go in its own `Cargo.toml` under the same rule, `workspace = true` or a version. Of Navaja's crates, it may use only `navaja-core` ([architecture](architecture.md) §2).
  - **Allowed:** small, maintained crates with a licence on the `deny.toml` allowlist.
  - **Not allowed:** Tauri, tokio, HTTP clients and socket crates. `cargo xtask check` and cargo-deny reject them.

## 5. Test it

```sh
cargo fmt --all --check
rustfmt --edition 2024 --check tools/<id>/mod.rs   # cargo fmt skips tool modules
cargo test -p navaja-tools            # your tests + the registry checks
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask check                     # dependency rules
cargo deny --all-features check bans licenses sources   # if you added a dependency
pnpm check && pnpm lint && pnpm test  # custom views only: svelte-check, the import allowlist, Prettier, Vitest
```

`cargo fmt` never sees your tool's files: `tools/lib.rs` declares them inside `register_tools!`, and rustfmt does not expand macros. Running rustfmt on `mod.rs` also checks the files it declares, such as `tests.rs`; without `--check`, it formats them. CI runs the same check on every tool folder.

- **If you changed `tools/Cargo.toml`,** a dev-dependency such as proptest included: build once, so that Cargo updates `Cargo.lock`, commit the lockfile with the tool, and run the cargo-deny line above. Nothing else notices a lockfile left behind: `tool-gate` sees committed changes only, and CI does not build with `--locked`.
- **On Windows, keep the checkout path short,** such as `C:\src\Navaja`. With a long one, MSVC's linker failed with LNK1104 on a build script's path over 260 characters, even with long paths turned on in Windows.

Then commit, and check what your PR touches:

```sh
git fetch origin
cargo xtask tool-gate origin/main
```

- **Commit first.** `tool-gate` compares `<base>...HEAD`, so it sees committed changes only. Before your first commit it prints `tool-gate: no new tool in this diff` and exits 0. Look for `tool-gate: ok (text tool: <id>)`, or `tool-gate: ok (system tool: <id>)` when the PR also adds a crate or changes the bindings.
- **The base** is the branch your PR targets, fetched from the main repository; CI passes `origin/<target branch>`. In a fork, add the main repository as a remote, such as `upstream`, fetch it and pass `upstream/main`.
- **Bindings:** CI also runs `cargo xtask bindings --check`. The bindings come only from the crates listed in [`bindings.rs`](../xtask/src/bindings.rs), today `navaja-core` and the app crate, so a text tool leaves them unchanged. A system tool's crate gets bindings once `xtask bindings` exports it, which is a `host-change` PR; after that, the tool PR commits the regenerated `app/src/bindings/**`.

In tests, call your tool through `navaja_core::run_single(TOOL, action, input)`, not `invoke`, as [`tools/uuid/tests.rs`](../tools/uuid/tests.rs) does. It runs the output and error-code checks, returns `Result<Value, ToolError>`, and panics if the metadata is invalid. A `tests.rs` for the tool above:

```rust
use navaja_core::run_single;
use serde_json::json;

use super::TOOL;

#[test]
fn indents_every_line_and_bounds_the_width() {
    let out = run_single(TOOL, "indent", json!({ "input": "a\nb", "width": 4 })).unwrap();
    assert_eq!(out["text"], "    a\n    b");
    let error = run_single(TOOL, "indent", json!({ "input": "a", "width": 17 })).unwrap_err();
    assert_eq!(error.code.as_str(), "my_tool.width_out_of_range");
}

#[test]
fn keeps_line_endings_and_the_final_newline() {
    let out = run_single(TOOL, "indent", json!({ "input": "a\r\nb\n", "width": 1 })).unwrap();
    assert_eq!(out["text"], " a\r\n b\n");
    let out = run_single(TOOL, "dedent", json!({ "input": " \ta\r\n  b\n" })).unwrap();
    assert_eq!(out["text"], "a\r\nb\n");
}
```

Test the behaviour, not the plumbing:
- each mode on typical and edge input (empty, huge, non-ASCII, malformed);
- every error code;
- the property that matters, such as round trips (`decode(encode(x)) == x`) with proptest. Add `proptest = { workspace = true }` under `[dev-dependencies]` in `tools/Cargo.toml`.

To try the tool in the app, run `pnpm dev` from the repository root. Quit any other Navaja first, an installed one included. Navaja runs as a single instance, so a dev or debug build that finds another one running hands its arguments to it and exits with code 0, and you see the other app's window. End-to-end builds use their own identifier, so they are not affected.

## 6. What a tool PR may touch

| Change | May touch | Never touches |
|---|---|---|
| **Text tool** | `tools/<id>/**`, one line in `tools/lib.rs`, `tools/Cargo.toml`, `Cargo.lock` | anything else |
| **System tool** | The text-tool set, plus target-specific dependencies, an optional new `crates/navaja-<x>/`, generated `app/src/bindings/**` (once `xtask bindings` exports the tool's crate, a `host-change` PR), and `tools/<id>/ui/**` | `app/src/{shell,generic,lib}`, `app/src-tauri`, `navaja-core`, `xtask` |

If your tool needs something the host doesn't offer, open a separate `host-change` PR first. Examples are a new `OutputKind`, a new `Capability`, a new host service, or a new entry in the root `[workspace.dependencies]`.

Tauri commands and front-end code stay out of a tool PR.
