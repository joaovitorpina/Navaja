#!/usr/bin/env bash
# Tests scripts/advisories/issue.sh against a stub gh that records its calls
# and answers `issue list` and `label list` from files, so no issue is ever
# opened. advisories.yml runs it on a pull request that changes the workflow
# or this folder; it also runs locally.
#
# Each case sets up the open issues and the labels the stub reports, runs
# issue.sh, and checks which writes it made: one comment on the open issue,
# or one new issue (and the label when it is missing), never both, and
# nothing at all in a dry run.
#
# Exit status: 0 when every case passes; 1 otherwise.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

script=scripts/advisories/issue.sh
# The values issue.sh must use. A change there that leaves an open issue
# behind fails here first.
title='cargo deny check advisories is failing'
label=advisories

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"

# One line per call, each argument quoted (printf %q), so a test can match
# the arguments without the multi-line body getting in the way.
cat >"$work/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%q ' "$@" >>"$STUB_DIR/calls"
echo >>"$STUB_DIR/calls"
case "$1 $2" in
  'issue list') cat "$STUB_DIR/issues.tsv" ;;
  'label list') cat "$STUB_DIR/labels.txt" ;;
  'issue comment' | 'issue create' | 'label create') ;;
  *)
    echo "stub gh: unexpected call: $*" >&2
    exit 1
    ;;
esac
EOF
chmod +x "$work/bin/gh"

failures=0
case_name=

fail() {
  echo "  FAIL: $*"
  failures=$((failures + 1))
}

# run_case NAME ISSUES LABELS [VAR=value...]: ISSUES is the stub's
# `issue list` answer (number<TAB>title per line), LABELS its label names.
run_case() {
  case_name=$1
  printf '%b' "$2" >"$work/issues.tsv"
  printf '%b' "$3" >"$work/labels.txt"
  shift 3
  : >"$work/calls"
  echo "$case_name"
  if ! env PATH="$work/bin:$PATH" STUB_DIR="$work" GH_REPO=example/repo \
    RUN_URL=https://example.invalid/runs/42 GITHUB_REF_NAME=main "$@" \
    bash "$script" >"$work/out" 2>&1; then
    fail "issue.sh exited non-zero:"
    sed 's/^/    /' "$work/out"
  fi
}

# expect_calls PATTERN COUNT: COUNT recorded calls match the extended regex.
expect_calls() {
  local got
  got=$(grep -cE -- "$1" "$work/calls" || true)
  if [ "$got" != "$2" ]; then
    fail "expected $2 call(s) matching '$1', got $got. Calls:"
    sed 's/^/    /' "$work/calls"
  fi
}

expect_output() {
  if ! grep -qF -- "$1" "$work/out"; then
    fail "expected output containing '$1', got:"
    sed 's/^/    /' "$work/out"
  fi
}

# The title as the stub records it (printf %q escapes its spaces), with each
# backslash doubled for grep -E.
q_title=$(printf '%q' "$title" | sed 's/\\/\\\\/g')
url='https://example.invalid/runs/42'
# "on main: " before the run's link, as the stub records it: printf %q
# escapes each space in a one-line argument, but not in a multi-line one.
on_main='on(\\ | )main:(\\ | )'

run_case 'No open issue, no label: creates the label, then the issue' '' 'bug\n'
expect_calls "^issue list .*--state open --label $label " 1
expect_calls "^label create $label " 1
expect_calls "^issue create .*--title $q_title --label $label --body .*$on_main$url" 1
expect_calls '^issue comment ' 0

run_case 'No open issue, label exists: creates the issue only' '' "bug\n$label\n"
expect_calls '^label create ' 0
expect_calls "^issue create .*--title $q_title --label $label " 1
expect_calls '^issue comment ' 0

run_case 'Open issue with the label but another title: creates a new issue' \
  "7\tRUSTSEC-2026-0001 triage\n" "$label\n"
expect_calls '^issue create ' 1
expect_calls '^issue comment ' 0

run_case 'Open issue with the title: comments on it, creates nothing' \
  "12\t$title\n" "$label\n"
expect_calls "^issue comment 12 .*$on_main$url" 1
expect_calls '^issue create ' 0
expect_calls '^label ' 0

run_case 'Two open issues with the title: comments on the newest (listed first)' \
  "15\t$title\n12\t$title\n" "$label\n"
expect_calls '^issue comment 15 ' 1
expect_calls '^issue comment 12 ' 0
expect_calls '^issue create ' 0

run_case 'Dry run, open issue: looks it up, writes nothing' \
  "12\t$title\n" "$label\n" DRY_RUN=1
expect_calls '^issue list ' 1
expect_calls '^(issue (comment|create)|label) ' 0
expect_output 'would comment on #12'

run_case 'Dry run, no open issue: looks it up, writes nothing' '' '' DRY_RUN=1
expect_calls '^issue list ' 1
expect_calls '^(issue (comment|create)|label) ' 0
expect_output 'would open one'

# The guards: an issue without the run's link is no use, and one that does
# not name the branch could read as main failing when it is not.
echo 'RUN_URL missing: fails before calling gh'
: >"$work/calls"
if env PATH="$work/bin:$PATH" STUB_DIR="$work" GH_REPO=example/repo RUN_URL= \
  GITHUB_REF_NAME=main bash "$script" >"$work/out" 2>&1; then
  fail 'issue.sh passed without RUN_URL'
fi
expect_calls '.' 0

echo 'GITHUB_REF_NAME missing: fails before calling gh'
: >"$work/calls"
if env PATH="$work/bin:$PATH" STUB_DIR="$work" GH_REPO=example/repo \
  RUN_URL=https://example.invalid/runs/42 GITHUB_REF_NAME= \
  bash "$script" >"$work/out" 2>&1; then
  fail 'issue.sh passed without GITHUB_REF_NAME'
fi
expect_calls '.' 0

if [ "$failures" -gt 0 ]; then
  echo "::error::issue-test: $failures check(s) failed"
  exit 1
fi
echo 'issue.sh: every case passed.'
