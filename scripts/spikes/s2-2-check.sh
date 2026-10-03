#!/usr/bin/env bash
# Spike S2.2 (docs/spikes.md): the checks in .github/workflows/spikes.yml that
# need more than one command, so that a local run does exactly what CI does.
# Each runs on the probe from s2-2-probe.sh, prints what it found, and exits
# non-zero when the check fails.
#
# Usage, from anywhere in the repository, after `pnpm install` and
# `bash scripts/spikes/s2-2-probe.sh create`:
#   bash scripts/spikes/s2-2-check.sh registry      # cargo test -p navaja-tools, the probe's own test included
#   bash scripts/spikes/s2-2-check.sh lint-refuses  # pnpm lint fails on forbidden imports (.ts and .svelte) and names them
#   bash scripts/spikes/s2-2-check.sh vitest        # pnpm test, with the probe's View.test.ts among the passes
#   bash scripts/spikes/s2-2-check.sh build         # pnpm build: the view in a chunk of its own, its class in the CSS
#   bash scripts/spikes/s2-2-check.sh packaged      # the probe's end-to-end spec, after the e2e build
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

fail() {
  echo "::error::S2.2 $*"
  exit 1
}

# A new temporary folder, as a path the OS's own programs understand (on
# Windows, not Git Bash's /tmp).
native_tmp() {
  node -e "console.log(require('fs').mkdtempSync(require('path').join(require('os').tmpdir(), 'navaja-s2-2-')))"
}

registry() {
  local log
  log="$(native_tmp)/cargo-test.log"
  cargo test -p navaja-tools 2>&1 | tee "$log"
  # registry_test.rs checks every registered tool; the probe's own test shows
  # it is one of them. The test passed, or cargo test would have failed; the
  # result after the name may be coloured.
  grep -qF 'test probe::tests::answers_with_a_fixed_value ... ' "$log" ||
    fail "registry: the probe's own test did not run"
  echo "The registry accepts the probe, and its tests pass."
}

lint_refuses() {
  local log status=0
  log="$(native_tmp)/lint.log"
  bash scripts/spikes/s2-2-probe.sh forbid
  trap 'bash scripts/spikes/s2-2-probe.sh unforbid' EXIT
  # No colour codes in the output the checks below read; on GitHub Actions
  # ESLint's formatter colours it otherwise.
  FORCE_COLOR=0 pnpm lint > "$log" 2>&1 || status=$?
  cat "$log"
  [ "$status" -ne 0 ] || fail "lint: pnpm lint passed with forbidden imports in tools/probe/ui/"
  grep -q 'forbidden\.ts' "$log" || fail "lint: the output does not name forbidden.ts"
  grep -q 'Forbidden\.svelte' "$log" || fail "lint: the output does not name Forbidden.svelte"
  grep -qF "'\$lib/ipc' is not allowed here" "$log" || fail "lint: the output does not name '\$lib/ipc'"
  grep -qF 'navaja/view-imports' "$log" || fail "lint: the error is not from navaja/view-imports"
  grep -qF '2 problems (2 errors, 0 warnings)' "$log" ||
    fail "lint: expected exactly two problems, the forbidden imports"
  echo "pnpm lint failed (exit $status) on both forbidden imports, and named them."
}

vitest() {
  local report
  report="$(native_tmp)/vitest.json"
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
  entry=$(grep -o '<script type="module"[^>]* src="/assets/[^"]*\.js"' "$dist/index.html" |
    sed 's|.* src="/||; s|"$||')
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
  local spec
  [ -f "$SPEC" ] || fail "packaged: no $SPEC; run s2-2-probe.sh create first"
  # wdio looks for a relative --spec from the working directory but resolves
  # it from the config's folder, so the path is absolute, in the OS's own form.
  spec=$(node -p "require('path').resolve(process.argv[1])" "$SPEC")
  if [ "$(uname -s)" = Linux ]; then
    WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- xvfb-run -a pnpm e2e --spec "$spec"
  else
    pnpm e2e --spec "$spec"
  fi
}

case "${1:-}" in
  registry) registry ;;
  lint-refuses) lint_refuses ;;
  vitest) vitest ;;
  build) build ;;
  packaged) packaged ;;
  *)
    echo "usage: $0 registry|lint-refuses|vitest|build|packaged" >&2
    exit 2
    ;;
esac
