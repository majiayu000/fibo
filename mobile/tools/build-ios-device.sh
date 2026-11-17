#!/usr/bin/env bash
# Check real-device code generation without Apple credentials. This archive is NOT an IPA.
# Signed installation: bunx eas-cli build --platform ios --profile preview
# Store distribution: bunx eas-cli build --platform ios --profile production
set -euo pipefail
cd "$(dirname "$0")/.."
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
evidence="$(cd .. && pwd)/target/ios-device-evidence"
mkdir -p "$evidence"
bun install --frozen-lockfile
CI=1 bunx expo prebuild -p ios --no-install
(cd ios && pod install)
(
  cd ios
  CI=1 nice -n 10 xcodebuild -workspace Fibo.xcworkspace -scheme Fibo \
    -configuration Release -destination 'generic/platform=iOS' \
    -derivedDataPath build/ios-device -archivePath "$evidence/Fibo.xcarchive" \
    -jobs 4 CODE_SIGNING_ALLOWED=NO archive
) > "$evidence/device-build.log" 2>&1
app="$evidence/Fibo.xcarchive/Products/Applications/Fibo.app"
test -s "$app/main.jsbundle"
archive="$evidence/Fibo-ios-device-unsigned.zip"
ditto -c -k --keepParent "$evidence/Fibo.xcarchive" "$archive"
{
  printf 'Unsigned device archive; not installable until signed.\n'
  git rev-parse HEAD
  git status --short
  xcodebuild -version
  shasum -a 256 "$app/Fibo" "$app/main.jsbundle"
  shasum -a 256 "$archive"
} > "$evidence/build-manifest.txt"
printf 'Unsigned device archive (not an installable IPA): %s\n' "$archive"
