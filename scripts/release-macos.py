#!/usr/bin/env python3
"""Build the macOS release files from a release build: a notarized, stapled Fibo.app packed
as Fibo-<version>-<arch>.zip (arm64 or x86_64; what the in-app updater installs) and .dmg (what people drag into
Applications), plus SHA256SUMS. It does not publish anything; `gh release create` does that.

Needs: `cargo build --release -p fibo-host -p fibo-app` done first (for Intel, add
`--target x86_64-apple-darwin` there and here), the Developer ID
identity in the keychain, and a notarytool keychain profile (`xcrun notarytool store-credentials`).
"""
import argparse
import hashlib
import json
import os
import pathlib
import platform
import shutil
import subprocess
import tomllib

# actool, notarytool and stapler come from Xcode, not the Command Line Tools.
os.environ.setdefault('DEVELOPER_DIR', '/Applications/Xcode.app/Contents/Developer')
root = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--notary-profile', required=True, help='notarytool keychain profile name')
parser.add_argument('--out', type=pathlib.Path, default=root / 'target/dist')
parser.add_argument('--skip-notarize', action='store_true', help='local dry run: sign only')
parser.add_argument('--target', help='a cargo target such as x86_64-apple-darwin; default: this Mac')
args = parser.parse_args()

version = tomllib.loads((root / 'crates/fibo-app/Cargo.toml').read_text())['package']['version']
arch = {'arm64': 'arm64', 'x86_64': 'x86_64', 'aarch64': 'arm64'}[(args.target or platform.machine()).split('-')[0]]
release_dir = root / 'target' / args.target / 'release' if args.target else root / 'target/release'
name = f'Fibo-{version}-{arch}'
out = args.out.resolve()
shutil.rmtree(out, ignore_errors=True)
out.mkdir(parents=True)
app = out / 'Fibo.app'


def run(*cmd, **kw):
    print('+', ' '.join(map(str, cmd)), flush=True)
    return subprocess.run(list(map(str, cmd)), check=True, **kw)


def notarize(path):
    if args.skip_notarize:
        return
    result = run('xcrun', 'notarytool', 'submit', path, '--keychain-profile', args.notary_profile,
                 '--wait', '--output-format', 'json', capture_output=True, text=True)
    status = json.loads(result.stdout)
    print(json.dumps(status, indent=2))
    if status.get('status') != 'Accepted':
        log = run('xcrun', 'notarytool', 'log', status['id'], '--keychain-profile', args.notary_profile,
                  capture_output=True, text=True)
        raise SystemExit(f'notarization failed:\n{log.stdout}')


run('python3', root / 'package-macos.py', '--release-dir', release_dir, '--output', app)

# The app first: notarize a zip of it, then staple the ticket so it opens offline.
submit = out / 'submit.zip'
run('ditto', '-c', '-k', '--keepParent', app, submit)
notarize(submit)
submit.unlink()
if not args.skip_notarize:
    run('xcrun', 'stapler', 'staple', app)
    run('spctl', '--assess', '--type', 'execute', '-vv', app)

zip_path = out / f'{name}.zip'
run('ditto', '-c', '-k', '--keepParent', app, zip_path)

# The disk image: the app and a link to /Applications, signed and notarized itself too.
dmg_root = out / 'dmg'
dmg_root.mkdir()
run('ditto', app, dmg_root / 'Fibo.app')
(dmg_root / 'Applications').symlink_to('/Applications')
dmg = out / f'{name}.dmg'
run('hdiutil', 'create', '-volname', 'Fibo', '-srcfolder', dmg_root, '-ov', '-format', 'UDZO', dmg)
shutil.rmtree(dmg_root)
run('codesign', '--force', '--timestamp', '--sign', 'Developer ID Application: Ma JiaYu (C5UWZ934C2)', dmg)
notarize(dmg)
if not args.skip_notarize:
    run('xcrun', 'stapler', 'staple', dmg)
    run('spctl', '--assess', '--type', 'open', '--context', 'context:primary-signature', '-vv', dmg)

sums = [f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}' for p in (zip_path, dmg)]
(out / 'SHA256SUMS').write_text('\n'.join(sums) + '\n')
print('\n'.join(sums))
print(f'\nready: {zip_path}\n       {dmg}\n       {out / "SHA256SUMS"}')
