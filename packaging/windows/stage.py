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
GUI, CONSOLE = 2, 3


def subsystem(exe: pathlib.Path) -> int:
    """The PE optional header's Subsystem field (same offset in PE32 and PE32+)."""
    data = exe.read_bytes()
    pe = int.from_bytes(data[0x3C:0x40], 'little')
    if data[:2] != b'MZ' or data[pe:pe + 4] != b'PE\0\0':
        raise SystemExit(f'{exe} 不是 Windows 程序')
    return int.from_bytes(data[pe + 24 + 68:pe + 24 + 70], 'little')


# A console program double-clicked gets a command prompt opened beside it, which only shows
# on Windows: the window must be a GUI program (`windows_subsystem` in mooring-app's
# main.rs), and the CLI that Agents run inside terminals must stay a console one.
for binary, want, words in (
    ('mooring-app.exe', GUI, '窗口程序，否则打开时旁边会多一个命令行窗口'),
    ('mooring.exe', CONSOLE, '命令行程序，否则在终端里敲 mooring 没有输出'),
):
    got = subsystem(args.release_dir / binary)
    if got != want:
        raise SystemExit(f'{binary} 应该是{words}（PE Subsystem 是 {got}，应为 {want}）')
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
