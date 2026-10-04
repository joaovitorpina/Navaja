#!/usr/bin/env bash
# Spike S2.7 (docs/spikes.md): does any process of Navaja's tree send a
# packet, idle or in use? The steps of the s2-7 job in
# .github/workflows/spikes.yml, one mode each, so that a run elsewhere does
# exactly what CI does.
#
# It captures on the machine it runs on and needs root (sudo). On Linux it
# changes the network setup, and on Windows the audit policy, so it is meant
# for a throwaway CI runner VM, not a developer's machine.
#
# Usage, from anywhere in the repository, after `pnpm install` and the
# end-to-end build
# (pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/e2e.conf.json):
#   bash scripts/spikes/s2-7-egress.sh setup           # Linux: the network namespace; Windows: auditing
#   bash scripts/spikes/s2-7-egress.sh control         # a known request must be captured and attributed
#   bash scripts/spikes/s2-7-egress.sh baseline        # 5 min under capture without the app
#   bash scripts/spikes/s2-7-egress.sh check baseline  # nothing may be attributed to the app then
#   bash scripts/spikes/s2-7-egress.sh idle            # the app idles 5 min, window shown, under capture
#   bash scripts/spikes/s2-7-egress.sh check idle      # what the capture holds from the process tree
#   bash scripts/spikes/s2-7-egress.sh in-use          # the end-to-end suite under capture
#   bash scripts/spikes/s2-7-egress.sh check in-use
#   bash scripts/spikes/s2-7-egress.sh teardown        # undoes setup
# On Windows every mode runs scripts/spikes/s2-7-egress.ps1 instead. The
# modes idle-app and in-use-app are the parts that run as the app's user,
# inside the namespace on Linux; idle and in-use call them.
#
# How it captures:
# - Linux: the app runs in a network namespace with a veth pair to the host,
#   NAT and a public resolver (8.8.8.8), so anything it sends leaves as
#   packets. In use, WebdriverIO runs there too: it talks to the app's
#   WebDriver on 127.0.0.1. tcpdump captures the host end of the veth and
#   the namespace's loopback. On the veth, the only packets expected from
#   the namespace are the kernel's own link-layer ones (ARP, IPv6 neighbour
#   discovery and MLD); any other is a finding. On the loopback, so is DNS.
#   After each phase, outside its capture, the namespace must still reach
#   github.com: a broken route would make a phase look clean.
# - macOS: tcpdump on pktap, which tags each packet with the process that
#   sent or received it, and the process it acted for. A packet outside lo0
#   is a finding when its process is navaja or one of WebKit's (Networking,
#   WebContent, GPU), or one of the daemons WebKit hands work to: webprivacyd
#   (its privacy lists), the Safe Browsing service, adattributiond (Private
#   Click Measurement) or webpushd (Web Push). WebKit starts them for the
#   app, outside its process tree. A daemon counts only if it started within
#   15 s after a start of the app: the VM starts some by itself.
# On both, a DNS question for one of the egress canary's hosts is a finding
# too, whichever process asks.
#
# Traffic from those daemons that an entry of s2-7-disclosed.tsv covers
# (service, host, phase) is reported as disclosed and fails nothing
# (docs/adr/0003-webview-network.md). Nothing from the tree's own processes
# can be disclosed.
#
# The baseline phase captures the same way for 5 min before the app ever
# runs. Nothing in it may count as the app's: otherwise the attribution
# blames the app for the machine's own traffic. Each later phase also lists
# the traffic outside the tree that the baseline lacks: on Linux, what the
# host itself sends on its uplink (a host daemon the app reached over a Unix
# socket or the system D-Bus would send from there); on macOS, each process
# that talks while the baseline had it silent. That list is for a person to
# review, and does not fail the check: these runners' own daemons come and
# go.
#
# The app is never asked to open a URL or the logs folder: the browser or
# file manager that would start is not Navaja (docs/architecture.md §5).
#
# Everything goes to $RUNNER_TEMP/navaja-s2-7 (or $S2_7_OUT): captures,
# listings, the app's output, a screenshot, and a findings table per phase
# (<phase>-findings.md). The workflow uploads that folder.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
REPO=$PWD
SELF=$REPO/scripts/spikes/s2-7-egress.sh

case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    exec pwsh -NoProfile -NonInteractive -File scripts/spikes/s2-7-egress.ps1 "$@"
    ;;
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *)
    echo "::error::S2.7: no capture for $(uname -s)"
    exit 2
    ;;
esac

OUT=${S2_7_OUT:-${RUNNER_TEMP:-/tmp}/navaja-s2-7}
mkdir -p "$OUT"
BINARY=$REPO/target/debug/navaja
# The harness's WebDriver port (wdio.conf.ts; @wdio/tauri-service's default).
PORT=4445
IDLE_SECONDS=${S2_7_IDLE_SECONDS:-300}
# Part of every host the egress canary asks for (app/e2e/specs/egress.e2e.ts).
CANARY=navaja-canary
# What the check reports as disclosed instead of failing on: traffic the app
# sets off that nothing it controls can stop (docs/adr/0003-webview-network.md).
DISCLOSED=$REPO/scripts/spikes/s2-7-disclosed.tsv

# Linux: the namespace, the veth pair that links it to the host, their
# addresses, and the namespace's resolver.
NS=navaja-s27
HOST_IF=s27-host
NS_IF=s27-ns
NET4=10.27.0
NET6=fd00:27::
RESOLVER=8.8.8.8

# What the kernel sends on a link by itself: ARP; IPv6 router and neighbour
# solicitations and advertisements and redirects (ICMPv6 133-137); and MLD
# queries, reports and dones (130-132, 143), which come behind an 8-byte
# hop-by-hop header.
ND='icmp6 and ip6[40] >= 133 and ip6[40] <= 137'
MLD='ip6 and ip6[6] == 0 and ip6[40] == 58 and ip6[41] == 0 and (ip6[48] == 130 or ip6[48] == 131 or ip6[48] == 132 or ip6[48] == 143)'
NOISE="arp or ($ND) or ($MLD)"
# A TCP connection attempt (SYN without ACK). libpcap can't index into TCP
# over IPv6, so the flags are read at their offset after a bare IPv6 header.
SYN='(ip and tcp[tcpflags] & (tcp-syn|tcp-ack) == tcp-syn) or (ip6 and ip6[6] == 6 and ip6[53] & 0x12 == 0x02)'

# Linux: what a capture of the host's uplink keeps of each packet. Headers
# only, but DNS whole: the uplink carries the whole runner's traffic, and the
# capture is uploaded.
UPLINK_SNAP=96
UPLINK_DNS_SNAP=512

# macOS: the processes of the tree, and the daemons that WebKit starts for
# it, as pktap names them. It cuts a name to 16 characters (15 for the
# process acted for), so com.apple.WebKit.Networking may read
# com.apple.WebKit, and com.apple.Safari.SafeBrowsing.Service reads
# com.apple.Safari or com.apple.Safar. A daemon counts as the app's only if
# it started within DAEMON_WINDOW seconds after a start of the app, by ps's
# start times in the job's process snapshots (<phase>-daemons.tsv); one the
# snapshots never saw counts too. Starting after the app is not enough: the
# VM starts the Safe Browsing service by itself, before a job and, in run
# 37160662987, 8 s into the baseline, which never runs the app.
# pktap may cut com.apple.WebKit.Networking to 15 characters, com.apple.WebKi.
TREE_PROCS='^(navaja|com\.apple\.WebKi.*)$'
WEBKIT_DAEMONS='^(webprivacyd|com\.apple\.Safar.*|adattributiond|webpushd)$'
# The same daemons as ps names them.
DAEMON_COMMANDS='webprivacyd|SafeBrowsing\.Service|adattributiond|webpushd'
# webprivacyd started within 1 s of the app in every run so far, and the
# Safe Browsing service 4 s after it in run 37151706483.
DAEMON_WINDOW=15
# Set by check() for the tree's checks only: the daemons' pktap names, and
# the PIDs (" 1 2 ") of those not counted as the app's.
MAC_DAEMONS=''
MAC_EXCLUDED=' '

fail() {
  echo "::error::S2.7 $*"
  exit 1
}

# What runs in the background, stopped on exit: captures (one PID file
# each), the process watcher, the app.
CAPTURES=()
WATCH_PID=''
APP_PID=''
cleanup() {
  local file
  for file in ${CAPTURES[@]+"${CAPTURES[@]}"}; do
    stop_capture "$file"
  done
  stop_watch
  if [ -n "$APP_PID" ]; then
    kill -KILL "$APP_PID" 2> /dev/null || true
  fi
}
trap cleanup EXIT

# Lines in a file, 0 if it is missing.
lines() {
  if [ -f "$1" ]; then
    awk 'END { print NR }' "$1"
  else
    echo 0
  fi
}

now() {
  date +%s
}

# with_timeout <seconds> <command...>: the command, killed if it runs longer
# (macOS has no timeout(1)).
with_timeout() {
  local seconds=$1 pid watcher status=0
  shift
  "$@" &
  pid=$!
  # Off the caller's output, which may be a command substitution that would
  # otherwise wait for the sleep.
  (
    sleep "$seconds"
    kill "$pid" 2> /dev/null
  ) > /dev/null 2>&1 &
  watcher=$!
  wait "$pid" || status=$?
  kill "$watcher" 2> /dev/null || true
  wait "$watcher" 2> /dev/null || true
  return "$status"
}

# start_capture <name> <command...>: runs a tcpdump command as root in the
# background, keeps its PID, and waits until it listens.
start_capture() {
  local name=$1 pidfile=$OUT/$1.pid log=$OUT/$1.tcpdump.log i
  shift
  rm -f "$pidfile"
  sudo sh -c 'echo $$ > "$1"; shift; exec "$@"' sh "$pidfile" "$@" > "$log" 2>&1 &
  CAPTURES+=("$pidfile")
  for ((i = 0; i < 100; i++)); do
    grep -q 'listening on' "$log" 2> /dev/null && return 0
    sleep 0.1
  done
  cat "$log"
  fail "capture $name: tcpdump did not start listening within 10 s"
}

# Stops a capture with SIGINT, so tcpdump writes out what it holds.
stop_capture() {
  local pidfile=$1 pid i
  [ -f "$pidfile" ] || return 0
  pid=$(cat "$pidfile")
  sudo kill -INT "$pid" 2> /dev/null || true
  for ((i = 0; i < 100; i++)); do
    sudo kill -0 "$pid" 2> /dev/null || break
    sleep 0.1
  done
  sudo kill -KILL "$pid" 2> /dev/null || true
  rm -f "$pidfile"
}

# What each capture of the phase reported when it stopped.
capture_counts() {
  local log
  for log in "$OUT/$1"*.tcpdump.log; do
    [ -f "$log" ] || continue
    echo "$(basename "$log" .tcpdump.log): $(grep -E 'packets (captured|received by filter|dropped by kernel)' "$log" | tr '
' ' ')"
  done
}

stop_captures() {
  local file
  for file in ${CAPTURES[@]+"${CAPTURES[@]}"}; do
    stop_capture "$file"
  done
  CAPTURES=()
  # The sudo processes that ran them.
  wait 2> /dev/null || true
}

# The captures of one phase.
start_captures() {
  local phase=$1
  now > "$OUT/$phase-start"
  case $OS in
    linux)
      # -Z root: Ubuntu's tcpdump drops to its own user before it opens the
      # file otherwise, and that user can't write here.
      start_capture "$phase-veth" tcpdump -i "$HOST_IF" -n -U -Z root -w "$OUT/$phase-veth.pcap"
      start_capture "$phase-lo" ip netns exec "$NS" tcpdump -i lo -n -U -Z root -w "$OUT/$phase-lo.pcap"
      # The host's own traffic, for the list of what the baseline lacks.
      if [ "$phase" != control ]; then
        local uplink
        uplink=$(cat "$OUT/uplink")
        start_capture "$phase-uplink" tcpdump -i "$uplink" -n -U -Z root -s "$UPLINK_SNAP" \
          -w "$OUT/$phase-uplink.pcap" 'not port 53'
        start_capture "$phase-uplink-dns" tcpdump -i "$uplink" -n -U -Z root -s "$UPLINK_DNS_SNAP" \
          -w "$OUT/$phase-uplink-dns.pcap" 'port 53'
      fi
      ;;
    macos)
      # pktap,all: every interface, each packet with its process. 512 bytes
      # keep the headers and a DNS question, not whole payloads: the capture
      # holds the whole runner's traffic, and it is uploaded.
      start_capture "$phase" tcpdump -i pktap,all -P -n -U -s 512 -w "$OUT/$phase.pcapng"
      ;;
  esac
}

# Linux: as root in the namespace.
ns() {
  sudo ip netns exec "$NS" "$@"
}

# Linux: as this user, in the namespace, with this environment (sudo would
# reset it).
in_ns() {
  sudo -E env "PATH=$PATH" "HOME=$HOME" ip netns exec "$NS" \
    setpriv --reuid="$(id -u)" --regid="$(id -g)" --init-groups -- "$@"
}

# Linux: the processes in the namespace, captures left out.
ns_pids() {
  local pids skip='' file
  pids=$(sudo ip netns pids "$NS" 2> /dev/null || true)
  for file in ${CAPTURES[@]+"${CAPTURES[@]}"}; do
    [ -f "$file" ] && skip="$skip $(cat "$file")"
  done
  for pid in $pids; do
    case " $skip " in *" $pid "*) ;; *) echo "$pid" ;; esac
  done
}

# Fails unless this process runs in the namespace (Linux).
assert_in_namespace() {
  ip -o -4 addr show dev "$NS_IF" 2> /dev/null | grep -F "$NET4.2/" > /dev/null ||
    fail "not inside the namespace $NS: no $NS_IF with $NET4.2 here"
}

# What the tree looks like now: on Linux every process in the namespace, on
# macOS navaja and WebKit's processes. From the host's side only: sudo in
# the namespace could look the machine's name up.
snapshot() {
  local pids
  case $OS in
    linux)
      pids=$(ns_pids | paste -sd, -)
      [ -z "$pids" ] || ps -o pid=,ppid=,lstart=,args= -p "$pids" || true
      ;;
    macos) app_processes ;;
  esac
}

# The app's processes and WebKit's, without sudo; on macOS also the daemons
# WebKit starts for it, by their full names (pktap cuts them short).
app_processes() {
  local list
  case $OS in
    linux) list=$(ps -eo pid=,ppid=,lstart=,args=) ;;
    # In UTC, as mac_starts reads the start times.
    macos) list=$(TZ=UTC ps -axo pid=,ppid=,lstart=,command=) ;;
  esac
  printf '%s\n' "$list" |
    grep -E 'navaja|com\.apple\.WebKit|WebKit(Network|Web|GPU)Process|webprivacyd|SafeBrowsing|adattributiond|webpushd' || true
}

# Writes a snapshot every 5 s to <phase>-processes.txt, until stopped.
watch_processes() {
  local file=$OUT/$1-processes.txt
  (
    while :; do
      echo "--- $(date -u +%H:%M:%S)"
      snapshot
      sleep 5
    done
  ) > "$file" 2>&1 &
  WATCH_PID=$!
}

stop_watch() {
  [ -n "$WATCH_PID" ] || return 0
  kill "$WATCH_PID" 2> /dev/null || true
  wait "$WATCH_PID" 2> /dev/null || true
  WATCH_PID=''
}

# Waits until the run has left nothing behind, so the capture covers the
# tree's whole life. Whatever is still there after 30 s is listed and killed
# (it was captured until then). On macOS that is the app; WebKit's processes
# are XPC services that outlive it by a moment, so they get 10 s more, and
# are never killed (other programs may use them).
settle() {
  local i left
  for ((i = 0; i < 60; i++)); do
    case $OS in
      linux) left=$(ns_pids) ;;
      macos) left=$(pgrep -f "$BINARY" || true) ;;
    esac
    [ -n "$left" ] || break
    sleep 0.5
  done
  if [ -n "$left" ]; then
    echo "::warning::S2.7 still running 30 s after the run, now killed:"
    # shellcheck disable=SC2086
    ps -o pid=,args= -p "$(echo $left | tr ' ' ,)" || true
    # shellcheck disable=SC2086
    sudo kill -KILL $left 2> /dev/null || true
  fi
  if [ "$OS" = macos ]; then
    for ((i = 0; i < 20; i++)); do
      pgrep -f 'com\.apple\.WebKit' > /dev/null || break
      sleep 0.5
    done
    if pgrep -f 'com\.apple\.WebKit' > /dev/null; then
      echo "WebKit processes still running (captured until now):"
      pgrep -fl 'com\.apple\.WebKit' || true
    fi
  fi
  # A moment for anything still in flight.
  sleep 2
}

port_open() {
  curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$PORT/" 2> /dev/null
  [ $? -ne 7 ]
}

setup() {
  case $OS in
    linux) linux_setup ;;
    macos)
      command -v tcpdump > /dev/null || fail "setup: no tcpdump"
      # pktap needs Apple's tcpdump.
      tcpdump --version 2>&1 | awk 'NR <= 3'
      sw_vers
      echo "macOS needs no setup: pktap is built in."
      ;;
  esac
}

linux_setup() {
  local uplink
  command -v tcpdump > /dev/null || fail "setup: no tcpdump"
  uplink=$(ip -4 route show default | awk '{ for (i = 1; i < NF; i++) if ($i == "dev") { print $(i + 1); exit } }')
  [ -n "$uplink" ] || fail "setup: no default IPv4 route"
  sudo ip netns add "$NS"
  sudo ip link add "$HOST_IF" type veth peer name "$NS_IF"
  sudo ip link set "$NS_IF" netns "$NS"
  sudo ip addr add "$NET4.1/24" dev "$HOST_IF"
  sudo ip -6 addr add "${NET6}1/64" dev "$HOST_IF" nodad
  sudo ip link set "$HOST_IF" up
  ns ip link set lo up
  ns ip addr add "$NET4.2/24" dev "$NS_IF"
  ns ip -6 addr add "${NET6}2/64" dev "$NS_IF" nodad
  ns ip link set "$NS_IF" up
  ns ip route add default via "$NET4.1"
  ns ip -6 route add default via "${NET6}1"
  # IPv4 leaves through the host's default route, masqueraded. Docker on the
  # runner sets the FORWARD policy to DROP, so both directions are let
  # through ahead of its rules. IPv6 is not forwarded (the runner has no
  # IPv6 route out), but a packet sent to an IPv6 address still crosses the
  # veth, where it is captured.
  sudo sysctl -qw net.ipv4.ip_forward=1
  sudo iptables -t nat -A POSTROUTING -s "$NET4.0/24" -o "$uplink" -j MASQUERADE
  sudo iptables -I FORWARD 1 -i "$HOST_IF" -o "$uplink" -j ACCEPT
  sudo iptables -I FORWARD 1 -i "$uplink" -o "$HOST_IF" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
  # `ip netns exec` mounts each file in /etc/netns/<ns>/ over the one in
  # /etc/. A public resolver, so a lookup crosses the veth; the host's
  # systemd-resolved stub, 127.0.0.53, is not in the namespace at all.
  # `hosts: files dns` keeps glibc from handing names to systemd-resolved
  # over its Unix socket (nss-resolve), which is outside the namespace and
  # would look them up from the host, out of the capture's sight.
  sudo mkdir -p "/etc/netns/$NS"
  echo "nameserver $RESOLVER" | sudo tee "/etc/netns/$NS/resolv.conf" > /dev/null
  awk '/^hosts:/ { print "hosts: files dns"; next } { print }' /etc/nsswitch.conf |
    sudo tee "/etc/netns/$NS/nsswitch.conf" > /dev/null
  grep -qx 'hosts: files dns' "/etc/netns/$NS/nsswitch.conf" || fail "setup: /etc/nsswitch.conf has no hosts line"
  # The machine's own name, so that a tool that looks it up (sudo, xauth)
  # finds it in /etc/hosts instead of asking the resolver, which would be a
  # packet from the namespace. Such a lookup sends nothing anywhere.
  {
    cat /etc/hosts
    grep -qw "$(hostname)" /etc/hosts || echo "127.0.1.1 $(hostname)"
  } | sudo tee "/etc/netns/$NS/hosts" > /dev/null
  echo "$uplink" > "$OUT/uplink"
  ip -4 -o addr show dev "$uplink" | awk '{ sub(/\/.*/, "", $4); print $4; exit }' > "$OUT/uplink-ip"
  [ -s "$OUT/uplink-ip" ] || fail "setup: no IPv4 address on $uplink"
  ns cat "/sys/class/net/$NS_IF/address" > "$OUT/namespace-mac"
  echo "Namespace $NS (uplink $uplink, $(cat "$OUT/uplink-ip"); NAT for $NET4.0/24):"
  ns ip -br addr
  ns ip route
  ns ip -6 route
  echo "resolv.conf: $(ns cat /etc/resolv.conf | tr '\n' ' ')"
  echo "nsswitch.conf: $(ns grep '^hosts:' /etc/nsswitch.conf)"
  tcpdump --version 2>&1 | awk 'NR <= 2'
}

teardown() {
  case $OS in
    linux)
      local uplink pid
      uplink=$(cat "$OUT/uplink" 2> /dev/null || true)
      for pid in $(sudo ip netns pids "$NS" 2> /dev/null || true); do
        sudo kill -KILL "$pid" 2> /dev/null || true
      done
      # Deleting the namespace deletes the veth pair with it.
      sudo ip netns del "$NS" 2> /dev/null || true
      sudo ip link del "$HOST_IF" 2> /dev/null || true
      if [ -n "$uplink" ]; then
        sudo iptables -t nat -D POSTROUTING -s "$NET4.0/24" -o "$uplink" -j MASQUERADE 2> /dev/null || true
        sudo iptables -D FORWARD -i "$HOST_IF" -o "$uplink" -j ACCEPT 2> /dev/null || true
        sudo iptables -D FORWARD -i "$uplink" -o "$HOST_IF" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2> /dev/null || true
      fi
      sudo rm -rf "/etc/netns/$NS"
      echo "Namespace, veth pair and NAT rules removed."
      ;;
    macos)
      pkill -KILL -f "$BINARY" 2> /dev/null || true
      echo "Nothing to undo on macOS."
      ;;
  esac
}

# --- Classifying a capture ---------------------------------------------------

# packets <capture> <filter> <listing> <tcpdump flags...>: writes the
# packets that match, one line each, epoch time first. -q gives the short
# form that group() reads, -v the long one, with DNS questions. Fails if
# tcpdump can't read the file or the filter, so a broken filter never reads
# as "no packets".
packets() {
  local file=$1 filter=$2 listing=$3
  shift 3
  if ! tcpdump -n -tt "$@" -r "$file" ${filter:+"$filter"} > "$listing" 2> "$listing.err"; then
    cat "$listing.err"
    fail "tcpdump could not read $file with the filter: ${filter:-none}"
  fi
  rm -f "$listing.err"
}

# Groups packet lines ("<label>\t<tcpdump -q -tt line>") by label,
# destination, port and protocol, as tab-separated rows: label, family,
# destination, port, protocol, packets, first seen (epoch).
group() {
  awk -F '\t' '
    {
      n = split($2, f, " ")
      family = f[2]
      dst = f[5]
      proto = f[6]
      sub(/:$/, "", dst)
      sub(/,$/, "", proto)
      port = ""
      if (proto == "tcp" || proto == "UDP" || proto == "udp") {
        port = dst
        sub(/.*\./, "", port)
        sub(/\.[^.]*$/, "", dst)
      }
      key = $1 SUBSEP family SUBSEP dst SUBSEP port SUBSEP proto
      if (!(key in count)) {
        order[++keys] = key
        first[key] = f[1]
      }
      count[key]++
    }
    END {
      for (i = 1; i <= keys; i++) {
        split(order[i], k, SUBSEP)
        printf "%s\t%s\t%s\t%s\t%s\t%d\t%s\n", k[1], k[2], k[3], k[4], k[5], count[order[i]], first[order[i]]
      }
    }
  ' "$@"
}

# Markdown rows for grouped findings; times relative to the phase's start.
finding_rows() {
  local start=$1 file=$2
  awk -F '\t' -v start="$start" '
    { printf "| %s | %s | %s | %s %s | %d | +%.1f s |\n", $1, $3, ($4 == "" ? "-" : $4), $2, $5, $6, $7 - start }
  ' "$file"
}

# The DNS questions in DNS packets in tcpdump's long form (on stdin), with
# a count each.
questions() {
  grep -oE '[A-Za-z0-9]+\? [^ ]+' | sort | uniq -c | sort -rn || true
}

# Joins tcpdump's continuation lines (indented) to the line before, so that
# each packet is one line, its time stamp first.
join_lines() {
  awk '
    /^[ \t]/ { sub(/^[ \t]+/, " "); line = line $0; next }
    { if (have) print line; line = $0; have = 1 }
    END { if (have) print line }
  ' "$1" > "$1.joined"
  mv "$1.joined" "$1"
}

# check <phase>: lists what the phase's capture holds from the process tree,
# writes <phase>-findings.md and -findings.tsv, and fails on any finding. The
# control calls it too, where findings are expected. In the baseline, which
# runs without the app, any finding means that the attribution would blame
# the app for the machine's own traffic. After the baseline, each phase also
# lists the traffic outside the tree that the baseline lacks, for review
# (<phase>-suspects.md, in the findings table too, and its count in
# <phase>-suspects.count); that list fails nothing.
check() {
  local phase=$1
  rm -f "$OUT/$phase-suspects.md" "$OUT/$phase-suspects.count"
  case $OS in
    linux) check_linux "$phase" ;;
    macos)
      MAC_DAEMONS=$WEBKIT_DAEMONS
      mac_daemons "$phase"
      check_macos "$phase" "$TREE_PROCS" ''
      ;;
  esac
}

# macOS: "<pid>\t<epoch>\t<command>" for each process in the job's process
# snapshots (and in record_daemons's lists) whose command contains <text>
# (mode -F) or matches <regex> (mode -E), with its start time as ps gives
# it, to the second. The epoch is empty if date can't read it.
mac_starts() {
  local mode=$1 pattern=$2 file pid when command
  for file in "$OUT"/*-processes.txt "$OUT"/*-daemons-before-ps.txt; do
    [ ! -f "$file" ] || cat "$file"
  done | MODE=$mode PATTERN=$pattern awk '
    # "<pid> [<ppid>] <weekday> <month> <day> <time> <year> <command>"
    $1 !~ /^[0-9]+$/ || ($1 in seen) { next }
    ENVIRON["MODE"] == "-F" && !index($0, ENVIRON["PATTERN"]) { next }
    ENVIRON["MODE"] == "-E" && $0 !~ ENVIRON["PATTERN"] { next }
    {
      seen[$1] = 1
      i = ($2 ~ /^(Mon|Tue|Wed|Thu|Fri|Sat|Sun)$/) ? 2 : 3
      command = ""
      for (k = i + 5; k <= NF; k++) command = command (command == "" ? "" : " ") $k
      print $1 "\t" $i " " $(i + 1) " " $(i + 2) " " $(i + 3) " " $(i + 4) "\t" command
    }
  ' | while IFS=$'\t' read -r pid when command; do
    printf '%s\t%s\t%s\n' "$pid" "$(TZ=UTC date -j -f '%a %b %d %T %Y' "$when" +%s 2> /dev/null || true)" "$command"
  done
}

# macOS: decides which WebKit daemons count as the app's (see DAEMON_WINDOW),
# into <phase>-daemons.tsv: "<pid>\t<start>\t<counted or left out>\t<why>\t
# <command>". Sets MAC_EXCLUDED to those left out. Later phases' snapshots
# count too, if they exist: a daemon may outlive the phase that woke it.
mac_daemons() {
  local phase=$1
  mac_starts -F "$BINARY" > "$OUT/app-starts.tsv"
  mac_starts -E "$DAEMON_COMMANDS" > "$OUT/daemon-starts.tsv"
  if [ "$phase" != baseline ] && ! awk -F '\t' '$2 != "" { found = 1 } END { exit !found }' "$OUT/app-starts.tsv"; then
    fail "check $phase: the process snapshots hold no start of $BINARY, so no WebKit daemon could be tied to the app"
  fi
  awk -F '\t' -v window="$DAEMON_WINDOW" '
    FILENAME == ARGV[1] { if ($2 != "") { app[++n] = $2; pid[n] = $1 } next }
    {
      verdict = "left out"
      why = "not within " window " s after a start of the app"
      if ($2 == "") { verdict = "counted"; why = "start time unknown" }
      for (i = 1; i <= n && verdict == "left out"; i++) {
        if ($2 >= app[i] && $2 <= app[i] + window) {
          verdict = "counted"
          why = sprintf("%d s after a start of the app (PID %s)", $2 - app[i], pid[i])
        }
      }
      print $1 "\t" $2 "\t" verdict "\t" why "\t" $3
    }
  ' "$OUT/app-starts.tsv" "$OUT/daemon-starts.tsv" > "$OUT/$phase-daemons.tsv"
  MAC_EXCLUDED=" $(awk -F '\t' '$3 == "left out" { printf "%s ", $1 }' "$OUT/$phase-daemons.tsv")"
}

# macOS: the daemons that <phase>-daemons.tsv marks <verdict> ("counted" or
# "left out"), as "3216 (`/System/.../com.apple.Safari.SafeBrowsing.Service`,
# started 23:08:44 UTC, not within 15 s after a start of the app)", or "none".
daemon_names() {
  local out
  out=$(awk -F '\t' -v verdict="$2" '$3 == verdict { print $1 "\t" $2 "\t" $4 "\t" $5 }' "$OUT/$1-daemons.tsv" |
    while IFS=$'\t' read -r pid when why command; do
      [ -z "$when" ] || when="started $(TZ=UTC date -r "$when" +%T) UTC, "
      printf '%s (`%s`, %s%s); ' "$pid" "$command" "$when" "$why"
    done)
  out=${out%; }
  echo "${out:-none}"
}

# macOS: the PIDs of the WebKit daemons running now, one a line, and their
# commands and start times on stderr.
daemon_pids() {
  local pids
  pids=$(pgrep -f "$DAEMON_COMMANDS" || true)
  if [ -n "$pids" ]; then
    # shellcheck disable=SC2086
    TZ=UTC ps -o pid=,lstart=,command= -p "$(echo $pids | tr ' ' ,)" >&2 || true
    printf '%s\n' $pids
  fi
}

# macOS: before a phase, the WebKit daemons already running, with their
# start times, for the record; mac_starts reads them too.
record_daemons() {
  local phase=$1
  [ "$OS" = macos ] || return 0
  daemon_pids > "$OUT/$phase-daemons-before.txt" 2> "$OUT/$phase-daemons-before-ps.txt"
  echo "WebKit daemons running before the $phase phase:"
  if [ -s "$OUT/$phase-daemons-before.txt" ]; then
    cat "$OUT/$phase-daemons-before-ps.txt"
  else
    echo "  none"
  fi
}

check_linux() {
  local phase=$1 veth=$OUT/$1-veth.pcap lo=$OUT/$1-lo.pcap mac start
  local total noise sent hostside lodns findings webdriver md=$OUT/$1-findings.md tsv=$OUT/$1-findings.tsv
  [ -f "$veth" ] && [ -f "$lo" ] || fail "check $phase: no capture; run the $phase mode first"
  mac=$(cat "$OUT/namespace-mac")
  start=$(cat "$OUT/$phase-start")

  # Everything, for the artifact; then each class.
  packets "$veth" '' "$OUT/$phase-veth.txt" -e -v
  packets "$veth" '' "$OUT/$phase-veth-all.txt" -q
  packets "$veth" "$NOISE" "$OUT/$phase-veth-noise.txt" -q
  packets "$veth" "arp" "$OUT/$phase-veth-arp.txt" -q
  packets "$veth" "$ND" "$OUT/$phase-veth-nd.txt" -q
  packets "$veth" "$MLD" "$OUT/$phase-veth-mld.txt" -q
  packets "$veth" "ether src $mac and not ($NOISE)" "$OUT/$phase-veth-sent.txt" -q
  packets "$veth" "not ether src $mac and not ($NOISE)" "$OUT/$phase-veth-host.txt" -q
  packets "$lo" '' "$OUT/$phase-lo.txt" -q
  packets "$lo" 'port 53' "$OUT/$phase-lo-dns.txt" -q
  packets "$lo" "$SYN" "$OUT/$phase-lo-syn.txt" -q
  # The same packets in tcpdump's long form, which shows DNS questions.
  packets "$veth" "ether src $mac and not ($NOISE)" "$OUT/$phase-veth-sent-long.txt" -v
  packets "$lo" 'port 53' "$OUT/$phase-lo-dns-long.txt" -v

  # The harness's and the phase's own connections to the app's WebDriver:
  # they show that the app ran in this namespace.
  webdriver=$(grep -cE "> (127\.0\.0\.1|::1)\.$PORT: " "$OUT/$phase-lo-syn.txt" || true)
  total=$(lines "$OUT/$phase-veth-all.txt")
  noise=$(lines "$OUT/$phase-veth-noise.txt")
  sent=$(lines "$OUT/$phase-veth-sent.txt")
  hostside=$(lines "$OUT/$phase-veth-host.txt")
  lodns=$(lines "$OUT/$phase-lo-dns.txt")
  findings=$((sent + lodns))
  # The disclosed list has no entry for Linux: everything the namespace
  # sends is a finding.
  echo 0 > "$OUT/$phase-disclosed.count"

  {
    awk '{ print "namespace, veth\t" $0 }' "$OUT/$phase-veth-sent.txt"
    awk '{ print "namespace, loopback DNS\t" $0 }' "$OUT/$phase-lo-dns.txt"
  } | group > "$tsv"

  {
    echo "#### $phase: $(os_label)"
    echo
    echo "| Capture | Packets | Kernel link-layer noise | Findings | Host side, not noise |"
    echo "|---|---|---|---|---|"
    echo "| Host end of the veth (\`$HOST_IF\`) | $total | $noise | $sent sent by the namespace | $hostside |"
    echo "| Namespace loopback | $(lines "$OUT/$phase-lo.txt") | - | $lodns DNS (port 53) | - |"
    echo
    echo "Connection attempts on the namespace's loopback, which never leave it: $(loopback_summary "$OUT/$phase-lo-syn.txt")."
    echo
    echo "Link-layer noise, by kind: ARP $(lines "$OUT/$phase-veth-arp.txt"), IPv6 neighbour discovery $(lines "$OUT/$phase-veth-nd.txt"), MLD $(lines "$OUT/$phase-veth-mld.txt")."
    echo
    if [ "$findings" -eq 0 ]; then
      echo "No packet from the process tree."
    else
      echo "| Sent by | Remote end | Port | Protocol | Packets | First, from the phase's start |"
      echo "|---|---|---|---|---|---|"
      finding_rows "$start" "$tsv"
      dns_lines "$OUT/$phase-veth-sent-long.txt" "$OUT/$phase-lo-dns-long.txt"
    fi
    echo
  } > "$md"
  case $phase in
    idle | in-use)
      if [ "$webdriver" -eq 0 ]; then
        cat "$md"
        fail "$phase: no connection to the app's WebDriver on the namespace's loopback; the app did not run in this namespace, so the capture proves nothing"
      fi
      if [ -f "$OUT/$phase-no-way-out" ]; then
        cat "$md"
        fail "$phase: after the phase the namespace could no longer reach the network, so a send would have failed inside it and its empty capture proves nothing"
      fi
      ;;
  esac
  if [ "$phase" != control ]; then
    uplink_flows "$phase"
    if [ "$phase" != baseline ]; then
      suspects_linux "$phase"
      cat "$OUT/$phase-suspects.md" >> "$md"
    fi
  fi
  report "$phase" "$md" "$findings" "$OUT/$phase-veth-sent-long.txt" "$OUT/$phase-lo-dns-long.txt"
}

# Linux: what the host itself sent on its uplink in the phase, by remote end
# (<phase>-uplink.tsv, rows as group() writes them) and the DNS questions it
# asked (<phase>-uplink-names.txt).
uplink_flows() {
  local phase=$1 ip
  ip=$(cat "$OUT/uplink-ip")
  [ -f "$OUT/$phase-uplink.pcap" ] && [ -f "$OUT/$phase-uplink-dns.pcap" ] ||
    fail "check $phase: no capture of the uplink"
  packets "$OUT/$phase-uplink.pcap" "src host $ip" "$OUT/$phase-uplink-sent.txt" -q
  packets "$OUT/$phase-uplink-dns.pcap" "src host $ip" "$OUT/$phase-uplink-dns-sent.txt" -q
  packets "$OUT/$phase-uplink-dns.pcap" "src host $ip" "$OUT/$phase-uplink-dns-long.txt" -v
  cat "$OUT/$phase-uplink-sent.txt" "$OUT/$phase-uplink-dns-sent.txt" |
    awk '{ print "host\t" $0 }' | group | sort -t "$(printf '\t')" -k7,7n > "$OUT/$phase-uplink.tsv"
  questions < "$OUT/$phase-uplink-dns-long.txt" > "$OUT/$phase-uplink-names.txt"
  echo "$phase: the host sent $(lines "$OUT/$phase-uplink-sent.txt") packet(s) and $(lines "$OUT/$phase-uplink-dns-sent.txt") DNS packet(s) on its uplink, to $(lines "$OUT/$phase-uplink.tsv") remote end(s)."
}

# Linux: the remote ends and names the host's uplink shows in the phase but
# not in the baseline, into <phase>-suspects.md and .count.
suspects_linux() {
  local phase=$1 start count ends names file=$OUT/$1-suspects.md
  start=$(cat "$OUT/$phase-start")
  {
    echo
    echo "##### Outside the process tree, not in the baseline (for review, not a finding)"
    echo
    echo "What the host itself sent on its uplink (\`$(cat "$OUT/uplink")\`) in the phase, to remote ends and names that the 5 min baseline did not show. A host daemon the app reached over a Unix socket or the system D-Bus would send from here."
    echo
  } > "$file"
  if [ ! -f "$OUT/baseline-uplink.tsv" ]; then
    echo "No baseline was captured, so there is no such list." >> "$file"
    return 0
  fi
  awk -F '\t' 'FILENAME == ARGV[1] { seen[$3 FS $4 FS $5] = 1; next } !(($3 FS $4 FS $5) in seen)' \
    "$OUT/baseline-uplink.tsv" "$OUT/$phase-uplink.tsv" > "$OUT/$phase-suspects.tsv"
  awk 'FILENAME == ARGV[1] { seen[$2 " " $3] = 1; next } !(($2 " " $3) in seen)' \
    "$OUT/baseline-uplink-names.txt" "$OUT/$phase-uplink-names.txt" > "$OUT/$phase-suspect-names.txt"
  ends=$(lines "$OUT/$phase-suspects.tsv")
  names=$(lines "$OUT/$phase-suspect-names.txt")
  count=$((ends + names))
  echo "$count" > "$OUT/$phase-suspects.count"
  {
    if [ "$count" -eq 0 ]; then
      echo "None."
    fi
    if [ "$ends" -gt 0 ]; then
      echo "| Sent by | Remote end | Port | Protocol | Packets | First, from the phase's start |"
      echo "|---|---|---|---|---|---|"
      finding_rows "$start" "$OUT/$phase-suspects.tsv"
      echo
    fi
    if [ "$names" -gt 0 ]; then
      echo "DNS questions the host asked:"
      echo
      echo '```'
      cat "$OUT/$phase-suspect-names.txt"
      echo '```'
    fi
    echo
  } >> "$file"
}

# "127.0.0.1:4445 (WebDriver) 12, 127.0.0.1:9 (dead proxy) 3", or "none".
loopback_summary() {
  local out
  out=$(awk '{ dst = $5; sub(/:$/, "", dst); n = split(dst, a, "."); port = a[n]; sub(/\.[^.]*$/, "", dst); c[dst ":" port]++ }
    END { for (k in c) print k "\t" c[k] }' "$1" | sort |
    awk -F '\t' -v port="$PORT" '{
      name = ""
      if ($1 ~ (":" port "$")) name = " (WebDriver)"
      else if ($1 ~ /:9$/) name = " (dead proxy)"
      printf "%s%s`%s`%s %d", sep, "", $1, name, $2; sep = ", "
    }')
  echo "${out:-none}"
}

dns_lines() {
  local names
  names=$(cat "$@" | questions)
  [ -n "$names" ] || return 0
  echo
  echo "DNS questions among them:"
  echo
  echo '```'
  echo "$names"
  echo '```'
}

os_label() {
  echo "${MATRIX_OS:-$OS}"
}

# Prints the phase's table, the first findings in full, and fails on any.
# The list of what the baseline lacks only gets a warning.
report() {
  local phase=$1 md=$2 findings=$3 suspects=0 disclosed=0
  shift 3
  cat "$md"
  [ ! -f "$OUT/$phase-suspects.count" ] || suspects=$(cat "$OUT/$phase-suspects.count")
  if [ "$suspects" -gt 0 ]; then
    echo "::warning::S2.7 $phase: $suspects item(s) of traffic outside the process tree that the baseline lacks, listed above for review (not findings)"
  fi
  [ ! -f "$OUT/$phase-disclosed.count" ] || disclosed=$(cat "$OUT/$phase-disclosed.count")
  if [ "${disclosed:-0}" -gt 0 ]; then
    echo "::notice::S2.7 $phase: $disclosed packet(s) covered by the disclosed list (scripts/spikes/s2-7-disclosed.tsv), reported above, not findings"
  fi
  if [ "$findings" -gt 0 ]; then
    echo "The first findings in full:"
    # Not cat into head: under pipefail, cat writing to the pipe head has
    # closed fails the run.
    awk 'NR <= 60' "$@"
    case $phase in
      control) return 0 ;;
      baseline) fail "baseline: $findings packet(s) counted as the app's while it was not running, so the attribution would blame it for the machine's own traffic; see the table above" ;;
      *) fail "$phase: $findings packet(s) from the process tree; see the table above" ;;
    esac
  fi
  case $phase in
    control) ;;
    baseline) echo "baseline: nothing counted as the app's while it was not running." ;;
    *)
      if [ "$disclosed" -gt 0 ]; then
        echo "$phase: no packet from the process tree but the $disclosed the disclosed list covers."
      else
        echo "$phase: no packet from the process tree."
      fi
      ;;
  esac
}

# macOS: the capture as text, with pktap's metadata after the time:
# "<epoch> (<interface>, proc <name>:<pid>:<uuid>, eproc ..., svc .., dir, ..) IP ..."
mac_listing() {
  local pcap=$1 listing=$2
  if ! tcpdump -n -q -tt -k -r "$pcap" > "$listing" 2> "$listing.err"; then
    cat "$listing.err"
    fail "tcpdump could not read $pcap"
  fi
  rm -f "$listing.err"
}

# macOS: keeps the packets outside lo0 whose process or delegated process
# matches <want> (and, if given, has PID <pid>), or is one of MAC_DAEMONS
# with a PID not in MAC_EXCLUDED, as "<label>\t<line without the
# metadata>". The label names the process and the direction; an incoming
# packet's addresses are swapped, so that the line's destination is always
# the remote end. <mode> "lo0" keeps the ones on lo0 instead. <mode> "all"
# keeps every packet outside lo0, as "<identity>\t<1 if it matches, else
# 0>\t<label>\t<line>"; the identity is the process the packet was for
# (eproc) where pktap names one, else the one that sent or received it, cut
# to 15 characters, as pktap cuts eproc. <mode> "excluded" keeps the packets
# outside lo0 of the daemons in MAC_EXCLUDED, as "outside" does.
mac_select() {
  local listing=$1 want=$2 pid=$3 mode=${4:-outside}
  WANT=$want DAEMONS=$MAC_DAEMONS EXCLUDED=$MAC_EXCLUDED awk -v pid="$pid" -v mode="$mode" '
    function name(s, a) { split(s, a, ":"); return a[1] }
    function id(s, a) { split(s, a, ":"); return a[2] }
    function daemon(s) { return s != "" && ENVIRON["DAEMONS"] != "" && name(s) ~ ENVIRON["DAEMONS"] }
    function excluded(s) { return daemon(s) && index(ENVIRON["EXCLUDED"], " " id(s) " ") > 0 }
    function matches(s) {
      if (s == "") return 0
      if (name(s) ~ ENVIRON["WANT"] && (pid == "" || id(s) == pid)) return 1
      return daemon(s) && !excluded(s)
    }
    {
      if (!match($0, /\([^()]*\)/)) next
      meta = substr($0, RSTART + 1, RLENGTH - 2)
      rest = substr($0, RSTART + RLENGTH + 1)
      n = split(meta, part, ", ")
      ifname = part[1]
      sub(/^(if|ifname) /, "", ifname)
      proc = ""
      eproc = ""
      dir = ""
      for (i = 1; i <= n; i++) {
        if (part[i] ~ /^proc /) proc = substr(part[i], 6)
        else if (part[i] ~ /^eproc /) eproc = substr(part[i], 7)
        else if (part[i] == "in" || part[i] == "out") dir = part[i]
      }
      tree = matches(proc) || matches(eproc)
      if (mode == "excluded") {
        if (!excluded(proc) && !excluded(eproc)) next
      } else if (mode != "all" && !tree) next
      lo = (ifname == "lo0")
      if ((mode == "lo0") != lo) next
      label = name(proc) " (" id(proc) ")"
      if (eproc != "" && eproc != proc) label = label " for " name(eproc) " (" id(eproc) ")"
      if (dir != "") label = label ", " dir
      if (dir == "in") {
        m = split(rest, r, " ")
        if (m >= 4 && r[3] == ">") {
          # What follows "<family> <src> > <dst>:", from its leading space.
          tail = substr(rest, length(r[1] r[2] r[3] r[4]) + 4)
          sub(/:$/, "", r[4])
          rest = r[1] " " r[4] " > " r[2] ":" tail
        }
      }
      if (mode == "all") {
        who = (eproc != "") ? name(eproc) : name(proc)
        print substr(who, 1, 15) "\t" (tree ? 1 : 0) "\t" label "\t" $1 " " rest
      } else {
        print label "\t" $1 " " rest
      }
    }
  ' "$listing"
}

# "navaja (123), out 30; navaja (123), in 25", or "none".
lo0_summary() {
  local out
  out=$(awk -F '\t' '{ c[$1]++ } END { for (k in c) { printf "%s%s %d", sep, k, c[k]; sep = "; " } }' "$1")
  echo "${out:-none}"
}

# macOS: the full command of each process a findings file names, from the
# phase's process snapshots: pktap cuts names short.
mac_names() {
  local file=$1 snapshots=$2 pid cmd
  [ -f "$snapshots" ] || return 0
  for pid in $(grep -oE '\([0-9]+\)' "$file" | tr -d '()' | sort -un); do
    # ps prints PID, parent, the start time in 5 fields, then the command.
    cmd=$(awk -v pid="$pid" '$1 == pid { for (i = 1; i <= 7; i++) $i = ""; sub(/^ +/, ""); print; exit }' "$snapshots")
    [ -z "$cmd" ] || echo "- $pid: \`$cmd\`"
  done
}

# macOS: each process that sent or received outside lo0 in the phase, while
# the baseline had it silent, and not counted as the tree's, into
# <phase>-suspects.md and .count.
suspects_macos() {
  local phase=$1 start count file=$OUT/$1-suspects.md
  start=$(cat "$OUT/$phase-start")
  {
    echo
    echo "##### Outside the process tree, not in the baseline (for review, not a finding)"
    echo
    echo "Each process that sent or received packets outside lo0 in the phase, or had them sent for it, but none in the 5 min baseline. macOS starts and stops many daemons of its own, so most of these have nothing to do with the app."
    echo
  } > "$file"
  if [ ! -f "$OUT/baseline-identities.txt" ]; then
    echo "No baseline was captured, so there is no such list." >> "$file"
    return 0
  fi
  awk -F '\t' 'FILENAME == ARGV[1] { seen[$1] = 1; next } $2 == 0 && !($1 in seen)' \
    "$OUT/baseline-identities.txt" "$OUT/$phase-outside.txt" > "$OUT/$phase-suspects.txt"
  cut -f 3,4 "$OUT/$phase-suspects.txt" | group > "$OUT/$phase-suspects.tsv"
  count=$(cut -f 1 "$OUT/$phase-suspects.txt" | sort -u | awk 'END { print NR }')
  echo "$count" > "$OUT/$phase-suspects.count"
  if [ "$count" -eq 0 ]; then
    echo "None." >> "$file"
    return 0
  fi
  {
    echo "| For | Processes | Packets | Remote ends | First, from the phase's start |"
    echo "|---|---|---|---|---|"
    awk -F '\t' -v start="$start" '
      {
        who = $1
        if (!(who in packets)) { order[++k] = who; first[who] = $4 + 0 }
        packets[who]++
        proc = $3
        sub(/, (in|out)$/, "", proc)
        sub(/ for .*/, "", proc)
        if (!((who SUBSEP proc) in seenproc)) {
          seenproc[who SUBSEP proc] = 1
          if (nprocs[who]++ < 3) procs[who] = procs[who] (procs[who] == "" ? "" : ", ") proc
        }
        split($4, f, " ")
        end = f[5]
        sub(/:$/, "", end)
        proto = f[6]
        sub(/,$/, "", proto)
        if (proto == "tcp" || proto == "UDP" || proto == "udp") sub(/\.[^.]*$/, "", end)
        if (!((who SUBSEP end) in seenend)) {
          seenend[who SUBSEP end] = 1
          if (nends[who]++ < 3) ends[who] = ends[who] (ends[who] == "" ? "" : ", ") end
        }
      }
      END {
        for (i = 1; i <= k; i++) {
          who = order[i]
          more = (nprocs[who] > 3) ? sprintf(" and %d more", nprocs[who] - 3) : ""
          moreends = (nends[who] > 3) ? sprintf(" and %d more", nends[who] - 3) : ""
          printf "| %s | %s%s | %d | %d: %s%s | +%.1f s |\n", who, procs[who], more, packets[who], nends[who], ends[who], moreends, first[who] - start
        }
      }
    ' "$OUT/$phase-suspects.txt"
    echo
  } >> "$file"
}

# The disclosed list's entries for an OS and a phase, as "<service>\t<hosts>
# \t<purpose>" lines.
disclosed_entries() {
  # Errors go to stderr: the caller writes stdout to a file.
  [ -f "$DISCLOSED" ] || fail "no disclosed list at $DISCLOSED" >&2
  awk -F '\t' -v os="$1" -v phase="$2" '
    /^#/ || NF == 0 { next }
    NF != 5 { print "s2-7-disclosed.tsv line " NR ": " NF " fields, not 5" > "/dev/stderr"; bad = 1; next }
    $1 == os && index(" " $2 " ", " " phase " ") { print $3 "\t" $4 "\t" $5 }
    END { exit bad }
  ' "$DISCLOSED" || fail "the disclosed list is malformed" >&2
}

# macOS: "<key>\t<name>" for every address the job's captures saw a DNS
# answer give, and for every name in those answers, with every name that
# leads to it: the record's own name and the names whose CNAMEs lead there.
# A lookup of a CNAME's target (a1845.dscg2.akamai.net) is then tied to the
# name first asked for (ocsp2.apple.com). Every phase so far counts, since
# mDNSResponder may answer a name from its cache, CNAME included.
mac_addr_names() {
  local file
  for file in "$OUT"/*-dns-long.txt; do
    [ ! -f "$file" ] || cat "$file"
  done | awk '
    # An answer, one a line: "... <id> <an>/<ns>/<ar> <name>. <TYPE> <data>, ... (<length>)"
    match($0, / [0-9]+\/[0-9]+\/[0-9]+ /) {
      rest = substr($0, RSTART + RLENGTH)
      sub(/ \([0-9]+\)$/, "", rest)
      n = split(rest, record, ", ")
      for (i = 1; i <= n; i++) {
        split(record[i], f, " ")
        owner = f[1]
        data = f[3]
        sub(/\.$/, "", owner)
        sub(/\.$/, "", data)
        if (f[2] == "CNAME") {
          parents[data] = parents[data] " " owner
          named[data] = 1
          named[owner] = 1
        } else if (f[2] == "A" || f[2] == "AAAA") {
          owners[data] = owners[data] " " owner
          named[owner] = 1
        }
      }
    }
    function walk(name, depth, list, k, m) {
      if (depth > 10 || (name in seen)) return
      seen[name] = 1
      print key "\t" name
      m = split(parents[name], list, " ")
      for (k = 1; k <= m; k++) walk(list[k], depth + 1)
    }
    END {
      for (key in owners) {
        for (name in seen) delete seen[name]
        n = split(owners[key], names, " ")
        for (i = 1; i <= n; i++) walk(names[i], 0)
      }
      for (key in named) {
        for (name in seen) delete seen[name]
        walk(key, 0)
      }
    }
  ' | sort -u > "$OUT/addr-names.tsv"
}

# macOS: splits the tree's packets (<phase>-tree.txt) into those an entry of
# the disclosed list covers, "<entry>\t<label>\t<line>" in
# <phase>-disclosed.txt, and the rest, "<label>\t<line>" in
# <phase>-undisclosed.txt. A packet is covered when the process it was for
# (pktap's eproc, else its proc) is the entry's service, and one of the
# entry's hosts is the name its DNS packet asks for (a DNS answer is matched
# to its question by port and ID) or a name the captures' DNS answers give
# for its remote address, through CNAMEs too (mac_addr_names). A packet sent
# by or for a process of the tree (TREE_PROCS) is never covered.
mac_disclose() {
  local phase=$1
  mac_addr_names
  disclosed_entries macos "$phase" > "$OUT/$phase-disclosed-entries.tsv"
  : > "$OUT/$phase-disclosed.txt"
  : > "$OUT/$phase-undisclosed.txt"
  TREE=$TREE_PROCS awk -F '\t' -v entries="$OUT/$phase-disclosed-entries.tsv" -v names="$OUT/addr-names.tsv" \
    -v covered="$OUT/$phase-disclosed.txt" -v rest="$OUT/$phase-undisclosed.txt" '
    function port(end, n, p) { n = split(end, p, "."); return p[n] }
    function host(end) { sub(/\.[^.]*$/, "", end); return end }
    BEGIN {
      while ((getline line < entries) > 0) {
        split(line, e, "\t")
        entry[++count] = e[1] " -> " e[2]
        service[count] = substr(e[1], 1, 15)
        hosts[count] = e[2]
      }
      while ((getline line < names) > 0) {
        split(line, a, "\t")
        known[a[1]] = known[a[1]] " " a[2] " "
      }
    }
    # The phase'"'"'s DNS packets in long form: "<epoch> (<pktap>) IP (<ip>)
    # <source> > <destination>: <id>[flags] <question or answers>".
    FILENAME == ARGV[1] {
      n = split($0, w, " ")
      for (i = 2; i < n; i++) if (w[i] == ">") break
      if (i >= n) next
      source = w[i - 1]
      target = w[i + 1]
      sub(/:$/, "", target)
      id = w[i + 2]
      sub(/[^0-9].*$/, "", id)
      if (port(target) == "53") {
        question = ""
        for (k = i + 3; k < n; k++) if (w[k] ~ /\?$/) { question = w[k + 1]; break }
        sub(/\.$/, "", question)
        asked[port(source) "/" id] = question
        dns[w[1] " " port(source)] = question
      } else if (port(source) == "53") {
        answer[w[1] " " port(target)] = port(target) "/" id
      }
      next
    }
    # The tree'"'"'s packets, short form, remote end last: "<label>\t<epoch> IP
    # <local> > <remote>: ...".
    {
      label = $1
      split($2, f, " ")
      local = f[3]
      remote = f[5]
      sub(/:$/, "", remote)
      who = label
      if (who ~ / for /) sub(/.* for /, "", who)
      sub(/ \(.*/, "", who)
      sender = label
      sub(/ \(.*/, "", sender)
      # The tree'"'"'s own packets are never disclosed, whatever an entry says.
      if (sender ~ ENVIRON["TREE"] || who ~ ENVIRON["TREE"]) {
        print $0 > rest
        next
      }
      if (port(remote) == "53") {
        key = f[1] " " port(local)
        name = (key in dns) ? dns[key] : asked[answer[key]]
        seen_as = " " name " " known[name]
      } else {
        seen_as = known[host(remote)]
      }
      match_entry = 0
      for (i = 1; i <= count && !match_entry; i++) {
        if (substr(who, 1, 15) != service[i]) continue
        m = split(hosts[i], h, ",")
        for (j = 1; j <= m; j++) {
          gsub(/^ +| +$/, "", h[j])
          if (h[j] != "" && index(seen_as, " " h[j] " ")) match_entry = i
        }
      }
      if (match_entry) print entry[match_entry] "\t" $0 > covered
      else print $0 > rest
    }
  ' "$OUT/$phase-dns-long.txt" "$OUT/$phase-tree.txt"
}

# Markdown rows for the disclosed packets: entry, packets, first seen.
disclosed_rows() {
  local start=$1 file=$2
  awk -F '\t' -v start="$start" '
    { if (!($1 in count)) { order[++k] = $1; first[$1] = $3 + 0 } count[$1]++ }
    END { for (i = 1; i <= k; i++) printf "| %s | %d | +%.1f s |\n", order[i], count[order[i]], first[order[i]] - start }
  ' "$file"
}

# check_macos <phase> <process regex> <pid or empty>
check_macos() {
  local phase=$1 want=$2 pid=$3 pcap=$OUT/$1.pcapng listing=$OUT/$1-packets.txt
  local start total outside tree lo0 canary findings disclosed md=$OUT/$1-findings.md tsv=$OUT/$1-findings.tsv
  [ -f "$pcap" ] || fail "check $phase: no capture; run the $phase mode first"
  start=$(cat "$OUT/$phase-start")
  mac_listing "$pcap" "$listing"
  echo "The capture as tcpdump prints it, first lines:"
  head -3 "$listing"
  total=$(lines "$listing")
  outside=$(grep -cvE '^[0-9.]+ \(lo0' "$listing" || true)
  mac_select "$listing" "$want" "$pid" > "$OUT/$phase-tree.txt"
  mac_select "$listing" "$want" "$pid" lo0 > "$OUT/$phase-tree-lo0.txt"
  tree=$(lines "$OUT/$phase-tree.txt")
  lo0=$(lines "$OUT/$phase-tree-lo0.txt")
  # The canary's names, whichever process asks: a lookup the resolver makes
  # for a process it does not name would still show here. One line a packet.
  packets "$pcap" 'port 53' "$OUT/$phase-dns-long.txt" -k -v
  join_lines "$OUT/$phase-dns-long.txt"
  grep -F "$CANARY" "$OUT/$phase-dns-long.txt" > "$OUT/$phase-canary.txt" || true
  canary=$(lines "$OUT/$phase-canary.txt")
  # Packets, each once: a canary lookup made for the tree is in both lists,
  # so a canary packet counts only if no packet of the tree has its time
  # stamp. (Time stamps alone can't be counted: pktap gives a burst of
  # packets one time stamp.)
  cut -f 2 "$OUT/$phase-tree.txt" | awk '{ print $1 }' > "$OUT/$phase-tree-times.txt"
  awk 'FILENAME == ARGV[1] { seen[$1] = 1; next } !($1 in seen)' \
    "$OUT/$phase-tree-times.txt" "$OUT/$phase-canary.txt" > "$OUT/$phase-canary-other.txt"
  # What the disclosed list covers is reported apart and fails nothing. The
  # control's request and the canary's names never are.
  if [ "$phase" = control ]; then
    cp "$OUT/$phase-tree.txt" "$OUT/$phase-undisclosed.txt"
    : > "$OUT/$phase-disclosed.txt"
  else
    mac_disclose "$phase"
  fi
  disclosed=$(lines "$OUT/$phase-disclosed.txt")
  echo "$disclosed" > "$OUT/$phase-disclosed.count"
  findings=$(($(lines "$OUT/$phase-undisclosed.txt") + $(lines "$OUT/$phase-canary-other.txt")))
  group "$OUT/$phase-undisclosed.txt" > "$tsv"
  # The tree's DNS packets in long form, which shows their questions.
  mac_select "$OUT/$phase-dns-long.txt" "$want" "$pid" | cut -f 2 > "$OUT/$phase-tree-dns.txt"

  {
    echo "#### $phase: $(os_label)"
    echo
    echo "| Capture | Packets, all processes | Outside lo0 | From the process tree, outside lo0 | From the tree on lo0 | DNS for the canary's hosts, any process | Disclosed | Findings (packets, each once) |"
    echo "|---|---|---|---|---|---|---|---|"
    echo "| pktap, every interface | $total | $outside | $tree | $lo0 | $canary | $disclosed | $findings |"
    echo
    echo "The tree's packets on lo0, which never leave the machine: $(lo0_summary "$OUT/$phase-tree-lo0.txt")."
    echo
    if [ "$phase" != control ]; then
      mac_select "$listing" "$want" "$pid" excluded > "$OUT/$phase-daemons-left-out-packets.txt"
      echo "WebKit daemons in the job's process snapshots that count as the app's, having started within $DAEMON_WINDOW s after it: $(daemon_names "$phase" counted)."
      echo
      echo "Left out, not the app's: $(daemon_names "$phase" 'left out'). Their packets outside lo0 in this phase: $(lines "$OUT/$phase-daemons-left-out-packets.txt")."
      echo
    fi
    if [ "$disclosed" -gt 0 ]; then
      echo "Disclosed, not findings: packets that an entry of \`scripts/spikes/s2-7-disclosed.tsv\` covers in this phase (docs/adr/0003-webview-network.md)."
      echo
      echo "| Entry (service -> hosts) | Packets | First, from the phase's start |"
      echo "|---|---|---|"
      disclosed_rows "$start" "$OUT/$phase-disclosed.txt"
      echo
    fi
    if [ "$findings" -eq 0 ] && [ "$disclosed" -gt 0 ]; then
      echo "No other packet from the process tree."
    elif [ "$findings" -eq 0 ]; then
      echo "No packet from the process tree."
    else
      echo "| Process | Remote end | Port | Protocol | Packets | First, from the phase's start |"
      echo "|---|---|---|---|---|---|"
      finding_rows "$start" "$tsv"
      dns_lines "$OUT/$phase-tree-dns.txt" "$OUT/$phase-canary-other.txt"
      if [ -s "$OUT/$phase-processes.txt" ]; then
        echo
        echo "The processes named above, from the phase's process snapshots:"
        echo
        mac_names "$tsv" "$OUT/$phase-processes.txt"
      fi
    fi
    echo
  } > "$md"
  # The app's own WebDriver traffic on lo0 shows that pktap saw the app and
  # named it.
  if { [ "$phase" = idle ] || [ "$phase" = in-use ]; } && [ "$lo0" -eq 0 ]; then
    cat "$md"
    fail "$phase: pktap named no packet as the app's, not even its WebDriver traffic on lo0, so the capture proves nothing"
  fi
  case $phase in
    baseline)
      mac_select "$listing" "$TREE_PROCS" '' all > "$OUT/$phase-outside.txt"
      cut -f 1 "$OUT/$phase-outside.txt" | sort -u > "$OUT/baseline-identities.txt"
      echo "baseline: $(lines "$OUT/baseline-identities.txt") process(es) sent or received packets outside lo0, or had them sent for them."
      ;;
    idle | in-use)
      mac_select "$listing" "$TREE_PROCS" '' all > "$OUT/$phase-outside.txt"
      suspects_macos "$phase"
      cat "$OUT/$phase-suspects.md" >> "$md"
      ;;
  esac
  report "$phase" "$md" "$findings" "$OUT/$phase-undisclosed.txt" "$OUT/$phase-canary-other.txt"
}

# --- Control ------------------------------------------------------------------

# A request the capture must see and attribute, through the same checks as
# the phases: if it doesn't, an empty phase capture would prove nothing.
control() {
  case $OS in
    linux) control_linux ;;
    macos) control_macos ;;
  esac
}

control_linux() {
  local tsv=$OUT/control-findings.tsv status=0
  start_captures control
  echo "In the namespace, as $(id -un):"
  # shellcheck disable=SC2016
  in_ns bash -c '
    set -e
    getent ahosts github.com | sed -n 1,3p
    code=$(curl -sS -o /dev/null -w "%{http_code}" --max-time 30 https://github.com)
    echo "curl https://github.com: HTTP $code"
    case $code in 2* | 3*) ;; *) exit 1 ;; esac
    # A DNS query to the loopback stub address, which only the loopback
    # capture can see.
    printf "\0" > /dev/udp/127.0.0.53/53
  ' || status=$?
  # ICMP echo, which the noise filter must not swallow.
  ns ping -c 1 -W 2 "$NET4.1" > /dev/null || status=1
  ns ping -6 -c 1 -W 2 "${NET6}1" > /dev/null || status=1
  settle
  stop_captures
  capture_counts control
  [ "$status" -eq 0 ] || fail "control: the requests from the namespace failed; it has no working route or resolver"
  check_linux control
  expect "$tsv" "a DNS query to $RESOLVER" "\tUDP\t" "\t$RESOLVER\t53\t"
  expect "$tsv" "a TCP connection to port 443" "namespace, veth" "\t443\ttcp\t"
  expect "$tsv" "an ICMP echo to $NET4.1" "\tICMP\t" "\t$NET4.1\t"
  expect "$tsv" "an ICMPv6 echo to ${NET6}1" "\tICMP6\t" "\t${NET6}1\t"
  expect "$tsv" "the DNS query on the loopback" "loopback DNS" "\t127.0.0.53\t53\t"
  echo "Control: the capture saw each request leave the namespace, and the check reported each one."
}

# expect <tsv> <what> <pattern> <pattern>: a row matching both must exist.
expect() {
  local file=$1 what=$2 one two
  one=$(printf '%b' "$3")
  two=$(printf '%b' "$4")
  grep -F -- "$one" "$file" | grep -F -- "$two" > /dev/null ||
    fail "control: the check did not report $what, so the capture can't be trusted"
}

control_macos() {
  local pid code tsv=$OUT/control-findings.tsv
  start_captures control
  # A cold cache, so curl's lookup goes out.
  sudo dscacheutil -flushcache
  sudo killall -HUP mDNSResponder
  sleep 1
  curl -sS -o /dev/null -w '%{http_code}' --max-time 30 https://github.com > "$OUT/control-curl.txt" &
  pid=$!
  wait "$pid" || fail "control: curl https://github.com failed"
  code=$(cat "$OUT/control-curl.txt")
  echo "curl (PID $pid) https://github.com: HTTP $code"
  sleep 2
  stop_captures
  capture_counts control
  check_macos control '^curl$' "$pid"
  expect "$tsv" "curl's TCP connection to port 443" "curl ($pid)" "\t443\ttcp\t"
  # pktap names the process a lookup was made for (eproc), not only
  # mDNSResponder, which sends it.
  grep -F "($pid)" "$tsv" | grep -E $'\t53\tUDP\t|\t53\ttcp\t' > /dev/null ||
    fail "control: no DNS packet attributed to curl ($pid); a lookup by the tree would go unseen, so the capture can't be trusted"
  echo "Control: pktap attributed curl's connection and its DNS lookup to curl."
}

# --- Phases -------------------------------------------------------------------

# The same captures as idle, as long, before the app ever runs on this
# machine, so that the daemons it starts are not running yet.
baseline() {
  local process
  process=$(pgrep -fl "$BINARY" || true)
  [ -z "$process" ] || fail "baseline: the app already runs, so this is no baseline: $process"
  record_daemons baseline
  start_captures baseline
  watch_processes baseline
  echo "Baseline: capturing $IDLE_SECONDS s with nothing of Navaja running."
  sleep "$IDLE_SECONDS"
  stop_watch
  stop_captures
  capture_counts baseline
  echo "Baseline: done; the capture is checked in the next step."
}

# Linux: after a phase, outside its capture, the namespace must still reach
# the network. Had its veth, route or NAT stopped working, a send from the
# app would have failed inside the namespace (no route) without putting a
# packet on the veth, and the phase would look clean.
namespace_alive() {
  local phase=$1 status=0 link route
  rm -f "$OUT/$phase-no-way-out"
  link=$(ns ip -o link show "$NS_IF" 2>&1 || true)
  route=$(ns ip route show default 2>&1 || true)
  echo "$NS_IF: $link"
  echo "Default route: $route"
  [[ $link == *"state UP"* ]] || status=1
  [[ $route == *"via $NET4.1 "* ]] || status=1
  ns ping -c 1 -W 2 "$NET4.1" > /dev/null || status=1
  # shellcheck disable=SC2016
  in_ns bash -c '
    getent ahosts github.com > /dev/null &&
      curl -sS -o /dev/null --max-time 30 https://github.com
  ' || status=1
  if [ "$status" -ne 0 ]; then
    : > "$OUT/$phase-no-way-out"
    echo "::error::S2.7 $phase: after the phase the namespace no longer reaches the network: $NS_IF, its default route, a ping to $NET4.1 or a request to github.com failed"
    return 1
  fi
  echo "After the phase, outside its capture, the namespace still reached github.com (DNS through $RESOLVER, TCP to port 443)."
}

idle() {
  local status=0 alive=0
  [ -x "$BINARY" ] || fail "idle: no $BINARY; build it first (see the top of this script)"
  record_daemons idle
  start_captures idle
  watch_processes idle
  case $OS in
    linux)
      in_ns env WEBKIT_DISABLE_DMABUF_RENDERER=1 dbus-run-session -- \
        xvfb-run -a -s '-screen 0 1280x800x24' bash "$SELF" idle-app || status=$?
      ;;
    macos) bash "$SELF" idle-app || status=$? ;;
  esac
  settle
  stop_watch
  stop_captures
  capture_counts idle
  [ "$OS" != linux ] || namespace_alive idle || alive=1
  [ "$status" -eq 0 ] || fail "idle: the app did not idle as planned (exit $status); see above"
  [ "$alive" -eq 0 ] || fail "idle: the namespace lost its way out; see above"
  echo "Idle: done; the capture is checked in the next step."
}

in_use() {
  local status=0 alive=0
  [ -x "$BINARY" ] || fail "in use: no $BINARY; build it first (see the top of this script)"
  record_daemons in-use
  start_captures in-use
  watch_processes in-use
  case $OS in
    linux)
      # As ci.yml runs the suite. pnpm makes no update check (it skips it
      # on CI anyway), so nothing but the app's tree has a reason to talk.
      in_ns env WEBKIT_DISABLE_DMABUF_RENDERER=1 npm_config_update_notifier=false dbus-run-session -- \
        xvfb-run -a bash "$SELF" in-use-app || status=$?
      ;;
    macos) bash "$SELF" in-use-app || status=$? ;;
  esac
  settle
  stop_watch
  stop_captures
  capture_counts in-use
  [ "$OS" != linux ] || namespace_alive in-use || alive=1
  [ "$status" -eq 0 ] || fail "in use: the end-to-end suite failed (exit $status)"
  [ "$alive" -eq 0 ] || fail "in use: the namespace lost its way out; see above"
  echo "In use: the suite passed; the capture is checked in the next step."
}

alive() {
  [ -n "$APP_PID" ] && kill -0 "$APP_PID" 2> /dev/null
}

stop_app() {
  local i
  [ -n "$APP_PID" ] || return 0
  kill -TERM "$APP_PID" 2> /dev/null || true
  for ((i = 0; i < 50; i++)); do
    alive || break
    sleep 0.2
  done
  kill -KILL "$APP_PID" 2> /dev/null || true
  wait "$APP_PID" 2> /dev/null || true
  APP_PID=''
}

# Whether the app's window is on screen.
window_shown() {
  local when=$1 ids id
  case $OS in
    linux)
      ids=$(xwininfo -root -tree | awk '/"Navaja"/ { print $1 }')
      for id in $ids; do
        if xwininfo -id "$id" | grep 'Map State: IsViewable' > /dev/null; then
          echo "Window ($when): $(xwininfo -id "$id" | grep -E 'xwininfo|Width|Height' | tr -s ' \n' ' ')"
          return 0
        fi
      done
      echo "No viewable Navaja window ($when). The windows:"
      xwininfo -root -tree | awk 'NR <= 40'
      return 1
      ;;
    macos)
      # The window server's list, without the Accessibility permission that
      # System Events needs: on-screen, normal-level windows of the app.
      local js=$OUT/windows.js found
      cat > "$js" << 'EOF'
ObjC.import('CoreGraphics');
function run(argv) {
  const pid = Number(argv[0]);
  // kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements
  const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1 | 16, 0))) || [];
  return JSON.stringify(list.filter((w) => w.kCGWindowOwnerPID === pid && w.kCGWindowLayer === 0)
    .map((w) => ({ bounds: w.kCGWindowBounds, onscreen: w.kCGWindowIsOnscreen, alpha: w.kCGWindowAlpha })));
}
EOF
      found=$(with_timeout 30 osascript -l JavaScript "$js" "$APP_PID" || true)
      echo "Windows of PID $APP_PID ($when): $found"
      [ "$found" != '[]' ] && [ -n "$found" ]
      ;;
  esac
}

screenshot() {
  case $OS in
    linux) timeout 30 import -window root "$1" ;;
    # Without the Screen Recording permission it may show the desktop only.
    macos) with_timeout 30 screencapture -x "$1" ;;
  esac
}

# The idle phase, as the app's user (inside the namespace, D-Bus session and
# virtual display on Linux): the app starts as @wdio/tauri-service starts it,
# runs for IDLE_SECONDS with its window shown and nothing driving it, and is
# stopped.
idle_app() {
  local app_dir log=$OUT/idle-app.log i started
  [ "$OS" != linux ] || assert_in_namespace
  ! port_open || fail "idle: 127.0.0.1:$PORT already accepts connections; stop whatever holds it"
  app_dir=$(mktemp -d)
  # The harness's launch (wdio.conf.ts, @wdio/tauri-service's embedded
  # driver): --tool uuid, a fresh app directory, and its WebDriver server.
  NAVAJA_APP_DIR=$app_dir TAURI_WEBDRIVER_PORT=$PORT WDIO_EMBEDDED_SERVER=true \
    "$BINARY" --tool uuid > "$log" 2>&1 &
  APP_PID=$!
  echo "The app started (PID $APP_PID)."
  # Like the harness, wait for the WebDriver server to report ready.
  for ((i = 0; ; i++)); do
    alive || {
      cat "$log"
      fail "idle: the app exited during startup"
    }
    curl -sf --max-time 5 "http://127.0.0.1:$PORT/status" 2> /dev/null | grep -E '"ready": ?true' > /dev/null && break
    [ "$i" -lt 120 ] || fail "idle: the WebDriver server did not report ready within 60 s"
    sleep 0.5
  done
  started=$(now)
  echo "WebDriver ready; idling for $IDLE_SECONDS s with nothing driving the app."
  # The window shows once the front end has rendered (shell_ready).
  sleep 5
  window_shown start || fail "idle: the app's window is not shown"
  app_processes > "$OUT/idle-tree.txt"
  echo "The app's processes:"
  cat "$OUT/idle-tree.txt"
  while [ $(($(now) - started)) -lt "$IDLE_SECONDS" ]; do
    alive || fail "idle: the app exited after $(($(now) - started)) s"
    sleep 10
  done
  alive || fail "idle: the app exited"
  window_shown end || fail "idle: the app's window is no longer shown"
  screenshot "$OUT/idle-screen.png" || echo "::warning::S2.7 idle: no screenshot"
  stop_app
  echo "The app ran $(($(now) - started)) s after WebDriver was ready, its window shown, and was stopped."
  echo "Its output:"
  sed 's/^/  | /' "$log"
}

in_use_app() {
  [ "$OS" != linux ] || assert_in_namespace
  if [ "$OS" = linux ]; then
    bash app/e2e/strace-guard.sh
  else
    pnpm e2e
  fi
}

case "${1:-}" in
  setup) setup ;;
  control) control ;;
  baseline) baseline ;;
  idle) idle ;;
  idle-app) idle_app ;;
  in-use) in_use ;;
  in-use-app) in_use_app ;;
  check)
    case "${2:-}" in
      baseline | idle | in-use) check "$2" ;;
      *)
        echo "usage: $0 check baseline|idle|in-use" >&2
        exit 2
        ;;
    esac
    ;;
  teardown) teardown ;;
  *)
    echo "usage: $0 setup|control|baseline|check baseline|idle|check idle|in-use|check in-use|teardown" >&2
    exit 2
    ;;
esac
