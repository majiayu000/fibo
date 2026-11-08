#!/usr/bin/env python3
"""Lay out a Windows release build the way the installer copies it; no signing or publishing.

bin/        mooring-app.exe (the window) and mooring.exe (host and CLI)
Resources/  the plugin catalog and notices, where market::bundled_dir looks beside bin/
"""
import argparse
import hashlib
import json
import pathlib
import shutil

root = pathlib.Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release-dir', type=pathlib.Path, default=root / 'target/release')
parser.add_argument('--output', type=pathlib.Path, default=root / 'target/windows-stage')
args = parser.parse_args()

stage = args.output.resolve()
shutil.rmtree(stage, ignore_errors=True)
bin_dir = stage / 'bin'
resources = stage / 'Resources'
bin_dir.mkdir(parents=True)
resources.mkdir(parents=True)
for binary in ('mooring-app.exe', 'mooring.exe'):
    shutil.copy2(args.release_dir / binary, bin_dir / binary)
# A build linked against a shared libghostty-vt needs it beside the programs.
for dll in args.release_dir.glob('ghostty-vt*.dll'):
    shutil.copy2(dll, bin_dir / dll.name)
shutil.copy2(root / 'crates/mooring-app/assets/NERD-FONTS-LICENSE', resources / 'NERD-FONTS-LICENSE')
shutil.copy2(root / 'LICENSE', resources / 'LICENSE')
shutil.copy2(root / 'assets/Mooring.ico', resources / 'Mooring.ico')
# The catalog 设置 › 插件 installs from: index.json and exactly the files it lists.
catalog = root / 'plugins'
for plugin in json.loads((catalog / 'index.json').read_text(encoding='utf-8'))['plugins']:
    for f in plugin['files']:
        src = catalog / plugin['id'] / f['path']
        if hashlib.sha256(src.read_bytes()).hexdigest() != f['sha256']:
            raise SystemExit(f'{src} 和 plugins/index.json 不一致：运行 mooring plugins index plugins')
        dst = resources / 'plugins' / plugin['id'] / f['path']
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
shutil.copy2(catalog / 'index.json', resources / 'plugins/index.json')
print(stage)
