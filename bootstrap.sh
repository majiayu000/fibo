#!/bin/sh
set -eu
cd "$(dirname "$0")"
mkdir -p .deps
checkout() {
  name=$1 url=$2 rev=$3
  if [ ! -e ".deps/$name" ]; then
    git init ".deps/$name"
    git -C ".deps/$name" remote add origin "$url"
    git -C ".deps/$name" fetch --depth 1 origin "$rev"
    git -C ".deps/$name" checkout --detach FETCH_HEAD
  fi
  test "$(git -C ".deps/$name" rev-parse HEAD)" = "$rev" || {
    echo "$name has a different revision; refusing to replace it" >&2; exit 1;
  }
}
checkout zed https://github.com/zed-industries/zed.git 2a857f5f2ec368745c1af5209d49836968fe75b8
checkout libghostty-rs https://github.com/Uzaaft/libghostty-rs.git 0ef80413de1326685743378efbfa06d849e041a7
# Later patches change lines earlier ones added, so an earlier patch can look unapplied
# once a later one is in. Everything before the last patch that is in is in as well.
pending=
for name in idle glyph-dilation retained-backbuffer present-render-pass; do
  if git -C .deps/zed apply --reverse --check "$(pwd)/patches/gpui-$name.patch" 2>/dev/null; then
    pending=
  else
    pending="$pending $name"
  fi
done
for name in $pending; do
  git -C .deps/zed apply --check "$(pwd)/patches/gpui-$name.patch"
  git -C .deps/zed apply "$(pwd)/patches/gpui-$name.patch"
done
"${ZIG:-zig}" version
printf '%s\n' 'Dependencies ready. Build with Xcode available and Zig 0.16.x.'
# libghostty-vt-sys clones Ghostty into the build directory on its first build (unless
# GHOSTTY_SOURCE_DIR points at a checkout), so that build still needs the network.
printf '%s\n' 'The first build also downloads the Ghostty source; keep the network up, or retry if it times out.'
