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

# Backport libghostty-rs e7edf5f: Cargo resolves watched paths from the crate root.
libghostty_patch=$PWD/patches/libghostty-rerun-if-changed.patch
if ! git -C .deps/libghostty-rs apply --reverse --check "$libghostty_patch" 2>/dev/null; then
  git -C .deps/libghostty-rs apply "$libghostty_patch"
fi
# Later patches change lines earlier ones added, so an earlier patch can look unapplied
# once a later one is in. Everything before the last patch that is in is in as well.
pending=
for name in idle glyph-dilation retained-backbuffer present-render-pass fit-truncated-row; do
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

# Ghostty pins this Zig package to a Codeberg archive that can fail on cold CI runners.
# Seed Zig's normal content-addressed cache. System-cache builds use their own packages.
if [ -z "${GHOSTTY_ZIG_SYSTEM_DIR:-}" ]; then
  translate_c_archive=$PWD/patches/translate-c/translate-c-4e879eb8.tar.gz
  translate_c_hash=translate_c-0.0.0-Q_BUWhVNBwDOEcIqub4VFPJPB6D9dgwzUMHTX5KWr8Xr
  mkdir -p .deps/zig-fetch
  # Zig 0.16 needs a project directory even when fetch is only seeding the cache.
  printf '%s\n' 'pub fn build(_: *@import("std").Build) void {}' > .deps/zig-fetch/build.zig
  translate_c_zig=$(command -v "${ZIG:-zig}")
  case "$translate_c_zig" in
    /*|[A-Za-z]:*) ;;
    *) translate_c_zig=$PWD/$translate_c_zig ;;
  esac
  fetched_hash=$(cd .deps/zig-fetch && "$translate_c_zig" fetch "$translate_c_archive")
  if [ "$fetched_hash" != "$translate_c_hash" ]; then
    printf 'Ghostty translate-c hash mismatch (expected %s, got %s)\n' "$translate_c_hash" "$fetched_hash" >&2
    exit 1
  fi
  printf 'Verified Ghostty translate-c package %s\n' "$fetched_hash"
fi
printf '%s\n' 'Dependencies ready. Build with Xcode available and Zig 0.16.x.'
# libghostty-vt-sys clones Ghostty into the build directory on its first build (unless
# GHOSTTY_SOURCE_DIR points at a checkout), so that build still needs the network.
printf '%s\n' 'The first build also downloads the Ghostty source; keep the network up, or retry if it times out.'
