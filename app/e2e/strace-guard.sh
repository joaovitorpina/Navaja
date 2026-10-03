#!/usr/bin/env bash
# Linux network guard (docs/roadmap.md §2, "Offline and privacy checks").
#
# Runs the end-to-end suite with every app process (including WebKit's
# network and web processes, which strace -f follows) traced for connect,
# sendto and sendmsg. Any AF_INET/AF_INET6 destination other than 127.0.0.1
# or ::1 fails the run, including the 127.0.0.53 DNS stub: the app must not
# even look a name up.
#
# Usage (from the repo root, after the e2e build):
#   dbus-run-session -- xvfb-run -a bash app/e2e/strace-guard.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="$repo/target/debug/navaja"
traces="$(mktemp -d)"
wrapper="$traces/navaja-traced"

# One trace file per launch: the single-instance spec starts a second process.
cat > "$wrapper" <<EOF
#!/usr/bin/env bash
exec strace -f -qq -e trace=connect,sendto,sendmsg -o "$traces/trace.\$\$" "$binary" "\$@"
EOF
chmod +x "$wrapper"

NAVAJA_E2E_BINARY="$wrapper" pnpm e2e

shopt -s nullglob
files=("$traces"/trace.*)
if [ "${#files[@]}" -eq 0 ]; then
  echo "::error::network guard: no strace output; the app never ran under the guard"
  exit 1
fi

# Keep inet destinations only, then drop loopback.
offending=$(grep -hE 'AF_INET6?' "${files[@]}" \
  | grep -vE 'inet_addr\("127\.0\.0\.1"\)|inet_pton\(AF_INET6, "::1"' || true)
if [ -n "$offending" ]; then
  echo "::error::network guard: the app tried to reach the network"
  printf '%s\n' "$offending" | head -50
  exit 1
fi
echo "Network guard: $(cat "${files[@]}" | wc -l) traced calls across ${#files[@]} launch(es); no connection left the machine."
