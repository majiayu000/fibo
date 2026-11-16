#!/usr/bin/env python3
"""Package a native Ubuntu release build. Builds no code and publishes nothing."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release-dir', type=Path, default=ROOT / 'target/release')
parser.add_argument('--output', type=Path, default=ROOT / 'target/linux-package')
args = parser.parse_args()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
arch = subprocess.check_output(['dpkg', '--print-architecture'], text=True).strip()
if arch not in ('amd64', 'arm64'):
    raise SystemExit(f'Unsupported package architecture: {arch}')
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
version = tomllib.loads((ROOT / 'crates/fibo-app/Cargo.toml').read_text())['package']['version']
package = out / f'fibo_{version}+{head[:8]}_{arch}.deb'

with tempfile.TemporaryDirectory(prefix='stage-', dir=out) as tmp:
    stage = Path(tmp)
    app = stage / 'usr/lib/fibo'
    binaries = app / 'bin'
    resources = app / 'Resources'
    binaries.mkdir(parents=True)
    resources.mkdir()
    for name in ('fibo', 'fibo-app'):
        shutil.copy2(args.release_dir / name, binaries / name)
        (binaries / name).chmod(0o755)
    # Keep the old CLI name for hooks already running in terminals.
    (binaries / 'mooring').symlink_to('fibo')
    commands = stage / 'usr/bin'
    commands.mkdir()
    for name in ('fibo', 'mooring'):
        (commands / name).symlink_to(f'../lib/fibo/bin/{name}')
    for source in (ROOT / 'LICENSE', ROOT / 'crates/fibo-app/assets/NERD-FONTS-LICENSE'):
        shutil.copy2(source, resources / source.name)
    catalog = ROOT / 'plugins'
    for plugin in json.loads((catalog / 'index.json').read_text())['plugins']:
        for entry in plugin['files']:
            source = catalog / plugin['id'] / entry['path']
            if hashlib.sha256(source.read_bytes()).hexdigest() != entry['sha256']:
                raise SystemExit(f'Plugin catalog does not match {source}')
            dest = resources / 'plugins' / plugin['id'] / entry['path']
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, dest)
    shutil.copy2(catalog / 'index.json', resources / 'plugins/index.json')
    icons = stage / 'usr/share/icons/hicolor/512x512/apps'
    icons.mkdir(parents=True)
    shutil.copy2(ROOT / 'mobile/assets/icon.png', icons / 'fibo.png')
    desktop = stage / 'usr/share/applications/page.fibo.app.desktop'
    desktop.parent.mkdir(parents=True)
    desktop.write_text('''[Desktop Entry]
Type=Application
Name=Fibo
Comment=A workbench for coding agents
Exec=/usr/lib/fibo/bin/fibo-app
Icon=fibo
Terminal=false
Categories=Development;TerminalEmulator;
StartupWMClass=fibo
''')
    subprocess.run(['desktop-file-validate', str(desktop)], check=True)
    # ELF dependencies are resolved on the same distribution that built the binaries.
    debian = stage / 'debian'
    debian.mkdir()
    (debian / 'control').write_text('Source: fibo\n\nPackage: fibo\nArchitecture: any\n')
    dependencies = subprocess.check_output(
        ['dpkg-shlibdeps', '-O', *[f'-e{binaries / n}' for n in ('fibo', 'fibo-app')]],
        cwd=stage, text=True).strip().removeprefix('shlibs:Depends=')
    # These are loaded at runtime rather than linked into the executable.
    dependencies += ', libvulkan1, libwayland-client0, libxkbcommon-x11-0, xdg-utils, libnotify-bin'
    shutil.rmtree(debian)
    control = stage / 'DEBIAN'
    control.mkdir()
    (control / 'control').write_text(f'''Package: fibo
Version: {version}+{head[:8]}
Architecture: {arch}
Maintainer: Fibo
Section: devel
Priority: optional
Depends: {dependencies}
Recommends: fonts-noto-cjk
Homepage: https://fibo.page
Description: Native workbench for coding agents
 A window and background host for coding agents, with terminal sessions,
 remote computers and a phone connection.
''')
    subprocess.run(['dpkg-deb', '--root-owner-group', '--build', str(stage), str(package)], check=True)

manifest = {
    'source': head, 'version': version, 'architecture': arch,
    'dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT)),
    'package': package.name, 'sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
    'published': False,
}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
