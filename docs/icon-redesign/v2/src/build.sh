#!/bin/zsh
# usage: build.sh <work dir> <published dir>: rasterize layers, compile a real .icon, ask macOS to render it.
set -eu
W=$1; P=$2; SRC=${0:A:h}
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
python3 "$SRC/gen.py" "$W/svg"
swiftc -O -o "$W/sysicon" "$SRC/sysicon.swift"
mkdir -p "$P"
for n in rope boat buoy; do
  for l in bg fg; do
    rm -f "$W/$n-$l.png"
    perl -e 'alarm 60; exec @ARGV' "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --force-color-profile=srgb --default-background-color=00000000 --window-size=1024,1024 --user-data-dir="$W/chrome" --screenshot="$W/$n-$l.png" "file://$W/svg/$n-$l.svg" 2>"$W/chrome.err" || true  # Chrome can crash on exit after writing; the file decides.
    [[ -s "$W/$n-$l.png" ]] || { tail -3 "$W/chrome.err"; exit 1; }
  done
  I="$W/$n.icon"; rm -rf "$I" "$W/$n-out" "$W/$n.app"; mkdir -p "$I/Assets" "$W/$n-out" "$W/$n.app/Contents/Resources" "$W/$n.app/Contents/MacOS"
  cp "$W/$n-bg.png" "$W/$n-fg.png" "$I/Assets/"
  cat > "$I/icon.json" <<E
{"groups":[{"layers":[{"image-name":"$n-fg.png","name":"fg"}],"shadow":{"kind":"neutral","opacity":0.4},"specular":false,"translucency":{"enabled":false,"value":0.5}},
{"layers":[{"image-name":"$n-bg.png","name":"bg","glass":false}],"translucency":{"enabled":false,"value":0.5}}],
"supported-platforms":{"circles":["watchOS"],"squares":"shared"}}
E
  perl -e 'alarm 120; exec @ARGV' "$DEVELOPER_DIR/usr/bin/actool" "$I" --compile "$W/$n-out" --app-icon "$n" --platform macosx --minimum-deployment-target 11.0 --output-partial-info-plist "$W/$n-out/p.plist" --output-format human-readable-text > "$W/$n-actool.log" 2>&1
  cp "$W/$n-out/Assets.car" "$W/$n-out/$n.icns" "$W/$n.app/Contents/Resources/"; printf '#!/bin/sh\n' > "$W/$n.app/Contents/MacOS/x"; chmod +x "$W/$n.app/Contents/MacOS/x"
  cat > "$W/$n.app/Contents/Info.plist" <<E
<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>test.mooring.v2.$n.$$</string><key>CFBundleName</key><string>$n</string><key>CFBundleExecutable</key><string>x</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleIconFile</key><string>$n</string><key>CFBundleIconName</key><string>$n</string></dict></plist>
E
  for s in 1024 128 64 32; do perl -e 'alarm 30; exec @ARGV' "$W/sysicon" "$W/$n.app" "$P/$n-$s.png" $s; done
  cp "$W/svg/$n-tray.svg" "$W/svg/$n-tray-off.svg" "$P/"
done
