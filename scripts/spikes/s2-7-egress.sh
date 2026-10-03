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
#   bash scripts/spikes/s2-7-egress.sh setup         # Linux: the network namespace; Windows: auditing
#   bash scripts/spikes/s2-7-egress.sh control       # a known request must be captured and attributed
#   bash scripts/spikes/s2-7-egress.sh idle          # the app idles 5 min, window shown, under capture
#   bash scripts/spikes/s2-7-egress.sh check idle    # what the capture holds from the process tree
#   bash scripts/spikes/s2-7-egress.sh in-use        # the end-to-end suite under capture
#   bash scripts/spikes/s2-7-egress.sh check in-use
#   bash scripts/spikes/s2-7-egress.sh teardown      # undoes setup
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
# - macOS: tcpdump on pktap, which tags each packet with the process that
#   sent or received it, and the process it acted for. A packet outside lo0
#   whose process is navaja or one of WebKit's (Networking, WebContent, GPU)
#   is a finding.
# On both, a DNS question for one of the egress canary's hosts is a finding
# too, whichever process asks.
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

# macOS: the processes of the tree, as pktap names them. It cuts a name to
# 16 characters, so com.apple.WebKit.Networking may read com.apple.WebKit.
TREE_PROCS='^(navaja|com\.apple\.WebKit.*)$'

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
  ip -o -4 addr show dev "$NS_IF" 2> /dev/null | grep -qF "$NET4.2/" ||
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

# The app's processes and WebKit's, without sudo.
app_processes() {
  local list
  case $OS in
    linux) list=$(ps -eo pid=,ppid=,lstart=,args=) ;;
    macos) list=$(ps -axo pid=,ppid=,lstart=,command=) ;;
  esac
  printf '%s\n' "$list" | grep -E 'navaja|com\.apple\.WebKit|WebKit(Network|Web|GPU)Process' || true
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
      tcpdump --version 2>&1 | head -3
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
  ns cat "/sys/class/net/$NS_IF/address" > "$OUT/namespace-mac"
  echo "Namespace $NS (uplink $uplink, NAT for $NET4.0/24):"
  ns ip -br addr
  ns ip route
  ns ip -6 route
  echo "resolv.conf: $(ns cat /etc/resolv.conf | tr '\n' ' ')"
  echo "nsswitch.conf: $(ns grep '^hosts:' /etc/nsswitch.conf)"
  tcpdump --version 2>&1 | head -2
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

# check <phase>: lists what the phase's capture holds from the process tree,
# writes <phase>-findings.md and -findings.tsv, and fails on any finding. The
# control calls it too, where findings are expected.
check() {
  local phase=$1
  case $OS in
    linux) check_linux "$phase" ;;
    macos) check_macos "$phase" "$TREE_PROCS" '' ;;
  esac
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
  if [ "$phase" != control ] && [ "$webdriver" -eq 0 ]; then
    cat "$md"
    fail "$phase: no connection to the app's WebDriver on the namespace's loopback; the app did not run in this namespace, so the capture proves nothing"
  fi
  report "$phase" "$md" "$findings" "$OUT/$phase-veth-sent-long.txt" "$OUT/$phase-lo-dns-long.txt"
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
report() {
  local phase=$1 md=$2 findings=$3
  shift 3
  cat "$md"
  if [ "$findings" -gt 0 ]; then
    echo "The first findings in full:"
    cat "$@" | head -60
    [ "$phase" = control ] && return 0
    fail "$phase: $findings packet(s) from the process tree; see the table above"
  fi
  [ "$phase" = control ] || echo "$phase: no packet from the process tree."
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
# matches <want> (and, if given, has PID <pid>), as "<label>\t<line without
# the metadata>". The label names the process and the direction; an
# incoming packet's addresses are swapped, so that the line's destination is
# always the remote end. <mode> "lo0" keeps the ones on lo0 instead.
mac_select() {
  local listing=$1 want=$2 pid=$3 mode=${4:-outside}
  WANT=$want awk -v pid="$pid" -v mode="$mode" '
    function name(s, a) { split(s, a, ":"); return a[1] }
    function id(s, a) { split(s, a, ":"); return a[2] }
    function matches(s) { return s != "" && name(s) ~ ENVIRON["WANT"] && (pid == "" || id(s) == pid) }
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
      if (!matches(proc) && !matches(eproc)) next
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
      print label "\t" $1 " " rest
    }
  ' "$listing"
}

# "navaja (123), out 30; navaja (123), in 25", or "none".
lo0_summary() {
  local out
  out=$(awk -F '\t' '{ c[$1]++ } END { for (k in c) { printf "%s%s %d", sep, k, c[k]; sep = "; " } }' "$1")
  echo "${out:-none}"
}

# check_macos <phase> <process regex> <pid or empty>
check_macos() {
  local phase=$1 want=$2 pid=$3 pcap=$OUT/$1.pcapng listing=$OUT/$1-packets.txt
  local start total outside tree lo0 canary findings md=$OUT/$1-findings.md tsv=$OUT/$1-findings.tsv
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
  # for a process it does not name would still show here.
  packets "$pcap" 'port 53' "$OUT/$phase-dns-long.txt" -k -v
  grep -F "$CANARY" "$OUT/$phase-dns-long.txt" > "$OUT/$phase-canary.txt" || true
  canary=$(lines "$OUT/$phase-canary.txt")
  findings=$((tree + canary))
  group "$OUT/$phase-tree.txt" > "$tsv"
  # The same packets in long form (DNS questions), by time stamp.
  cut -f 2 "$OUT/$phase-tree.txt" | awk '{ print $1 }' > "$OUT/$phase-tree-times.txt"
  grep -F -f "$OUT/$phase-tree-times.txt" "$OUT/$phase-dns-long.txt" > "$OUT/$phase-tree-dns.txt" 2> /dev/null || true

  {
    echo "#### $phase: $(os_label)"
    echo
    echo "| Capture | Packets, all processes | Outside lo0 | From the process tree, outside lo0 (findings) | From the tree on lo0 | DNS for the canary's hosts, any process |"
    echo "|---|---|---|---|---|---|"
    echo "| pktap, every interface | $total | $outside | $tree | $lo0 | $canary |"
    echo
    echo "The tree's packets on lo0, which never leave the machine: $(lo0_summary "$OUT/$phase-tree-lo0.txt")."
    echo
    if [ "$findings" -eq 0 ]; then
      echo "No packet from the process tree."
    else
      echo "| Process | Remote end | Port | Protocol | Packets | First, from the phase's start |"
      echo "|---|---|---|---|---|---|"
      finding_rows "$start" "$tsv"
      dns_lines "$OUT/$phase-tree-dns.txt" "$OUT/$phase-canary.txt"
    fi
    echo
  } > "$md"
  # The app's own WebDriver traffic on lo0 shows that pktap saw the app and
  # named it.
  if [ "$phase" != control ] && [ "$lo0" -eq 0 ]; then
    cat "$md"
    fail "$phase: pktap named no packet as the app's, not even its WebDriver traffic on lo0, so the capture proves nothing"
  fi
  report "$phase" "$md" "$findings" "$OUT/$phase-tree.txt" "$OUT/$phase-canary.txt"
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
    getent ahosts github.com | head -3
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
  grep -F -- "$one" "$file" | grep -qF -- "$two" ||
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
  grep -F "($pid)" "$tsv" | grep -qE $'\t53\tUDP\t|\t53\ttcp\t' ||
    fail "control: no DNS packet attributed to curl ($pid); a lookup by the tree would go unseen, so the capture can't be trusted"
  echo "Control: pktap attributed curl's connection and its DNS lookup to curl."
}

# --- Phases -------------------------------------------------------------------

idle() {
  local status=0
  [ -x "$BINARY" ] || fail "idle: no $BINARY; build it first (see the top of this script)"
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
  [ "$status" -eq 0 ] || fail "idle: the app did not idle as planned (exit $status); see above"
  echo "Idle: done; the capture is checked in the next step."
}

in_use() {
  local status=0
  [ -x "$BINARY" ] || fail "in use: no $BINARY; build it first (see the top of this script)"
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
  [ "$status" -eq 0 ] || fail "in use: the end-to-end suite failed (exit $status)"
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
        if xwininfo -id "$id" | grep -q 'Map State: IsViewable'; then
          echo "Window ($when): $(xwininfo -id "$id" | grep -E 'xwininfo|Width|Height' | tr -s ' \n' ' ')"
          return 0
        fi
      done
      echo "No viewable Navaja window ($when). The windows:"
      xwininfo -root -tree | head -40
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
    curl -sf --max-time 5 "http://127.0.0.1:$PORT/status" 2> /dev/null | grep -qE '"ready": ?true' && break
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
  idle) idle ;;
  idle-app) idle_app ;;
  in-use) in_use ;;
  in-use-app) in_use_app ;;
  check)
    case "${2:-}" in
      idle | in-use) check "$2" ;;
      *)
        echo "usage: $0 check idle|in-use" >&2
        exit 2
        ;;
    esac
    ;;
  teardown) teardown ;;
  *)
    echo "usage: $0 setup|control|idle|check idle|in-use|check in-use|teardown" >&2
    exit 2
    ;;
esac
