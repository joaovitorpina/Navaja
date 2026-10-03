#!/usr/bin/env bash
# Checks that what cargo built on this Mac targets Navaja's minimum macOS,
# tauri.conf.json's bundle.macOS.minimumSystemVersion (docs/architecture.md
# §5). It reads the version back from each Mach-O file's own load command
# (LC_BUILD_VERSION's minos, or LC_VERSION_MIN_MACOSX's version on older
# files), which is what the compiler and the linker actually used, whatever
# set it: .cargo/config.toml's MACOSX_DEPLOYMENT_TARGET, the environment, or
# tauri-build.
#
# Usage, after a build: bash scripts/check-macos-target.sh <path>...
# A file is checked as it is. A directory is searched for what a cargo build
# links or compiles there: executables (the app, test binaries, build
# scripts), dynamic libraries (proc macros) and C objects and archives (the
# cc crate's output). Rust libraries (.rlib) are left out: their objects are
# compiled with the same target as the build script and executables next to
# them, and reading them all would take long.
#
# Fails if any file declares another version, or if no Mach-O file was found:
# a path that holds nothing proves nothing.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

[ "$(uname -s)" = Darwin ] || {
  echo "::error::check-macos-target.sh reads Mach-O files with otool; run it on macOS"
  exit 2
}
[ "$#" -gt 0 ] || {
  echo "usage: $0 <file or directory>..." >&2
  exit 2
}

want=$(node -p "require('./app/src-tauri/tauri.conf.json').bundle?.macOS?.minimumSystemVersion ?? ''")
[ -n "$want" ] || {
  echo "::error::tauri.conf.json sets no bundle.macOS.minimumSystemVersion"
  exit 1
}

list=$(mktemp)
report=$(mktemp)
trap 'rm -f "$list" "$report"' EXIT

for path in "$@"; do
  if [ -d "$path" ]; then
    find "$path" -type f \( \
      \( -perm -u+x ! -name '*.*' \) -o -name 'build-script-*' -o \
      -name '*.dylib' -o -name '*.o' -o -name '*.a' \) \
      ! -name '*.d' ! -name '*.rlib' ! -name '*.rmeta' -print0
  elif [ -f "$path" ]; then
    printf '%s\0' "$path"
  else
    echo "::error::$path does not exist"
    exit 1
  fi
done > "$list"

# One otool run for many files. Its output names each file (or archive
# member) on a line of its own that ends in ":", then its load commands.
# otool complains on stderr about files that are not Mach-O (scripts, say);
# those are simply not counted.
xargs -0 otool -arch all -l < "$list" 2> /dev/null | awk '
  /^[^ \t].*:$/ { file = substr($0, 1, length($0) - 1); next }
  $1 == "cmd" { cmd = $2; next }
  cmd == "LC_BUILD_VERSION" && $1 == "minos" { print $2 "\t" file; cmd = "" }
  cmd == "LC_VERSION_MIN_MACOSX" && $1 == "version" { print $2 "\t" file; cmd = "" }
' > "$report" || true

total=$(awk 'END { print NR }' "$report")
if [ "$total" -eq 0 ]; then
  echo "::error::no Mach-O file with a macOS version under: $*"
  exit 1
fi

echo "Minimum macOS declared by $total Mach-O file(s) or archive member(s) under $*:"
cut -f 1 "$report" | sort | uniq -c | sed 's/^/  /'
wrong=$(awk -F '\t' -v want="$want" '$1 != want' "$report")
if [ -n "$wrong" ]; then
  echo "::error::$(printf '%s\n' "$wrong" | awk 'END { print NR }') file(s) target another macOS than $want (tauri.conf.json):"
  printf '%s\n' "$wrong" | awk 'NR <= 40'
  exit 1
fi
echo "All target macOS $want, tauri.conf.json's bundle.macOS.minimumSystemVersion."
