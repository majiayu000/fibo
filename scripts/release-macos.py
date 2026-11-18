#!/usr/bin/env python3
"""Build the macOS release files from a release build: a notarized, stapled Fibo.app packed
as Fibo-<version>-<arch>.zip (arm64 or x86_64; what the in-app updater installs) and .dmg (what people drag into
Applications), plus SHA256SUMS. It does not publish anything; `gh release create` does that.

Needs: `cargo build --release -p fibo-host -p fibo-app` done first (for Intel, add
`--target x86_64-apple-darwin` there and here), the Developer ID
identity in the keychain, and a notarytool keychain profile (`xcrun notarytool store-credentials`).

--app PATH packs an already-signed Fibo.app instead, such as the one a walkthrough checked.
Packaging signs again and a new signature changes the binaries' bytes, so only that bundle still
has the SHA256s its receipt.json and the RC record hold. It is copied, never changed, and refused
unless `codesign --verify --deep --strict` passes, its version is the app crate's and its main
executable is the architecture being packed. A receipt.json beside it must list exactly its
binaries; it is copied beside the output.
"""
import argparse
import hashlib
import json
import os
import pathlib
import platform
import plistlib
import shutil
import subprocess
import tomllib

# actool, notarytool and stapler come from Xcode, not the Command Line Tools.
os.environ.setdefault('DEVELOPER_DIR', '/Applications/Xcode.app/Contents/Developer')
root = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument('--notary-profile', required=True, help='notarytool keychain profile name')
parser.add_argument('--out', type=pathlib.Path, help='Architecture-specific output folder; default: target/dist/<arch>')
parser.add_argument('--skip-notarize', action='store_true', help='local dry run: sign only')
parser.add_argument('--target', help='a cargo target such as x86_64-apple-darwin; default: this Mac. '
                    'With --app, the architecture that bundle must be')
parser.add_argument('--app', type=pathlib.Path, metavar='PATH',
                    help='an already-signed Fibo.app to notarize and pack as is (copied, never changed), '
                    'instead of packaging the release build')
args = parser.parse_args()

version = tomllib.loads((root / 'crates/fibo-app/Cargo.toml').read_text())['package']['version']
arch = {'arm64': 'arm64', 'x86_64': 'x86_64', 'aarch64': 'arm64'}[(args.target or platform.machine()).split('-')[0]]
release_dir = root / 'target' / args.target / 'release' if args.target else root / 'target/release'
name = f'Fibo-{version}-{arch}'
out = (args.out or root / 'target/dist' / arch).resolve()
source = args.app.resolve() if args.app else None
if source:
    # Checked before the output folder is emptied: nothing is removed when the bundle is refused.
    if source.is_relative_to(out) or out.is_relative_to(source):
        raise SystemExit(f'--app {source} and --out {out} overlap; the output folder is emptied first')
    verify = subprocess.run(['codesign', '--verify', '--deep', '--strict', source], capture_output=True, text=True)
    if verify.returncode:
        raise SystemExit(f'--app {source}: codesign --verify --deep --strict failed:\n{verify.stderr.strip()}')
    info = plistlib.loads((source / 'Contents/Info.plist').read_bytes())
    if info.get('CFBundleShortVersionString') != version:
        raise SystemExit(f"--app {source} is version {info.get('CFBundleShortVersionString')}, "
                         f'but crates/fibo-app/Cargo.toml says {version}')
    executable = source / 'Contents/MacOS' / info.get('CFBundleExecutable', '')
    archs = subprocess.run(['lipo', '-archs', executable], capture_output=True, text=True).stdout.split()
    if archs != [arch]:
        raise SystemExit(f"--app {source}: {executable.name} is {' '.join(archs) or 'not a Mach-O file'}, "
                         f'but this packs {arch}')
    receipt = source.parent / 'receipt.json'
    if receipt.exists():
        binaries = {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in (source / 'Contents/MacOS').iterdir() if p.is_file() and not p.is_symlink()}
        listed = json.loads(receipt.read_text()).get('binaries')
        if listed != binaries:
            raise SystemExit(f'{receipt} is another build\'s: it lists {listed}\nbut {source} holds {binaries}\n'
                             'put the right receipt there or move this one away')
shutil.rmtree(out, ignore_errors=True)
out.mkdir(parents=True)
app = out / 'Fibo.app'


def run(*cmd, **kw):
    print('+', ' '.join(map(str, cmd)), flush=True)
    return subprocess.run(list(map(str, cmd)), **{'check': True, **kw})


def notarize(path):
    if args.skip_notarize:
        return
    # Only stdout (the JSON) is read; stderr goes to the terminal, because when an upload or the
    # credentials fail, notarytool exits non-zero and its stderr is the only reason given.
    result = run('xcrun', 'notarytool', 'submit', path, '--keychain-profile', args.notary_profile,
                 '--wait', '--output-format', 'json', stdout=subprocess.PIPE, text=True, check=False)
    if result.returncode:
        raise SystemExit(f'notarytool submit {path.name} exited {result.returncode} '
                         f'(its stderr is above):\n{result.stdout.strip()}')
    status = json.loads(result.stdout)
    print(json.dumps(status, indent=2))
    if status.get('status') != 'Accepted':
        log = run('xcrun', 'notarytool', 'log', status['id'], '--keychain-profile', args.notary_profile,
                  stdout=subprocess.PIPE, text=True)
        raise SystemExit(f'notarization failed:\n{log.stdout}')


if source:
    run('ditto', source, app)
    if receipt.exists():
        run('ditto', receipt, out / 'receipt.json')
else:
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
sums_path = out / f'SHA256SUMS-{arch}'
sums_path.write_text('\n'.join(sums) + '\n')
print('\n'.join(sums))
print(f'\nready: {zip_path}\n       {dmg}\n       {sums_path}')

print('When both architectures are ready: python3 scripts/release-checksums.py <arm64-dir> <x86_64-dir> --out <release-dir>')
