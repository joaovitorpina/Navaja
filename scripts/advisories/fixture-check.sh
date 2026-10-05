#!/usr/bin/env bash
# Shows that the advisories check fails when an advisory names a crate in
# Navaja's graph (.github/workflows/advisories.yml, on a pull request).
#
# It runs the daily job's command, `cargo deny --all-features check
# advisories`, with deny.toml's graph (targets, all features) unchanged, but
# against a scratch copy of the RustSec database with one made-up advisory
# added: RUSTSEC-2999-0001, for every version of serde. The check must fail,
# and must name that advisory. The real database in CARGO_HOME is left alone.
#
# Needs cargo-deny and the toolchain on PATH, and the network for the fetch.
# Exit status: 0 when the check fails on the fixture; 1 when it passes, or
# fails without naming the fixture.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fixture_id=RUSTSEC-2999-0001
fixture_crate=serde

fail() {
  echo "::error::fixture-check: $*"
  exit 1
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# cargo-deny is a native program: on Windows it needs C:/... rather than /tmp/...
native_path() {
  if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s\n' "$1"; fi
}
db_root=$(native_path "$work/advisory-dbs")
config=$(native_path "$work/deny.toml")

# deny.toml plus one line, db-path, so the database lands under $work.
awk -v root="$db_root" '{ print } $0 == "[advisories]" { print "db-path = \"" root "\"" }' deny.toml >"$config"
grep -qxF "db-path = \"$db_root\"" "$config" || fail "deny.toml has no [advisories] table to add db-path to"

# The database and the crates for every target, so the check below can run
# offline, with nothing fetched over the fixture.
cargo deny --all-features --config "$config" fetch db index

dbs=("$work"/advisory-dbs/*/)
[ "${#dbs[@]}" -eq 1 ] && [ -d "${dbs[0]}crates" ] || fail "expected one advisory database under $work/advisory-dbs"
db=${dbs[0]}

# cargo-deny ignores RUSTSEC-0000-0000, the template's placeholder, so the
# fixture needs an id of its own. No version is patched: every serde matches.
mkdir -p "$db/crates/$fixture_crate"
cat >"$db/crates/$fixture_crate/$fixture_id.md" <<EOF
\`\`\`toml
[advisory]
id = "$fixture_id"
package = "$fixture_crate"
date = "2026-10-04"
url = "https://github.com/joaovitorpina/Navaja/blob/main/scripts/advisories/fixture-check.sh"
categories = []
keywords = []

[versions]
patched = []
\`\`\`

# Fixture advisory from Navaja's advisories.yml, not a real one
EOF

status=0
cargo deny --all-features --config "$config" --offline check advisories >"$work/out.txt" 2>&1 || status=$?
# The head is the diagnostic; the rest is serde's inclusion graph.
head -n 12 "$work/out.txt"
echo "($(wc -l <"$work/out.txt" | tr -d ' ') lines of output in all)"

[ "$status" -ne 0 ] || fail "the check passed with $fixture_id in the database"
grep -qF "ID: $fixture_id" "$work/out.txt" || fail "the check failed (exit $status) but did not name $fixture_id"
echo "The check fails on $fixture_id (exit $status), as advisories.yml's daily run would."
