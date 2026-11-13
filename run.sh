#!/bin/sh
set -eu
cd "$(dirname "$0")"
export GPUI_PAUSE_IDLE_DISPLAY_LINK=1
export LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
cargo build -p fibo-host -p fibo-app
exec target/debug/fibo-app "$@"
