#!/usr/bin/env bash
# Spike S2.3 (docs/spikes.md): the macOS half of the tray-icon check in
# .github/workflows/spikes.yml. It runs the app as shipped, captures the menu
# bar in the light and the dark appearance, and checks that the template icon
# is drawn, monochrome, and tinted like the system's own menu bar items.
# Windows' half is s2-3-tray.ps1.
#
# Usage, on macOS, from anywhere in the repository, after
# `pnpm tauri build --debug --no-bundle` (the shipped configuration):
#   bash scripts/spikes/s2-3-check.sh helper           # compiles s2-3-menubar.swift; the display's size and scale
#   bash scripts/spikes/s2-3-check.sh baseline         # light appearance; captures the menu bar before the app starts
#   bash scripts/spikes/s2-3-check.sh launch           # starts target/debug/navaja; waits for its status item
#   bash scripts/spikes/s2-3-check.sh capture          # captures the menu bar in light, then dark, appearance
#   bash scripts/spikes/s2-3-check.sh present          # the icon is drawn in both captures
#   bash scripts/spikes/s2-3-check.sh present-refuses  # negative control: the same check fails on the baseline
#   bash scripts/spikes/s2-3-check.sh tint             # monochrome, and a system item's colour in each appearance
#   bash scripts/spikes/s2-3-check.sh control-build    # quits the app; builds it again with the icon not a template
#   bash scripts/spikes/s2-3-check.sh tint-refuses     # negative control: `tint` fails on that build's icon, on its tint
#   bash scripts/spikes/s2-3-check.sh stop             # quits the apps; puts the appearance back
#
# The modes share a folder, $S23_DIR (default: navaja-s2-3 under
# $RUNNER_TEMP, or under $TMPDIR outside CI), so that each can run as its own
# workflow step. Every capture, crop and log lands there, for the run's
# artifact. `stop` runs last whatever happened before it.
#
# control-build edits app/src-tauri/src/tray.rs to pass false to
# icon_as_template, builds, keeps the binary in the folder above as
# navaja-not-template, and puts tray.rs back, pass or fail. It overwrites
# target/debug/navaja, so it runs after the checks on the shipped build.
# The copy of tray.rs it keeps in that folder lasts only while the edit
# does. `stop` puts it back only when tray.rs is still exactly that copy
# with the edit, so it never overwrites changes made to tray.rs since.
#
# It changes the appearance through System Events, which needs the
# Automation permission that GitHub's macOS images grant; screencapture needs
# Screen Recording, granted there too. On a Mac of your own, macOS asks.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

DIR=${S23_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/navaja-s2-3}
HELPER=$DIR/s2-3-menubar
APP=target/debug/navaja
CONTROL_APP=$DIR/navaja-not-template
TRAY=app/src-tauri/src/tray.rs
TRAY_COPY=$DIR/tray.rs.orig
TEMPLATE_CALL='.icon_as_template(cfg!(target_os = "macos"))'
# The control build's one edit to tray.rs, as a sed script.
CONTROL_EDIT='s/\.icon_as_template(cfg!(target_os = "macos"))/.icon_as_template(false)/'
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
  local want=$1 dark=false i
  [ "$want" = Dark ] && dark=true
  osascript -e "tell application \"System Events\" to tell appearance preferences to set dark mode to $dark" ||
    fail "System Events refused to set the appearance to $want"
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

# A status item's rectangle, in points: x y w h.
rect() {
  cat "$DIR/$1.rect" 2> /dev/null || fail "no rectangle for $1; start that app first"
}

# The menu bar's status items (layer 25), one "x y w h" line each, left to right.
status_items() {
  "$HELPER" items | awk '$6 == 25 { print $1, $2, $3, $4 }'
}

# The rightmost status item that is not the app's: on macOS 26 the clock.
reference_rect() {
  status_items | grep -vxF "$(rect "$1")" | tail -n 1
}

# Starts `binary` as `tag` and waits for its status item.
start_app() {
  local binary=$1 tag=$2 item
  helper
  [ -x "$binary" ] || fail "no $binary to start"
  if pgrep -x "$(basename "$binary")" > /dev/null; then
    fail "a $(basename "$binary") process is already running; quit it first"
  fi
  # The app's settings and logs go to a folder of the spike's own (debug
  # builds honour NAVAJA_APP_DIR). Its window shows once the front end is
  # ready (window.rs); the checks look only at the menu bar's status items.
  mkdir -p "$DIR/app-dir-$tag"
  # Control Center owns every status item on macOS 26, so the app's is
  # the new one, left of the others (s2-3-menubar.swift, wait-item).
  local count leftmost
  count=$(status_items | wc -l | tr -d ' ')
  leftmost=$(status_items | head -n 1 | cut -d' ' -f1)
  NAVAJA_APP_DIR="$DIR/app-dir-$tag" nohup "$binary" > "$DIR/$tag.log" 2>&1 &
  echo $! > "$DIR/$tag.pid"
  echo "Started $binary, pid $(cat "$DIR/$tag.pid"), with $count status items in the menu bar."
  if ! item=$("$HELPER" wait-item "$count" "${leftmost:-100000}" 60); then
    echo "$item"
    echo "The app's output:"
    cat "$DIR/$tag.log"
    echo "Menu bar items:"
    "$HELPER" items
    exit 1
  fi
  echo "$item" > "$DIR/$tag.rect"
  echo "Its status item: $item (x y w h, points)."
  echo "Menu bar items with the app running:"
  "$HELPER" items | tee "$DIR/items-$tag.txt"
}

stop_app() {
  local tag=$1 pid
  [ -f "$DIR/$tag.pid" ] || return 0
  pid=$(cat "$DIR/$tag.pid")
  kill "$pid" 2> /dev/null || true
  for _ in $(seq 1 20); do
    kill -0 "$pid" 2> /dev/null || break
    sleep 0.25
  done
  kill -9 "$pid" 2> /dev/null || true
  rm -f "$DIR/$tag.pid"
  echo "Stopped $tag (pid $pid)."
}

# Quits `tag` and checks that its status item goes with it: the item the
# checks measured was the app's.
quit_app() {
  local tag=$1
  stop_app "$tag"
  # shellcheck disable=SC2046
  "$HELPER" wait-gone $(rect "$tag") 10
}

# Captures `tag`'s light and dark menu bar, plus crops for a person to look
# at: the menu bar's right half, and the icon eight times larger.
capture_set() {
  local tag=$1 x y w h width bar
  helper
  read -r x y w h <<< "$(rect "$tag")"
  width=$("$HELPER" screen | sed -E 's/^display: ([0-9]+)x.*/\1/')
  bar=$(awk -v y="$y" -v h="$h" 'BEGIN { print y + h }')
  set_appearance Light
  capture "$DIR/$tag-light.png"
  set_appearance Dark
  capture "$DIR/$tag-dark.png"
  for name in "$tag-light" "$tag-dark"; do
    "$HELPER" crop "$DIR/$name.png" "$DIR/menubar-$name.png" "$((width / 2))" 0 "$((width / 2))" "$bar"
    "$HELPER" crop "$DIR/$name.png" "$DIR/icon-$name-x8.png" "$x" "$y" "$w" "$h" 8
  done
}

build_helper() {
  swiftc -O -o "$HELPER" scripts/spikes/s2-3-menubar.swift
  {
    "$HELPER" screen
    sw_vers
  } | tee "$DIR/screen.txt"
}

baseline() {
  local x y w h width
  helper
  appearance > "$DIR/appearance.original"
  echo "Appearance before the spike: $(cat "$DIR/appearance.original")."
  set_appearance Light
  capture "$DIR/before.png"
  echo "Menu bar items before the app starts:"
  "$HELPER" items | tee "$DIR/items-before.txt"
  if ! [ -s "$DIR/items-before.txt" ]; then
    echo "Every on-screen window:"
    "$HELPER" windows
    fail "baseline: no status items found in the menu bar, so nothing can be compared"
  fi
  # The menu bar's right half, as tall as the rightmost item reaches.
  width=$("$HELPER" screen | sed -E 's/^display: ([0-9]+)x.*/\1/')
  read -r x y w h <<< "$(tail -n 1 "$DIR/items-before.txt" | awk '{ print $1, $2, $3, $4 }')"
  "$HELPER" crop "$DIR/before.png" "$DIR/menubar-before.png" "$((width / 2))" 0 "$((width / 2))" \
    "$(awk -v y="$y" -v h="$h" 'BEGIN { print y + h }')"
}

present() {
  local icon
  helper
  icon=$(rect shipped)
  # shellcheck disable=SC2086
  "$HELPER" present "$DIR/shipped-light.png" $icon
  # shellcheck disable=SC2086
  "$HELPER" present "$DIR/shipped-dark.png" $icon
}

# The negative control for `present`: the same check, at the same place, on
# the capture taken before the app started, must find no icon.
present_refuses() {
  local icon status=0 x y w h
  helper
  icon=$(rect shipped)
  read -r x y w h <<< "$icon"
  "$HELPER" crop "$DIR/before.png" "$DIR/icon-before-x8.png" "$x" "$y" "$w" "$h" 8
  # shellcheck disable=SC2086
  "$HELPER" present "$DIR/before.png" $icon > "$DIR/present-refuses.log" 2>&1 || status=$?
  # Behind a prefix: the output holds an error on purpose.
  sed 's/^/  | /' "$DIR/present-refuses.log"
  [ "$status" -ne 0 ] || fail "present-refuses: the check found an icon in the capture taken before the app started"
  grep -qF 'no icon in' "$DIR/present-refuses.log" ||
    fail "present-refuses: the check failed for another reason"
  echo "The check finds no icon there before the app starts."
}

tint() {
  local icon reference
  helper
  icon=$(rect shipped)
  reference=$(reference_rect shipped)
  [ -n "$reference" ] || fail "tint: no other status item to compare with"
  echo "Reference: the rightmost other status item, at $reference."
  # shellcheck disable=SC2086
  "$HELPER" tint "$DIR/shipped-light.png" "$DIR/shipped-dark.png" $icon $reference
}

control_build() {
  quit_app shipped
  grep -qF "$TEMPLATE_CALL" "$TRAY" || fail "control-build: $TRAY no longer has $TEMPLATE_CALL"
  # The copy lives as long as the edit: whichever way this ends, tray.rs
  # goes back and the copy goes, so no later run can restore a stale one.
  cp "$TRAY" "$TRAY_COPY"
  trap 'cp "$TRAY_COPY" "$TRAY" && rm -f "$TRAY_COPY"' EXIT
  sed -i '' "$CONTROL_EDIT" "$TRAY"
  grep -qF '.icon_as_template(false)' "$TRAY" || fail "control-build: the edit did not apply"
  pnpm tauri build --debug --no-bundle
  cp "$APP" "$CONTROL_APP"
  cp "$TRAY_COPY" "$TRAY"
  trap - EXIT
  # Against the copy, not git: tray.rs may hold changes of its own.
  cmp -s "$TRAY_COPY" "$TRAY" || fail "control-build: $TRAY was not put back; the copy is $TRAY_COPY"
  rm -f "$TRAY_COPY"
  echo "Built $CONTROL_APP: the same app, with its tray icon not a template."
}

# Whether `tint`'s output (file $1) shows it failed for the reason the
# negative control is about: the icon does not take the system's colour.
# The helper ends a failed check with one line,
#   ::error::S2.3 tint: <reason>; <reason>; ...
# and every reason must be about the icon's tint: its luma against the
# reference item's, its side of the background, no icon drawn in the dark
# appearance, or too small a change between the appearances. At least one
# must be about the dark appearance or that change. Any other reason (no
# icon in the light capture, a reference item that shows nothing, an icon
# that is not monochrome) or any other error means the check failed for
# something else, and so does an output without that line.
tint_refused_for_tint() {
  local errors reasons reason dark=0
  errors=$(grep -F '::error::' "$1" || true)
  case $errors in
    '::error::S2.3 tint: '*) ;;
    *) return 1 ;;
  esac
  # One error line only.
  [ "$(printf '%s\n' "$errors" | wc -l | tr -d ' ')" -eq 1 ] || return 1
  reasons=${errors#'::error::S2.3 tint: '}
  # Split on "; " with expansions only, which bash 3.2 has too.
  while [ -n "$reasons" ]; do
    reason=${reasons%%; *}
    if [ "$reason" = "$reasons" ]; then reasons=''; else reasons=${reasons#*; }; fi
    case $reason in
      'dark: no icon drawn' | "the icon's luma changes by "*) dark=1 ;;
      "dark: the icon's luma "* | 'dark: the icon is '*' than the menu bar, the reference '*) dark=1 ;;
      "light: the icon's luma "* | 'light: the icon is '*' than the menu bar, the reference '*) ;;
      *) return 1 ;;
    esac
  done
  [ "$dark" -eq 1 ]
}

# The negative control for `tint`: the same icon, drawn as a plain image,
# stays black in the dark appearance, and the check must say so.
tint_refuses() {
  local icon reference status=0
  helper
  [ -x "$CONTROL_APP" ] || fail "no $CONTROL_APP; run '$0 control-build' first"
  stop_app shipped
  start_app "$CONTROL_APP" control
  capture_set control
  icon=$(rect control)
  reference=$(reference_rect control)
  quit_app control
  [ -n "$reference" ] || fail "tint-refuses: no other status item to compare with"
  # shellcheck disable=SC2086
  "$HELPER" tint "$DIR/control-light.png" "$DIR/control-dark.png" $icon $reference \
    > "$DIR/tint-refuses.log" 2>&1 || status=$?
  # Behind a prefix: the output holds an error on purpose.
  sed 's/^/  | /' "$DIR/tint-refuses.log"
  [ "$status" -ne 0 ] || fail "tint-refuses: the check passed an icon that is not a template"
  tint_refused_for_tint "$DIR/tint-refuses.log" ||
    fail "tint-refuses: the check failed, but not only on the icon's tint, or not on the dark appearance"
  echo "The tint check refuses the icon drawn as a plain image, on its tint in the dark appearance."
}

stop() {
  stop_app shipped
  stop_app control
  if [ -f "$DIR/appearance.original" ] && [ "$(appearance)" != "$(cat "$DIR/appearance.original")" ]; then
    set_appearance "$(cat "$DIR/appearance.original")"
  fi
  osascript -e 'quit app "System Events"' 2> /dev/null || true
  # A copy left only when control-build was killed before its trap ran.
  # Put it back only while tray.rs is exactly that copy with the edit.
  if [ -f "$TRAY_COPY" ]; then
    if sed "$CONTROL_EDIT" "$TRAY_COPY" | cmp -s - "$TRAY"; then
      cp "$TRAY_COPY" "$TRAY"
      echo "Put $TRAY back from $TRAY_COPY."
    else
      echo "$TRAY is not the control build's edit of $TRAY_COPY; left as it is."
    fi
    rm -f "$TRAY_COPY"
  fi
}

case "${1:-}" in
  helper) build_helper ;;
  baseline) baseline ;;
  launch) start_app "$APP" shipped ;;
  capture) capture_set shipped ;;
  present) present ;;
  present-refuses) present_refuses ;;
  tint) tint ;;
  control-build) control_build ;;
  tint-refuses) tint_refuses ;;
  stop) stop ;;
  *)
    echo "usage: $0 helper|baseline|launch|capture|present|present-refuses|tint|control-build|tint-refuses|stop" >&2
    exit 2
    ;;
esac
