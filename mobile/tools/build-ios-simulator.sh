#!/usr/bin/env bash
# Builds a standalone Apple Silicon simulator app. It is not an iPhone IPA.
set -euo pipefail
cd "$(dirname "$0")/.."
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
evidence="$(cd .. && pwd)/target/ios-evidence"
mkdir -p "$evidence"

bun install --frozen-lockfile
bun test
bun run typecheck
CI=1 bunx expo prebuild -p ios --no-install
(cd ios && pod install)
(
  cd ios
  CI=1 nice -n 10 xcodebuild -workspace Fibo.xcworkspace -scheme Fibo \
    -configuration Release -sdk iphonesimulator \
    -destination 'generic/platform=iOS Simulator' \
    -derivedDataPath build/ios-release -jobs 4 ARCHS=arm64 build
) > "$evidence/simulator-build.log" 2>&1

app="ios/build/ios-release/Build/Products/Release-iphonesimulator/Fibo.app"
codesign --verify --deep --strict "$app"
test -s "$app/main.jsbundle"
ditto -c -k --sequesterRsrc --keepParent "$app" "$evidence/Fibo-ios-simulator.zip"
{
  git rev-parse HEAD
  git status --short
  xcodebuild -version
  shasum -a 256 "$evidence/Fibo-ios-simulator.zip"
} > "$evidence/build-manifest.txt"
printf 'Simulator app: %s/%s\nEvidence: %s\n' "$PWD" "$app" "$evidence"
