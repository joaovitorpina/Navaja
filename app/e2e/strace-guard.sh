#!/usr/bin/env bash
# Linux network guard (docs/roadmap.md §2, "Offline and privacy checks").
#
# Runs the end-to-end suite with every app process (including WebKit's
# network and web processes, which strace -f follows) traced for connect,
# sendto, sendmsg and sendmmsg. Every AF_INET/AF_INET6 address in those calls
# must be 127.0.0.1 or ::1. Any other address fails the run, including the
# 127.0.0.53 DNS stub (the app must not even look a name up), and so does an
# inet call whose address can't be read.
#
# Usage (from the repo root, after the e2e build):
#   dbus-run-session -- xvfb-run -a bash app/e2e/strace-guard.sh
set -euo pipefail

# Reads strace output and prints every inet call that reaches an address
# other than 127.0.0.1 or ::1, or whose address can't be read. Each address is
# checked on its own: one sendmmsg line can carry several destinations, and a
# loopback one must not hide the others.
offending() {
  awk '
    /AF_INET/ {
      rest = $0
      found = 0
      bad = ""
      while (match(rest, /inet_addr\("[^"]*"\)|inet_pton\(AF_INET6, "[^"]*"/)) {
        addr = substr(rest, RSTART, RLENGTH)
        rest = substr(rest, RSTART + RLENGTH)
        sub(/^[^"]*"/, "", addr)
        sub(/".*$/, "", addr)
        found = 1
        if (addr != "127.0.0.1" && addr != "::1") bad = bad " " addr
      }
      if (!found) print "unreadable address: " $0
      else if (bad != "") print "to" bad ": " $0
    }
  ' "$@"
}

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
binary="$repo/target/debug/navaja"
traces="$(mktemp -d)"
wrapper="$traces/navaja-traced"

# --kill-on-exit needs strace 6.6 or newer (ubuntu-24.04 ships 6.8).
if ! strace -qq --kill-on-exit -o /dev/null true; then
  echo "::error::network guard: strace can't run with --kill-on-exit here (it needs strace 6.6 or newer and ptrace access)"
  exit 1
fi

# One trace file per launch: the single-instance spec starts a second process.
# With -o FILE PROG, strace ignores SIGTERM by default; -I1 lets the harness
# stop it, and --kill-on-exit takes the app and every process it started down
# with strace, so nothing keeps running untraced.
cat > "$wrapper" <<EOF
#!/usr/bin/env bash
exec strace -f -qq -I1 --kill-on-exit -e trace=connect,sendto,sendmsg,sendmmsg -o "$traces/trace.\$\$" "$binary" "\$@"
EOF
chmod +x "$wrapper"

# A Navaja already running from the same build (say, `pnpm tauri dev`) is
# not this run's: it is never reported or killed.
before="$(pgrep -f "$binary" || true)"
ours() {
  pgrep -f "$binary" | grep -vxF -f <(printf '%s\n' "$before") || true
}

status=0
NAVAJA_E2E_BINARY="$wrapper" pnpm e2e || status=$?

# Nothing the suite started may outlive it. The kernel ends the traced
# processes once strace exits, which can take a moment.
for _ in $(seq 20); do
  [ -z "$(ours)" ] && break
  sleep 0.25
done
left="$(ours)"
if [ -n "$left" ]; then
  echo "::error::network guard: app processes are still running after the end-to-end run"
  for pid in $left; do
    ps -o pid=,args= -p "$pid" || true
    kill -KILL "$pid" 2> /dev/null || true
  done
  status=1
fi

shopt -s nullglob
files=("$traces"/trace.*)
if [ "${#files[@]}" -eq 0 ]; then
  echo "::error::network guard: no strace output; the app never ran under the guard"
  exit 1
fi

found=$(offending "${files[@]}")
if [ -n "$found" ]; then
  echo "::error::network guard: the app tried to reach the network"
  printf '%s\n' "$found" | head -50 || true
  exit 1
fi
echo "Network guard: $(cat "${files[@]}" | wc -l) traced calls across ${#files[@]} launch(es); no connection left the machine."
exit "$status"
