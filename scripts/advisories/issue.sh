#!/usr/bin/env bash
# Reports a failed advisories check (.github/workflows/advisories.yml, job
# report) as a GitHub issue. It opens the issue, or comments on it when it is
# already open, so a failure that lasts for days gives one issue with a
# comment per run instead of an issue per run.
#
# The open issue is found by its label and its exact title. An issue that
# was renamed, unlabelled or closed is not reused: the next failure opens a
# new one.
#
# Environment:
#   GH_REPO   owner/name of the repository
#   RUN_URL   the failed run, linked from the issue or comment
#   GH_TOKEN  for gh: issues: write, or issues: read for a dry run
#   DRY_RUN   1 looks the issue up and prints what it would do, and writes
#             nothing (advisories.yml runs it that way on a pull request)
#
# Exit status: 0 when the issue was opened or commented on (or would be, in
# a dry run); non-zero when gh fails or a variable is missing.
set -euo pipefail

: "${GH_REPO:?set GH_REPO to owner/name}"
: "${RUN_URL:?set RUN_URL to the failed run}"
dry_run=${DRY_RUN:-0}

# scripts/advisories/issue-test.sh expects these exact values. Changing one
# leaves an issue that is already open behind.
label=advisories
title='cargo deny check advisories is failing'
label_color=B60205
label_description='The daily RustSec advisories check is failing (advisories.yml)'

body="The advisories check failed: $RUN_URL

The run's log names each advisory and the crates it affects. The check also fails when cargo-deny cannot fetch the advisory database or the crate index, so read the log first.

To fix it, update the affected crate, or add the advisory to \`[advisories] ignore\` in deny.toml with a reason. Close this issue once a run passes. Each later failed run comments here while it is open."

comment="The advisories check failed again: $RUN_URL"

# Newest first, so a stray duplicate never hides the latest issue.
number=$(gh issue list --repo "$GH_REPO" --state open --label "$label" --limit 100 \
  --json number,title --jq '.[] | [.number, .title] | @tsv' |
  awk -F '\t' -v want="$title" '$2 == want && !found++ { print $1 }')

if [ -n "$number" ]; then
  if [ "$dry_run" = 1 ]; then
    echo "Dry run: would comment on #$number ('$title')."
    exit 0
  fi
  gh issue comment "$number" --repo "$GH_REPO" --body "$comment"
  echo "Commented on #$number."
  exit 0
fi

if [ "$dry_run" = 1 ]; then
  echo "Dry run: no open issue titled '$title' with the label '$label'; would open one."
  exit 0
fi

# gh issue create fails on a label the repository does not have.
labels=$(gh label list --repo "$GH_REPO" --limit 1000 --json name --jq '.[].name')
if ! grep -Fqx -- "$label" <<<"$labels"; then
  gh label create "$label" --repo "$GH_REPO" --color "$label_color" --description "$label_description"
fi

gh issue create --repo "$GH_REPO" --title "$title" --label "$label" --body "$body"
