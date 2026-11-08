#!/usr/bin/env python3
"""The native theme scenes, each in a real window with its own config, home and runtime.
Builds nothing: BIN_DIR holds `mooring` and a `mooring-app` built with `--features
mooring-app/visual-test`.

  python3 scripts/theme-scenes.py BIN_DIR [OUT]

A scene fails on any assertion inside it (crates/mooring-app/src/settings_evidence.rs).
Its waits are in wall-clock time, so a failure is run once more, as ci-test.sh does for
real terminals; a second failure is a failure.
"""
import json, os, pathlib, shutil, signal, subprocess, sys, tempfile

BIN = pathlib.Path(sys.argv[1]).resolve()
OUT = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else tempfile.mkdtemp(prefix='scenes-')).resolve()
SCENES = ['themes', 'editor', 'details', 'material', 'sharing', 'migrate']
COLORS = {'status': {'working': '#112233'}, 'tone': {'ok': '#223344'}, 'seen': {'title': '#445566', 'dot': 'fade'}}
TIMEOUT = 180

def run(scene):
    # A unix socket's path is limited to about 104 bytes, so a short directory.
    root = pathlib.Path(tempfile.mkdtemp(prefix='ms-', dir='/tmp'))
    try:
        for name in ('home', 'cfg/mooring', 'claude'):
            (root / name).mkdir(parents=True)
        runtime = root / 'rt'
        runtime.mkdir(mode=0o700)
        # The scenes look at themes in a workbench already in use: a fresh config would be
        # greeted with the first-run page instead of the shell tab they type into.
        (root / 'cfg/mooring/first-run-done').write_text('')
        if scene == 'migrate':
            (root / 'cfg/mooring/colors.json').write_text(json.dumps(COLORS))
        out = OUT / scene
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir(parents=True)
        env = {**os.environ, 'MOORING_SETTINGS_EVIDENCE': scene, 'XDG_CONFIG_HOME': str(root / 'cfg'),
               'CLAUDE_CONFIG_DIR': str(root / 'claude'), 'HOME': str(root / 'home')}
        app = subprocess.Popen([str(BIN / 'mooring-app'), '--runtime', str(runtime), '--visual-evidence', str(out)],
                               env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            log, _ = app.communicate(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            app.kill()
            log, _ = app.communicate()
            log += f'\ntimed out after {TIMEOUT}s'
        (out / 'app.log').write_text(log)
        return f'settings {scene} PASS' in log, log
    finally:
        subprocess.run(['pkill', '-f', f'serve --runtime {root}/rt'], check=False)
        shutil.rmtree(root, ignore_errors=True)

failed = []
for scene in SCENES:
    ok, log = run(scene)
    if not ok:
        print(f'{scene}: failed, once more\n{log[-1500:]}', flush=True)
        ok, log = run(scene)
    print(f'{scene}: {"PASS" if ok else "FAIL"}', flush=True)
    if not ok:
        print(log[-3000:])
        failed.append(scene)
sys.exit(1 if failed else 0)
