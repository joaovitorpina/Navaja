#!/usr/bin/env bash
# Spike S2.2 (docs/spikes.md): writes a throwaway tool with a custom view,
# `probe`, into the checkout, so that .github/workflows/spikes.yml can check
# that a view in tools/<id>/ui/ is type-checked, linted, tested, built,
# hot-reloaded and packaged like code in app/src. It never commits anything.
#
# Usage, from anywhere in the repository:
#   bash scripts/spikes/s2-2-probe.sh create    # the probe, its line in tools/lib.rs, its end-to-end spec
#   bash scripts/spikes/s2-2-probe.sh forbid    # adds a file with an import the allowlist refuses
#   bash scripts/spikes/s2-2-probe.sh unforbid  # removes that file again
#   bash scripts/spikes/s2-2-probe.sh remove    # removes everything the modes above wrote
#
# The probe is a system tool with a custom view, laid out as
# docs/adding-a-tool.md describes:
# - tools/probe/mod.rs, icon.svg and tests.rs: UiSpec::Custom and one action,
#   `answer`, that returns a fixed value;
# - tools/probe/ui/View.svelte: imports only what the allowlist permits
#   ($lib/view-kit, a $bindings type and its sibling label.ts), shows a marker
#   and, on mount, runs `answer` through runTool and shows the result;
# - tools/probe/ui/label.ts, i18n/en.ts and View.test.ts (Vitest, mockIPC);
# - `probe,` in register_tools! (tools/lib.rs), in sorted order.
# The view uses one Tailwind class that nothing else in the repository uses,
# tracking-[0.4242em], so the build check can look for it in the emitted CSS.
#
# The end-to-end spec goes to app/e2e/spikes/. `pnpm e2e` runs only
# app/e2e/specs/, so the spec runs only when named with --spec.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

LIB=tools/lib.rs
LINE='    probe,'
PROBE=tools/probe
SPEC_DIR=app/e2e/spikes
SPEC="$SPEC_DIR/s2-2-probe.e2e.ts"
FORBIDDEN="$PROBE/ui/forbidden.ts"

fail() {
  echo "::error::s2-2-probe: $*"
  exit 1
}

# Adds the probe's line to register_tools! and sorts the list byte by byte,
# as registry_test.rs expects.
register() {
  grep -qxF "$LINE" "$LIB" && return 0
  local start end
  start=$(grep -n '^register_tools! {$' "$LIB" | cut -d: -f1)
  [ -n "$start" ] || fail "no 'register_tools! {' line in $LIB"
  end=$(awk -v start="$start" 'NR > start && $0 == "}" { print NR; exit }' "$LIB")
  [ -n "$end" ] || fail "register_tools! in $LIB has no closing brace"
  {
    head -n "$start" "$LIB"
    { sed -n "$((start + 1)),$((end - 1))p" "$LIB"; printf '%s\n' "$LINE"; } | LC_ALL=C sort
    tail -n "+$end" "$LIB"
  } > "$LIB.s2-2"
  mv "$LIB.s2-2" "$LIB"
}

unregister() {
  grep -qxF "$LINE" "$LIB" || return 0
  grep -vxF "$LINE" "$LIB" > "$LIB.s2-2" || true
  mv "$LIB.s2-2" "$LIB"
}

create() {
  mkdir -p "$PROBE/ui/i18n" "$SPEC_DIR"

  cat > "$PROBE/mod.rs" <<'EOF'
//! Probe: spike S2.2's throwaway system tool with a custom view
//! (docs/spikes.md). scripts/spikes/s2-2-probe.sh writes this folder and
//! removes it again; it is never committed.

use navaja_core::{
    ActionMeta, Category, Ctx, SPEC_VERSION, Tool, ToolError, ToolId, ToolMeta, UiSpec, Value,
    typed,
};
use serde::Deserialize;
use serde_json::json;

pub(crate) const TOOL: Probe = Probe;

pub(crate) struct Probe;

/// The one action's fixed answer. The end-to-end spec looks for it in the
/// view, where only a run through Rust can put it.
pub(crate) const ANSWER: &str = "Answer from Rust";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {}

impl Tool for Probe {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            spec_version: SPEC_VERSION,
            id: ToolId::from_static("probe"),
            name: "Probe".into(),
            description: "Spike S2.2's throwaway custom view.".into(),
            category: Category::SYSTEM,
            keywords: vec!["spike".into()],
            icon: include_str!("icon.svg").into(),
            capabilities: vec![],
            actions: vec![ActionMeta::new("answer", "Answer")],
            tray: false,
            ui: UiSpec::Custom {
                view: "probe".into(),
            },
        }
    }

    fn invoke(&self, _action: &str, input: Value, ctx: &Ctx<'_>) -> Result<Value, ToolError> {
        let Input {} = typed(input)?;
        ctx.check()?;
        Ok(json!({ "answer": ANSWER }))
    }
}

#[cfg(test)]
mod tests;
EOF

  cat > "$PROBE/tests.rs" <<'EOF'
use navaja_core::run_single;
use serde_json::json;

use super::{ANSWER, TOOL};

#[test]
fn answers_with_a_fixed_value() {
    let out = run_single(TOOL, "answer", json!({})).unwrap();
    assert_eq!(out, json!({ "answer": ANSWER }));
}
EOF

  cat > "$PROBE/icon.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="11" cy="11" r="6"/>
  <path d="M15.5 15.5L20 20"/>
</svg>
EOF

  cat > "$PROBE/ui/View.svelte" <<'EOF'
<script lang="ts">
  // Spike S2.2's throwaway view (scripts/spikes/s2-2-probe.sh). It imports
  // only what the allowlist permits and, on mount, runs its own action
  // through runTool, so the packaged app shows a value that came from Rust.
  import { onMount } from 'svelte';
  import type { JsonValue } from '$bindings/serde_json/JsonValue';
  import {
    CopyButton,
    OUTPUT_ATTRIBUTE,
    runTool,
    t,
    type ToolError,
    type ViewProps,
  } from '$lib/view-kit';
  import { LABEL } from './label';

  let { meta }: ViewProps = $props();

  let answer = $state<string | null>(null);
  let failure = $state.raw<ToolError | null>(null);

  /** The `answer` field of the action's result, if it has one. */
  function answerOf(value: JsonValue): string | null {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
    const field = value.answer;
    return typeof field === 'string' ? field : null;
  }

  onMount(() => {
    void runTool(meta.id, 'answer', {}).result.then((result) => {
      if (result.ok) answer = answerOf(result.value);
      else failure = result.error;
    });
  });
</script>

<section data-testid="probe-view" class="flex flex-col gap-3 tracking-[0.4242em]">
  <p data-testid="probe-label">{t('tool.probe.label', LABEL)}</p>
  {#if answer !== null}
    <p data-testid="probe-answer" {...{ [OUTPUT_ATTRIBUTE]: '' }}>{answer}</p>
    <div><CopyButton text={answer} /></div>
  {:else if failure !== null}
    <p class="text-sm text-danger">{t(`error.${failure.code}`, failure.message)}</p>
  {/if}
</section>
EOF

  cat > "$PROBE/ui/label.ts" <<'EOF'
// The view's marker text. A sibling module, so that the build, the tests and
// hot reload all have one of the view's own imports to follow.
export const LABEL = 'Probe view from tools/probe/ui';
EOF

  cat > "$PROBE/ui/i18n/en.ts" <<'EOF'
// The view's English strings (docs/adding-a-tool.md, "Writing a custom view").
import { LABEL } from '../label';

export default {
  'tool.probe.label': LABEL,
} satisfies Record<string, string>;
EOF

  cat > "$PROBE/ui/View.test.ts" <<'EOF'
import { render, screen } from '@testing-library/svelte';
import { mockIPC } from '@tauri-apps/api/mocks';
import { describe, expect, it } from 'vitest';
import type { RunEnvelope } from '$bindings/RunEnvelope';
import type { ToolMeta } from '$bindings/ToolMeta';
import { LABEL } from './label';
import View from './View.svelte';

const meta = {
  specVersion: 1,
  id: 'probe',
  name: 'Probe',
  description: "Spike S2.2's throwaway custom view.",
  category: 'system',
  keywords: ['spike'],
  icon: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"></svg>',
  capabilities: [],
  actions: [{ id: 'answer', label: 'Answer', destructive: false }],
  tray: false,
  ui: { kind: 'custom', view: 'probe' },
} satisfies ToolMeta;

describe('the probe view', () => {
  it('shows its marker and the answer its action returns over IPC', async () => {
    const runs: unknown[] = [];
    mockIPC((cmd, payload) => {
      if (cmd !== 'run_tool') return null;
      runs.push(payload);
      return { ok: { answer: 'Mocked answer' } } satisfies RunEnvelope;
    });

    render(View, { meta });

    expect(screen.getByTestId('probe-label').textContent).toBe(LABEL);
    expect((await screen.findByTestId('probe-answer')).textContent).toBe('Mocked answer');
    expect(runs).toEqual([expect.objectContaining({ tool: 'probe', action: 'answer', input: {} })]);
  });
});
EOF

  cat > "$SPEC" <<'EOF'
// Spike S2.2 (docs/spikes.md): the probe's custom view in the packaged app.
// scripts/spikes/s2-2-probe.sh writes this file outside specs/, so `pnpm e2e`
// skips it; .github/workflows/spikes.yml runs it alone with --spec.
import { $, browser, expect } from '@wdio/globals';

describe('S2.2 probe', () => {
  it('opens the view from tools/probe/ui and shows the answer from Rust', async () => {
    await browser.execute(() => {
      window.location.hash = '#/tool/probe';
    });
    await expect($('h1')).toHaveText('Probe');

    const view = $('[data-testid="probe-view"]');
    await view.waitForDisplayed();
    await expect($('[data-testid="probe-label"]')).toHaveText('Probe view from tools/probe/ui');
    // Only the view's own run_tool call, answered by Rust, fills this in.
    await expect($('[data-testid="probe-answer"]')).toHaveText('Answer from Rust');

    // The view's own Tailwind class reached the app's CSS.
    const spacing = await view.getCSSProperty('letter-spacing');
    expect(Number.parseFloat(String(spacing.value))).toBeGreaterThan(0);
  });
});
EOF

  register
  echo "Probe written: $PROBE/, $SPEC, and 'probe,' in $LIB."
}

forbid() {
  [ -d "$PROBE/ui" ] || fail "no probe; run '$0 create' first"
  cat > "$FORBIDDEN" <<'EOF'
// Spike S2.2's negative lint check (scripts/spikes/s2-2-probe.sh forbid). A
// view reaches the shell only through $lib/view-kit, so `pnpm lint` must
// refuse this import and name it.
import { listTools } from '$lib/ipc';

export const forbidden = listTools;
EOF
  echo "Forbidden import written: $FORBIDDEN."
}

unforbid() {
  rm -f "$FORBIDDEN"
  echo "Forbidden import removed."
}

remove() {
  rm -rf "$PROBE" "$SPEC"
  # The folder only if the probe's spec was all it held.
  rmdir "$SPEC_DIR" 2> /dev/null || true
  unregister
  echo "Probe removed."
}

case "${1:-}" in
  create) create ;;
  forbid) forbid ;;
  unforbid) unforbid ;;
  remove) remove ;;
  *)
    echo "usage: $0 create|forbid|unforbid|remove" >&2
    exit 2
    ;;
esac
