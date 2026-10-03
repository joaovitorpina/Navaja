#!/usr/bin/env bash
# Spike S2.3 (docs/spikes.md): the macOS half of the tray-icon check in
# .github/workflows/spikes.yml. It runs the app as shipped, captures the menu
# bar in the light and the dark appearance, and checks that the template icon
# is drawn, monochrome, and tinted like the system's own menu bar items.
# Windows' half is s2-3-tray.ps1.
#
# Usage, on macOS, from anywhere in the repository, after
# `pnpm tauri build --debug --no-bundle` (the shipped configuration):
#   bash scripts/spikes/s2-3-check.sh helper            # compiles s2-3-menubar.swift
#   bash scripts/spikes/s2-3-check.sh baseline          # light appearance; captures the menu bar before the app starts
#   bash scripts/spikes/s2-3-check.sh launch            # starts target/debug/navaja; waits for its status item
#   bash scripts/spikes/s2-3-check.sh capture           # captures the menu bar in light, then dark, appearance
#   bash scripts/spikes/s2-3-check.sh present           # the icon is drawn in both captures
#   bash scripts/spikes/s2-3-check.sh present-refuses   # negative control: the same check fails on the baseline
#   bash scripts/spikes/s2-3-check.sh tint              # monochrome, and the reference item's colour in each appearance
#   bash scripts/spikes/s2-3-check.sh stop              # quits the app; puts the appearance back
#
# The modes share a folder, $S23_DIR (default: navaja-s2-3 under
# $RUNNER_TEMP, or under $TMPDIR outside CI), so that each can run as its own
# workflow step. Every capture, crop and log lands there, for the run's
# artifact. `stop` runs last whatever happened before it.
#
# It changes the appearance through System Events, which needs the
# Automation permission that GitHub's macOS images grant; screencapture needs
# Screen Recording, granted there too. On a Mac of your own, macOS asks.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

DIR=${S23_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/navaja-s2-3}
HELPER=$DIR/s2-3-menubar
APP=target/debug/navaja
mkdir -p "$DIR"

fail() {
  echo "::error::S2.3 $*"
  exit 1
}

[ "$(uname -s)" = Darwin ] || fail "this script runs on macOS; Windows' half is s2-3-tray.ps1"

helper() {
  [ -x "$HELPER" ] || fail "no $HELPER; run '$0 helper' first"
}

# The appearance as macOS reports it: Dark, or Light when the key is unset.
appearance() {
  defaults read -g AppleInterfaceStyle 2> /dev/null || echo Light
}

# Sets the appearance and reads it back.
set_appearance() {
  local want=$1 dark=false
  [ "$want" = Dark ] && dark=true
  osascript -e "tell application \"System Events\" to tell appearance preferences to set dark mode to $dark" ||
    fail "System Events refused to set the appearance to $want"
  local i
  for ((i = 0; i < 20; i++)); do
    [ "$(appearance)" = "$want" ] && break
    sleep 0.25
  done
  [ "$(appearance)" = "$want" ] || fail "the appearance reads '$(appearance)' after setting $want"
  # The menu bar redraws its items after the change; give it time.
  sleep 3
  echo "Appearance: $(appearance)."
}

# Captures the whole main display, without the shutter sound.
capture() {
  rm -f "$1"
  screencapture -x -m -t png "$1" || fail "screencapture failed"
  [ -s "$1" ] || fail "screencapture wrote nothing to $1"
}

rect() {
  cat "$DIR/icon.rect" 2> /dev/null || fail "no icon rectangle; run '$0 launch' first"
}

# The rightmost status item that is not Navaja's: on macOS 26 the clock.
reference_rect() {
  local pid
  pid=$(cat "$DIR/navaja.pid")
  "$HELPER" items | awk -v pid="$pid" '$6 == 25 && $8 != pid { r = $1 " " $2 " " $3 " " $4 } END { print r }'
}

build_helper() {
  swiftc -O -o "$HELPER" scripts/spikes/s2-3-menubar.swift
  "$HELPER" screen | tee "$DIR/screen.txt"
  sw_vers | tee -a "$DIR/screen.txt"
}

baseline() {
  helper
  appearance > "$DIR/appearance.original"
  echo "Appearance before the spike: $(cat "$DIR/appearance.original")."
  set_appearance Light
  capture "$DIR/before.png"
  echo "Menu bar items before the app starts:"
  "$HELPER" items | tee "$DIR/items-before.txt"
}

launch() {
  helper
  [ -x "$APP" ] || fail "no $APP; build it first with: pnpm tauri build --debug --no-bundle"
  if pgrep -x navaja > /dev/null; then
    fail "a navaja process is already running; quit it first"
  fi
  # The app's settings and logs go to a folder of the spike's own (debug
  # builds honour NAVAJA_APP_DIR). It starts hidden: only the tray icon shows.
  mkdir -p "$DIR/app-dir"
  NAVAJA_APP_DIR="$DIR/app-dir" nohup "$APP" > "$DIR/navaja.log" 2>&1 &
  echo $! > "$DIR/navaja.pid"
  echo "Started $APP, pid $(cat "$DIR/navaja.pid")."
  local item
  item=$("$HELPER" wait-item "$(cat "$DIR/navaja.pid")" 60) || {
    cat "$DIR/navaja.log"
    "$HELPER" items
    exit 1
  }
  echo "$item" > "$DIR/icon.rect"
  echo "Navaja's status item: $item (x y w h, points)."
  echo "Menu bar items with the app running:"
  "$HELPER" items | tee "$DIR/items-after.txt"
}

capture_both() {
  helper
  local icon x y w h width
  icon=$(rect)
  read -r x y w h <<< "$icon"
  width=$("$HELPER" screen | sed -E 's/^display: ([0-9]+)x.*/\1/')
  set_appearance Light
  capture "$DIR/light.png"
  set_appearance Dark
  capture "$DIR/dark.png"
  # The menu bar's right half and the icon, eight times larger, for a person
  # to look at. The thickness is the menu bar's own.
  local bar
  bar=$("$HELPER" screen | sed -E 's/.*menu bar ([0-9.]+) points.*/\1/')
  for name in before light dark; do
    "$HELPER" crop "$DIR/$name.png" "$DIR/menubar-$name.png" "$((width / 2))" 0 "$((width / 2))" "$bar"
    "$HELPER" crop "$DIR/$name.png" "$DIR/icon-$name-x8.png" "$x" "$y" "$w" "$h" 8
  done
}

present() {
  helper
  local icon
  icon=$(rect)
  # shellcheck disable=SC2086
  "$HELPER" present "$DIR/light.png" $icon
  # shellcheck disable=SC2086
  "$HELPER" present "$DIR/dark.png" $icon
}

# The negative control: the same check, at the same place, on the capture
# taken before the app started, must find no icon.
present_refuses() {
  helper
  local icon status=0
  icon=$(rect)
  # shellcheck disable=SC2086
  "$HELPER" present "$DIR/before.png" $icon > "$DIR/present-refuses.log" 2>&1 || status=$?
  sed 's/^/  | /' "$DIR/present-refuses.log"
  [ "$status" -ne 0 ] || fail "present-refuses: the check found an icon in the capture taken before the app started"
  grep -qF 'no icon in' "$DIR/present-refuses.log" ||
    fail "present-refuses: the check failed for another reason"
  echo "The check finds no icon there before the app starts."
}

tint() {
  helper
  local icon reference
  icon=$(rect)
  reference=$(reference_rect)
  [ -n "$reference" ] || fail "tint: no other status item to compare with"
  echo "Reference item (the rightmost other status item): $reference."
  # shellcheck disable=SC2086
  "$HELPER" tint "$DIR/light.png" "$DIR/dark.png" $icon $reference
}

stop() {
  local pid
  if [ -f "$DIR/navaja.pid" ]; then
    pid=$(cat "$DIR/navaja.pid")
    kill "$pid" 2> /dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$pid" 2> /dev/null || break
      sleep 0.25
    done
    kill -9 "$pid" 2> /dev/null || true
    echo "Navaja stopped."
  fi
  if [ -f "$DIR/appearance.original" ] && [ "$(appearance)" != "$(cat "$DIR/appearance.original")" ]; then
    set_appearance "$(cat "$DIR/appearance.original")"
  fi
}

case "${1:-}" in
  helper) build_helper ;;
  baseline) baseline ;;
  launch) launch ;;
  capture) capture_both ;;
  present) present ;;
  present-refuses) present_refuses ;;
  tint) tint ;;
  stop) stop ;;
  *)
    echo "usage: $0 helper|baseline|launch|capture|present|present-refuses|tint|stop" >&2
    exit 2
    ;;
esac
