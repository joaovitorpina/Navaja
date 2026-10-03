#!/usr/bin/env bash
# Fails if any tracked text file is stored with CRLF or mixed line endings.
# .gitattributes normalises text to LF; this catches files committed before
# the rule applied or with the attribute overridden.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

bad=$(git ls-files --eol | awk '$1 == "i/crlf" || $1 == "i/mixed"' || true)

if [ -n "$bad" ]; then
  echo "::error::Files stored with CRLF or mixed line endings:"
  printf '%s\n' "$bad"
  echo "Fix with: git add --renormalize ."
  exit 1
fi

echo "Line endings are LF."
