#!/usr/bin/env python3
"""Package the current release build locally; no installation, notarization or publishing."""
import argparse
import hashlib
import json
import pathlib
import plistlib
import shutil
import subprocess
import tomllib

root = pathlib.Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release-dir', type=pathlib.Path, default=root / 'target/release')
parser.add_argument('--output', type=pathlib.Path, default=root / 'target/bundle/Fibo.app',
                    help='Bundle path; use a separate path when an installed build is running')
parser.add_argument('--icon', type=pathlib.Path,
                    default=root / 'assets/Fibo.icns',
                    help='App icon (.icns); defaults to the ring icon')
# A stable identity keeps macOS privacy grants (e.g. Desktop access) across rebuilds;
# ad-hoc ('-') signatures change with every build, so macOS asks again each time.
parser.add_argument('--sign-identity', default='Developer ID Application: Ma JiaYu (C5UWZ934C2)',
                    help="codesign identity; pass '-' for ad-hoc signing")
args = parser.parse_args()
bundle = args.output.resolve()
sign = ['codesign', '--force', '--sign', args.sign_identity]
if args.sign_identity != '-':
    # The WASM plug-in host (wasmtime/Cranelift) compiles plug-ins to machine code at run
    # time and flips those pages to executable with mprotect. Under the hardened runtime
    # that page has no code signature, so macOS kills the app (SIGKILL, Code Signature
    # Invalid) the moment a plug-in starts. Grant JIT and unsigned executable memory.
    entitlements = root / 'target/Fibo.entitlements'
    entitlements.parent.mkdir(parents=True, exist_ok=True)
    entitlements.write_bytes(plistlib.dumps({
        'com.apple.security.cs.allow-jit': True,
        'com.apple.security.cs.allow-unsigned-executable-memory': True,
    }))
    sign += ['--options', 'runtime', '--timestamp', '--entitlements', str(entitlements)]
# Build and sign beside the target, then swap directories by rename. Writing into a
# running bundle invalidates its signature and macOS kills the live app and daemon.
stage = bundle.with_name(bundle.name + '.staging')
shutil.rmtree(stage, ignore_errors=True)
mac = stage / 'Contents/MacOS'
resources = stage / 'Contents/Resources'
mac.mkdir(parents=True, exist_ok=True)
resources.mkdir(parents=True, exist_ok=True)
for binary in ('fibo-app', 'fibo'):
    shutil.copy2(args.release_dir / binary, mac / binary)
    subprocess.run([*sign, str(mac / binary)], check=True)
# The program and command are `fibo`. Older names stay as links to it: terminals started
# before the rename keep MOORING_HOOK=.../MacOS/mooring, and scripts may still look for the
# window's old program file. Keepline's `keepline` and `keeplined` went on 11-13: it was never
# released, and no terminal still carried only its variables.
(mac / 'mooring').symlink_to('fibo')
(mac / 'mooring-app').symlink_to('fibo-app')
shutil.copy2(root / 'crates/fibo-app/assets/NERD-FONTS-LICENSE', resources / 'NERD-FONTS-LICENSE')
shutil.copy2(args.icon, resources / 'Fibo.icns')
# macOS 26 shrinks a plain .icns and puts a light plate behind it; an Icon Composer asset fills the tile.
# Keep the .icns beside it for older systems, and skip this when another icon was asked for.
icon_name = {}
if args.icon == root / 'assets/Fibo.icns':
    subprocess.run(['xcrun', 'actool', str(root / 'assets/Fibo.icon'), '--compile', str(resources),
                    '--app-icon', 'Fibo', '--platform', 'macosx', '--minimum-deployment-target', '14.0',
                    '--output-partial-info-plist', str(stage / 'icon-info.plist'),
                    '--output-format', 'human-readable-text'], check=True, stdout=subprocess.DEVNULL)
    (stage / 'icon-info.plist').unlink()
    icon_name = {'CFBundleIconName': 'Fibo'}
# The plugin catalog 设置 › 插件 installs from: index.json and exactly the files it lists.
catalog = root / 'plugins'
for plugin in json.loads((catalog / 'index.json').read_text())['plugins']:
    for f in plugin['files']:
        src = catalog / plugin['id'] / f['path']
        if hashlib.sha256(src.read_bytes()).hexdigest() != f['sha256']:
            raise SystemExit(f'{src} 和 plugins/index.json 不一致：运行 fibo plugins index plugins')
        dst = resources / 'plugins' / plugin['id'] / f['path']
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
shutil.copy2(catalog / 'index.json', resources / 'plugins/index.json')
# The version users and the updater compare: the app crate's, so a release bumps it in one place.
# CFBundleVersion must only grow; the commit count does.
version = tomllib.loads((root / 'crates/fibo-app/Cargo.toml').read_text())['package']['version']
build = subprocess.check_output(['git', 'rev-list', '--count', 'HEAD'], cwd=root, text=True).strip()
(stage / 'Contents/Info.plist').write_bytes(plistlib.dumps({
    'CFBundleName': 'Fibo', 'CFBundleDisplayName': 'Fibo',
    # Matches update::BUNDLE_ID; before the rename it was local.mooring.workbench.
    'CFBundleIdentifier': 'page.fibo.app', 'CFBundleExecutable': 'fibo-app',
    'CFBundlePackageType': 'APPL', 'CFBundleIconFile': 'Fibo', **icon_name, 'CFBundleVersion': build, 'CFBundleShortVersionString': version,
    'NSHighResolutionCapable': True, 'LSMinimumSystemVersion': '14.0',
    # macOS names the app, not the Agent in its terminal, when a program there first opens
    # 桌面, 文稿 or 下载; without these it asks with no reason given.
    'NSDesktopFolderUsageDescription': 'Fibo 终端里运行的程序（例如 Claude Code、Codex）要读写桌面上的项目。',
    'NSDocumentsFolderUsageDescription': 'Fibo 终端里运行的程序（例如 Claude Code、Codex）要读写文稿里的项目。',
    'NSDownloadsFolderUsageDescription': 'Fibo 终端里运行的程序（例如 Claude Code、Codex）要读写下载里的文件。',
}))
subprocess.run([*sign, str(stage)], check=True)
subprocess.run(['codesign', '--verify', '--deep', '--strict', str(stage)], check=True)
# Running processes keep the old inodes alive after the rename and unlink.
retired = bundle.with_name(bundle.name + '.retired')
shutil.rmtree(retired, ignore_errors=True)
if bundle.exists():
    bundle.rename(retired)
stage.rename(bundle)
shutil.rmtree(retired, ignore_errors=True)
mac = bundle / 'Contents/MacOS'
receipt = {'bundle': str(bundle), 'version': version, 'build': build, 'signature': args.sign_identity,
           'hardened_runtime': args.sign_identity != '-', 'notarized': False, 'published': False,
           'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
           'binaries': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in mac.iterdir() if p.is_file() and not p.is_symlink()}}
(bundle.parent / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt, indent=2))
