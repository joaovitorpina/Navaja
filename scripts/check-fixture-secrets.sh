#!/usr/bin/env bash
# Scans tracked test fixtures and snapshots for secrets with gitleaks
# (docs/roadmap.md §2). Raw captures go in tests/fixtures/ and insta
# snapshots in snapshots/ (docs/spikes.md, "Rules"), and either could carry
# a token copied from a real machine.
#
# Only tracked files are scanned, as staged in the index. They are copied to
# a temporary directory first, so gitleaks sees exactly that set and nothing
# else in the tree. Set GITLEAKS to use a gitleaks that is not on PATH.
#
# Exit status: 0 when nothing is found, or there is nothing to scan; 1 when
# gitleaks reports a finding, is missing, or fails.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Wider than crates/*/tests/fixtures/ so tool and app fixtures count too.
PATHSPECS=(
  ':(glob)**/tests/fixtures/**'
  ':(glob)**/snapshots/**'
  ':(glob)**/*.snap'
)

# Counts NUL-terminated records, so odd file names can't skew the number.
count_records() {
  tr -cd '\000' | wc -c | tr -d ' '
}

count=$(git ls-files -z -- "${PATHSPECS[@]}" | count_records)

if [ "$count" -eq 0 ]; then
  echo "No fixtures or snapshots to scan."
  exit 0
fi

gitleaks=${GITLEAKS:-gitleaks}
if ! command -v "$gitleaks" >/dev/null 2>&1; then
  echo "::error::gitleaks not found ('$gitleaks'); $count fixture or snapshot file(s) left unscanned"
  exit 1
fi

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

git ls-files -z -- "${PATHSPECS[@]}" | git checkout-index -z --stdin --prefix="$scratch/"

copied=$(find "$scratch" ! -type d -print0 | count_records)
if [ "$copied" -ne "$count" ]; then
  echo "::error::copied $copied of $count fixture or snapshot file(s); refusing a partial scan"
  exit 1
fi

# The built-in rules only: a config from the environment could weaken them.
unset GITLEAKS_CONFIG GITLEAKS_CONFIG_TOML

echo "Scanning $count fixture or snapshot file(s) with gitleaks:"
git ls-files -- "${PATHSPECS[@]}" | sed 's/^/  /'

rc=0
"$gitleaks" dir --no-banner --redact --verbose "$scratch" || rc=$?
if [ "$rc" -ne 0 ]; then
  echo "::error::gitleaks exited with $rc: a fixture or snapshot may hold a secret, or the scan failed. Report paths are under $scratch/. Redact the value and commit again."
  exit 1
fi

echo "No secrets in fixtures or snapshots."
