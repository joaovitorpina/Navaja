#!/usr/bin/env bash
# M2a exit check "Copy stays out of Windows clipboard history" (docs/spikes.md),
# job exit-clipboard-history in .github/workflows/spikes.yml. Navaja copies
# through arboard with Windows' exclusion formats (app/src-tauri/src/clipboard.rs);
# this checks, on a real Windows clipboard, that such a copy reaches the
# clipboard and not its history, against controls that show the history
# records an ordinary copy.
#
# Usage, on Windows, from anywhere in the repository, after the end-to-end build
#   pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/e2e.conf.json
# and `clipboard-history.ps1 enable`:
#   bash scripts/spikes/exit-clipboard-check.sh control  # an ordinary copy of a random text reaches the history
#   bash scripts/spikes/exit-clipboard-check.sh copy     # Navaja's Copy button copies a fresh UUID (the canary)
#   bash scripts/spikes/exit-clipboard-check.sh absent   # the canary is on the clipboard but not in the history
#   bash scripts/spikes/exit-clipboard-check.sh detects  # the canary, copied the ordinary way, does reach it
#   bash scripts/spikes/exit-clipboard-check.sh clean    # removes the spec `copy` writes
#
# The modes share a folder, $EXIT_CLIPBOARD_DIR (default:
# navaja-exit-clipboard under $RUNNER_TEMP, or under the OS's temporary
# folder outside CI), so that each can run as its own workflow step.
#
# `copy` writes app/e2e/spikes/exit-clipboard.e2e.ts and runs it alone with
# --spec; `pnpm e2e` runs only app/e2e/specs/, so it never runs otherwise.
# The spec opens the UUID tool, reads the UUID it generated, presses the
# output's Copy button (which calls copy_text) and saves the UUID for the
# modes after it.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

SPEC_DIR=app/e2e/spikes
SPEC=$SPEC_DIR/exit-clipboard.e2e.ts
HELPER=scripts/spikes/clipboard-history.ps1
# How long the history gets to take an ordinary copy in, and how long the
# canary gets to show up in it before its absence counts.
WAIT=${EXIT_CLIPBOARD_WAIT:-10}

fail() {
  echo "::error::exit-clipboard $*"
  exit 1
}

case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) ;;
  *) fail "this check runs on Windows" ;;
esac

# The path of `$1` in the OS's own form, as programs other than bash need it.
native_path() {
  node -p "require('path').resolve(process.argv[1])" "$1"
}

# With forward slashes, which bash, Node and PowerShell all read.
DIR=${EXIT_CLIPBOARD_DIR:-$(node -p "require('path').join(process.env.RUNNER_TEMP || require('os').tmpdir(), 'navaja-exit-clipboard').replaceAll('\\\\', '/')")}
mkdir -p "$DIR"
CANARY_FILE=$DIR/canary.txt

ps() {
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$(native_path "$HELPER")" "$@"
}

canary() {
  [ -s "$CANARY_FILE" ] || fail "no canary; run '$0 copy' first"
  cat "$CANARY_FILE"
}

control() {
  local text
  text="navaja-control-$(node -p "require('crypto').randomUUID()")"
  ps put -Text "$text"
  ps history-has -Text "$text" -Seconds "$WAIT"
  echo "Clipboard history records an ordinary copy."
}

copy() {
  rm -f "$CANARY_FILE"
  mkdir -p "$SPEC_DIR"
  cat > "$SPEC" <<'EOF'
// M2a exit check "Copy stays out of Windows clipboard history"
// (docs/spikes.md). scripts/spikes/exit-clipboard-check.sh writes this file
// outside specs/, so `pnpm e2e` skips it; the script runs it alone with
// --spec, then looks for the copied UUID on the clipboard and in its history.
import { writeFileSync } from 'node:fs';
import { $, browser, expect } from '@wdio/globals';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

describe('exit: clipboard history', () => {
  it("copies a fresh UUID with the output's Copy button", async () => {
    const file = process.env.EXIT_CLIPBOARD_CANARY;
    if (!file) throw new Error('EXIT_CLIPBOARD_CANARY names no file');

    await browser.execute(() => {
      window.location.hash = '#/tool/uuid';
    });
    await expect($('h1')).toHaveText('UUID generator');
    const output = $('section[aria-label="UUIDs"] pre');
    await browser.waitUntil(async () => UUID.test(await output.getText()), {
      timeoutMsg: 'no UUID appeared',
    });
    const canary = await output.getText();

    // The output's own Copy button, which copies through copy_text.
    const button = $('section[aria-label="UUIDs"] button');
    await expect(button).toHaveText('Copy');
    await button.click();
    await expect(button).toHaveText('Copied');

    writeFileSync(file, canary);
    console.log(`exit-clipboard: copied ${canary} with the Copy button.`);
  });
});
EOF
  EXIT_CLIPBOARD_CANARY=$(native_path "$CANARY_FILE") pnpm e2e --spec "$(native_path "$SPEC")"
  [ -s "$CANARY_FILE" ] || fail "copy: the spec saved no canary"
  echo "Navaja copied the canary: $(canary)."
  ps clipboard-is -Text "$(canary)"
}

absent() {
  ps clipboard-is -Text "$(canary)"
  ps history-lacks -Text "$(canary)" -Seconds "$WAIT"
  echo "Navaja's copy stayed out of clipboard history."
}

# The negative control for `absent`: the same text, copied the ordinary
# way, must reach the history, so the lookup would have seen it.
detects() {
  ps put -Text "$(canary)"
  ps history-has -Text "$(canary)" -Seconds "$WAIT"
  echo "The same text, copied the ordinary way, reaches clipboard history."
}

clean() {
  rm -f "$SPEC"
  # The folder only if this spec was all it held.
  rmdir "$SPEC_DIR" 2> /dev/null || true
  echo "Spec removed."
}

case "${1:-}" in
  control) control ;;
  copy) copy ;;
  absent) absent ;;
  detects) detects ;;
  clean) clean ;;
  *)
    echo "usage: $0 control|copy|absent|detects|clean" >&2
    exit 2
    ;;
esac
