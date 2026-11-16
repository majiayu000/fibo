#!/usr/bin/env python3
"""Collect the four macOS release assets and compute one complete SHA256SUMS.

Run after release-macos.py has produced arm64 and x86_64 folders. Reads the
version from the app crate. Does not publish, sign, or alter the source assets.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import tomllib


def collect(sources, out, version):
    assets = []
    for arch, source in zip(('arm64', 'x86_64'), sources, strict=True):
        for extension in ('zip', 'dmg'):
            path = source / f'Fibo-{version}-{arch}.{extension}'
            if not path.is_file():
                raise ValueError(f'Missing release asset: {path}')
            assets.append(path)
    # Check all inputs before creating a publishable manifest.
    out.mkdir(parents=True, exist_ok=True)
    sums = []
    for asset in assets:
        destination = out / asset.name
        if asset.resolve() != destination.resolve():
            shutil.copy2(asset, destination)
        with destination.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        sums.append(f'{digest}  {asset.name}')
    manifest = out / 'SHA256SUMS'
    manifest.write_text('\n'.join(sums) + '\n')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('arm64', type=Path)
    parser.add_argument('x86_64', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    version = tomllib.loads((root / 'crates/fibo-app/Cargo.toml').read_text())['package']['version']
    try:
        manifest = collect((args.arm64, args.x86_64), args.out, version)
    except ValueError as error:
        parser.error(str(error))
    print(manifest.read_text(), end='')
    print(f'Ready: {manifest}')


if __name__ == '__main__':
    main()
