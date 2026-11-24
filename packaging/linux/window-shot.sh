#!/bin/sh
# Opens the installed window on the X display it is run under (xvfb-run) and fails unless
# a window was mapped and the screen is not all black (#508). Everything it saw is left in
# the output directory: the screenshot, the window tree and the app's log.
set -eu
out=$(realpath -m "${1:?usage: window-shot.sh <output directory>}")
mkdir -p "$out"
work=$(mktemp -d)
rt="$work/runtime"
mkdir -p "$rt" "$work/home"

# A fresh home and runtime: the run must not depend on, or leave, anything of the runner's.
HOME="$work/home" MOORING_RUNTIME="$rt" RUST_LOG="${RUST_LOG:-info}" \
  /usr/lib/fibo/bin/fibo-app > "$out/app.log" 2>&1 &
pid=$!
stop() {
  kill "$pid" 2>/dev/null || true
  pkill -TERM -f "serve --runtime $rt" 2>/dev/null || true
}
trap stop EXIT

mapped=
for _ in $(seq 1 60); do
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "fibo-app exited before a window appeared" >&2
    cat "$out/app.log" >&2
    exit 1
  fi
  # Any top-level window larger than a tooltip counts; the title follows the workspace.
  if xwininfo -root -children | awk '$1 ~ /^0x/ && match($0, /[0-9]+x[0-9]+\+/) {
      split(substr($0, RSTART, RLENGTH - 1), s, "x"); if (s[1] >= 300 && s[2] >= 200) found = 1
    } END { exit !found }'; then
    mapped=1
    break
  fi
  sleep 0.5
done
xwininfo -root -tree > "$out/windows.txt" 2>&1 || true
if [ -z "$mapped" ]; then
  echo "no window was mapped within 30 seconds" >&2
  cat "$out/app.log" >&2
  exit 1
fi

# The first frames on a software adapter take a moment.
sleep "${SHOT_DELAY:-8}"
import -window root "$out/window.png"
# Share of pixels that are not pure black, and how many colours the screen has.
lit=$(convert "$out/window.png" -alpha off -fill white +opaque black -colorspace Gray -format '%[fx:mean]' info:)
colours=$(convert "$out/window.png" -alpha off -format '%k' info:)
echo "non-black share: $lit, distinct colours: $colours"
grep -E 'GPU|adapter|Adapter' "$out/app.log" || true
if ! awk -v lit="$lit" -v colours="$colours" 'BEGIN { exit !(lit > 0.05 && colours > 8) }'; then
  echo "the screen is black: the window opened but drew nothing" >&2
  cat "$out/app.log" >&2
  exit 1
fi
