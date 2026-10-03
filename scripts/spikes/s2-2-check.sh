#!/usr/bin/env bash
# Spike S2.2 (docs/spikes.md): the checks in .github/workflows/spikes.yml that
# need more than one command, so that a local run does exactly what CI does.
# Each runs on the probe from s2-2-probe.sh, prints what it found, and exits
# non-zero when the check fails.
#
# Usage, from anywhere in the repository, after `pnpm install` and
# `bash scripts/spikes/s2-2-probe.sh create`:
#   bash scripts/spikes/s2-2-check.sh registry           # cargo test -p navaja-tools, the probe's own test included
#   bash scripts/spikes/s2-2-check.sh typecheck-refuses  # svelte-check fails on type errors (.ts and .svelte) and names them
#   bash scripts/spikes/s2-2-check.sh lint-refuses       # pnpm lint fails on forbidden imports (.ts and .svelte) and names them
#   bash scripts/spikes/s2-2-check.sh prettier-refuses   # pnpm lint fails in Prettier on a file ESLint accepts, and names it
#   bash scripts/spikes/s2-2-check.sh vitest             # pnpm test, with the probe's View.test.ts among the passes
#   bash scripts/spikes/s2-2-check.sh build              # pnpm build: the view in a chunk of its own, its class in the CSS
#   bash scripts/spikes/s2-2-check.sh packaged           # the probe's end-to-end spec, after the e2e build
#
# The *-refuses checks add their files to the probe and remove them again on
# exit, pass or fail. They print what they ran with a prefix: the output holds
# errors on purpose, and GitHub's problem matchers (setup-node registers
# ESLint's) would turn them into error annotations on a green run.
#
# `packaged` needs the end-to-end build first:
#   pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/e2e.conf.json
# On Linux it runs inside dbus-run-session and xvfb-run, as ci.yml does, but
# without the strace network guard, which runs the whole suite only.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# The probe view's one Tailwind class that nothing else in the repository
# uses (s2-2-probe.sh), as Tailwind writes its selector.
SELECTOR='.tracking-\[0\.4242em\]'
SPEC=app/e2e/spikes/s2-2-probe.e2e.ts
PROBE_UI=tools/probe/ui

fail() {
  echo "::error::S2.2 $*"
  exit 1
}

# A new temporary folder, as a path the OS's own programs understand (on
# Windows, not Git Bash's /tmp), with forward slashes, which both sides read.
native_tmp() {
  node -e "console.log(require('fs').mkdtempSync(require('path').join(require('os').tmpdir(), 'navaja-s2-2-')).replaceAll('\\\\', '/'))"
}

# One temporary folder per run, for every log and report, removed on exit.
# A mode that must undo something on exit registers it with on_exit instead
# of setting a trap of its own: a second `trap ... EXIT` would replace this
# one. The commands run last registered first, then the folder goes.
TMP=$(native_tmp)
CLEANUP=''
on_exit() {
  CLEANUP="$1"$'\n'"$CLEANUP"
}
cleanup() {
  eval "$CLEANUP" || true
  rm -rf "$TMP"
}
trap cleanup EXIT

# Prints a log that holds errors on purpose, out of the problem matchers' reach.
show() {
  sed 's/^/  | /' "$1"
}

# The path of `$1` in the OS's own form, as programs other than bash need it.
native_path() {
  node -p "require('path').resolve(process.argv[1])" "$1"
}

# Runs an end-to-end spec: inside a D-Bus session and a virtual display on
# Linux, as ci.yml does.
e2e() {
  if [ "$(uname -s)" = Linux ]; then
    WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- xvfb-run -a pnpm e2e --spec "$1"
  else
    pnpm e2e --spec "$1"
  fi
}

registry() {
  local log=$TMP/cargo-test.log
  cargo test -p navaja-tools 2>&1 | tee "$log"
  # registry_test.rs checks every registered tool; the probe's own test shows
  # it is one of them. The test passed, or cargo test would have failed; the
  # result after the name may be coloured.
  grep -qF 'test probe::tests::answers_with_a_fixed_value ... ' "$log" ||
    fail "registry: the probe's own test did not run"
  echo "The registry accepts the probe, and its tests pass."
}

# The negative control for `pnpm check`: without it, a clean svelte-check
# would read the same whether or not tsconfig.json's tools/*/ui includes
# still matched the view folder.
typecheck_refuses() {
  local log=$TMP/svelte-check.log status=0 errors
  on_exit 'bash scripts/spikes/s2-2-probe.sh untypecheck'
  bash scripts/spikes/s2-2-probe.sh typecheck
  # svelte-check alone, as `pnpm check` runs it first, for its exit status
  # and its machine-readable output: one ERROR line per error, naming the
  # file relative to app/ (with back slashes on Windows).
  (cd app && FORCE_COLOR=0 pnpm exec svelte-check --tsconfig ./tsconfig.json --output machine) \
    > "$log" 2>&1 || status=$?
  show "$log"
  [ "$status" -ne 0 ] || fail "typecheck: svelte-check passed with type errors in $PROBE_UI/"
  grep -qE ' ERROR "[^"]*[/\\]typed\.ts" ' "$log" ||
    fail "typecheck: no ERROR line names typed.ts"
  grep -qE ' ERROR "[^"]*[/\\]Typed\.svelte" ' "$log" ||
    fail "typecheck: no ERROR line names Typed.svelte"
  grep -qF 'not assignable' "$log" || fail "typecheck: no 'not assignable' error"
  errors=$(grep -E ' COMPLETED ' "$log" | sed -E 's/.* ([0-9]+) ERRORS .*/\1/' || true)
  [ "$errors" = 2 ] ||
    fail "typecheck: expected exactly two errors, the planted ones; svelte-check counted '${errors}'"
  echo "svelte-check failed (exit $status) on both planted type errors, and named them."
}

lint_refuses() {
  local log=$TMP/lint.log status=0
  on_exit 'bash scripts/spikes/s2-2-probe.sh unforbid'
  bash scripts/spikes/s2-2-probe.sh forbid
  # No colour codes in the output the checks below read: on GitHub Actions,
  # and always on Windows, ESLint's formatter and Prettier colour it
  # otherwise. Both variables: ESLint's colour library turns off with
  # FORCE_COLOR=0, Prettier's only with NO_COLOR (FORCE_COLOR=0 turns it on,
  # as any value does).
  FORCE_COLOR=0 NO_COLOR=1 pnpm lint > "$log" 2>&1 || status=$?
  show "$log"
  [ "$status" -ne 0 ] || fail "lint: pnpm lint passed with forbidden imports in $PROBE_UI/"
  grep -q 'forbidden\.ts' "$log" || fail "lint: the output does not name forbidden.ts"
  grep -q 'Forbidden\.svelte' "$log" || fail "lint: the output does not name Forbidden.svelte"
  grep -qF "'\$lib/ipc' is not allowed here" "$log" || fail "lint: the output does not name '\$lib/ipc'"
  grep -qF 'navaja/view-imports' "$log" || fail "lint: the error is not from navaja/view-imports"
  grep -qF '2 problems (2 errors, 0 warnings)' "$log" ||
    fail "lint: expected exactly two problems, the forbidden imports"
  echo "pnpm lint failed (exit $status) on both forbidden imports, and named them."
}

# The negative control for the Prettier half of `pnpm lint`, which runs only
# once ESLint has passed.
prettier_refuses() {
  local log=$TMP/prettier.log status=0 flagged
  on_exit 'bash scripts/spikes/s2-2-probe.sh unmisformat'
  bash scripts/spikes/s2-2-probe.sh misformat
  # Without colour codes, as in lint_refuses.
  FORCE_COLOR=0 NO_COLOR=1 pnpm lint > "$log" 2>&1 || status=$?
  show "$log"
  [ "$status" -ne 0 ] || fail "prettier: pnpm lint passed with a misformatted file in $PROBE_UI/"
  grep -qF 'Checking formatting' "$log" ||
    fail "prettier: Prettier never ran; the view-file check or ESLint failed first"
  grep -qE '^\[warn\] tools[/\\]probe[/\\]ui[/\\]misformatted\.ts$' "$log" ||
    fail "prettier: no [warn] line names misformatted.ts"
  grep -qF 'Code style issues found' "$log" || fail "prettier: no 'Code style issues found'"
  # Prettier lists each file it refuses on a [warn] line of its own, a path
  # with no spaces; the summary line has spaces.
  flagged=$(grep -cE '^\[warn\] [^ ]+$' "$log" || true)
  [ "$flagged" = 1 ] || fail "prettier: expected one refused file, misformatted.ts; Prettier listed $flagged"
  echo "pnpm lint failed (exit $status) in Prettier on the misformatted file only, and named it."
}

vitest() {
  local report=$TMP/vitest.json
  pnpm test --reporter=default --reporter=json "--outputFile.json=$report"
  node - "$report" <<'EOF'
const report = JSON.parse(require('fs').readFileSync(process.argv[2], 'utf8'));
const file = report.testResults.find((result) =>
  result.name.replaceAll('\\', '/').endsWith('/tools/probe/ui/View.test.ts'),
);
if (!file) {
  console.log('::error::S2.2 vitest: tools/probe/ui/View.test.ts did not run');
  process.exit(1);
}
const passed = file.assertionResults.filter((test) => test.status === 'passed');
if (file.status !== 'passed' || passed.length === 0) {
  console.log(`::error::S2.2 vitest: tools/probe/ui/View.test.ts: ${file.status}`);
  process.exit(1);
}
console.log(
  `Vitest ran tools/probe/ui/View.test.ts: ${passed.length} passed ` +
    `(${report.numPassedTests} of ${report.numTotalTests} tests in ${report.testResults.length} files).`,
);
EOF
}

build() {
  local dist=app/dist entry chunk chunks css
  pnpm build
  # The entry script, as `assets/<name>.js` whether the src is absolute or
  # starts with `./` (Vite's `base: './'`).
  entry=$(grep -oE '<script type="module"[^>]* src="\.?/assets/[^"]*\.js"' "$dist/index.html" |
    sed -E 's|.* src="\.?/||; s|"$||' || true)
  [ -n "$entry" ] || fail "build: no entry script in $dist/index.html"
  chunks=$(grep -l 'probe-view' "$dist"/assets/*.js || true)
  [ -n "$chunks" ] || fail "build: no chunk holds the probe view"
  for chunk in $chunks; do
    [ "$chunk" != "$dist/$entry" ] || fail "build: the probe view is in the entry chunk, $entry"
    echo "The probe view is in its own chunk: $chunk ($(wc -c < "$chunk" | tr -d " ") bytes; entry: $entry)."
  done
  css=$(grep -lF "$SELECTOR" "$dist"/assets/*.css || true)
  [ -n "$css" ] || fail "build: the view's Tailwind class, $SELECTOR, is not in the emitted CSS"
  echo "The view's Tailwind class is in the emitted CSS: $css."
}

packaged() {
  [ -f "$SPEC" ] || fail "packaged: no $SPEC; run s2-2-probe.sh create first"
  # wdio looks for a relative --spec from the working directory but resolves
  # it from the config's folder, so the path is absolute, in the OS's own form.
  e2e "$(native_path "$SPEC")"
}

case "${1:-}" in
  registry) registry ;;
  typecheck-refuses) typecheck_refuses ;;
  lint-refuses) lint_refuses ;;
  prettier-refuses) prettier_refuses ;;
  vitest) vitest ;;
  build) build ;;
  packaged) packaged ;;
  *)
    echo "usage: $0 registry|typecheck-refuses|lint-refuses|prettier-refuses|vitest|build|packaged" >&2
    exit 2
    ;;
esac
