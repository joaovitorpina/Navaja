#!/usr/bin/env bash
# Checks that the frozen identifiers from docs/adr/0001-stack.md are spelled
# the same way everywhere in the tree. Any near-miss (another owner, another
# casing, another feed URL) fails the build.
#
# Portability notes:
# - Word boundaries come from `git grep -w`, not `\b`: macOS git uses the system
#   regex library, where `\b` in an extended regex matches a literal "b".
# - Every identifier is defined in the ADR, so zero candidates means the
#   pattern is broken on this platform, and that fails too.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

status=0

# check <description> <extra git grep flags, e.g. "-i" or ""> <regex> <exact allowed value>...
# Candidates are found with the given flags; the allowlist comparison is always
# exact and case-sensitive. Paths in EXCLUDES are skipped.
check() {
  local what=$1 flags=$2 pattern=$3
  shift 3
  local hits bad rc=0
  # --untracked also covers new files before their first commit; ignored files stay excluded.
  # shellcheck disable=SC2086
  hits=$(git grep --untracked -w $flags -h -o -I --no-line-number --no-column --color=never \
           -E "$pattern" -- . "${EXCLUDES[@]}") || rc=$?
  if [ "$rc" -gt 1 ]; then
    echo "::error::${what}: git grep failed (exit $rc)"
    status=1
    return 0
  fi
  if [ -z "$hits" ]; then
    echo "::error::${what}: no candidates found; the ADR defines this value, so the pattern is broken here"
    status=1
    return 0
  fi
  bad=$(printf '%s\n' "$hits" | sort -u | grep -v -x -F "$(printf '%s\n' "$@")" || true)
  if [ -n "$bad" ]; then
    status=1
    printf '%s\n' "$bad" | while IFS= read -r v; do
      echo "::error::${what}: unexpected value '${v}'"
      git grep --untracked -n -I --color=never -F -e "$v" -- . "${EXCLUDES[@]}" || true
    done
  fi
}

EXCLUDES=(':(exclude)scripts/check-identifiers.sh')

check "App identifier" "-i" \
  'io\.github\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+' \
  'io.github.joaovitorpina.Navaja'

check "Repository URL" "" \
  'github\.com[:/][A-Za-z0-9_.-]+/[Nn][Aa][Vv][Aa][Jj][Aa]' \
  'github.com/joaovitorpina/Navaja' 'github.com:joaovitorpina/Navaja'

check "Updater feed" "" \
  'https?://[^ )`"'"'"']*latest\.json' \
  'https://github.com/joaovitorpina/Navaja/releases/latest/download/latest.json'

check "winget PackageIdentifier" "" \
  '[A-Z][A-Za-z0-9]*\.[Nn][Aa][Vv][Aa][Jj][Aa]' \
  'JoaoVitorPina.Navaja'

check "Homebrew tap" "-i" \
  '[A-Za-z0-9_-]+/(homebrew-tap|tap/navaja)' \
  'joaovitorpina/homebrew-tap' 'joaovitorpina/tap/navaja'

check "Scoop bucket" "-i" \
  '[A-Za-z0-9_-]+/scoop-bucket' \
  'joaovitorpina/scoop-bucket'

# The brief's byline predates the frozen publisher spelling, so it is excluded.
EXCLUDES+=(':(exclude)BRIEF.md')
check "Publisher" "" \
  'Jo(a|ã)o Vitor Pina' \
  'João Vitor Pina'

if [ "$status" -eq 0 ]; then
  echo "Frozen identifiers are consistent."
fi
exit "$status"
