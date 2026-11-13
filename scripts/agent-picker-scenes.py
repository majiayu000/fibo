#!/usr/bin/env python3
"""Native Agent picker selection, scrolling and restart readback in isolated runtimes.

  python3 scripts/agent-picker-scenes.py BIN_DIR OUT

Requires fibo and fibo-app built with fibo-app/visual-test. No model calls.
"""
import json
import pathlib
import subprocess
import sys
import tempfile
import time

binaries, out = (pathlib.Path(arg).resolve() for arg in sys.argv[1:])
out.mkdir(parents=True, exist_ok=True)
for added, phases in [(0, ['select', 'readback']), (8, ['scroll', 'scroll-readback'])]:
    with tempfile.TemporaryDirectory(prefix='fibo-picker-', dir='/tmp') as directory:
        root = pathlib.Path(directory)
        config = root / 'cfg/fibo'
        for path in [config / 'agents', root / 'home', root / 'claude', root / 'rt']:
            path.mkdir(parents=True)
        (root / 'rt').chmod(0o700)
        (config / 'first-run-done').touch()
        for i in range(added):
            agent = f'fixture-{i}'
            (config / 'agents' / f'{agent}.json').write_text(json.dumps({
                'api': 1, 'id': agent, 'name': f'Fixture Agent {i}',
                'executable': agent, 'start': agent, 'resume': agent + ' {id}',
            }))
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'en_US.UTF-8',
               'HOME': str(root / 'home'), 'XDG_CONFIG_HOME': str(root / 'cfg'),
               'CODEX_HOME': str(root / 'home/.codex'),
               'CLAUDE_CONFIG_DIR': str(root / 'claude'), 'MOORING_LANG': 'zh'}
        for phase in phases:
            scene = out / phase
            scene.mkdir(parents=True, exist_ok=True)
            with (scene / 'host.log').open('w') as host_log:
                host = subprocess.Popen([str(binaries / 'fibo'), 'serve', '--runtime', str(root / 'rt')],
                                        env=env, cwd=root, stdout=host_log, stderr=subprocess.STDOUT)
                try:
                    deadline = time.monotonic() + 20
                    while not (root / 'rt/workbench.sock').exists():
                        assert host.poll() is None and time.monotonic() < deadline, 'host did not start'
                        time.sleep(0.05)
                    with (scene / 'app.log').open('w') as app_log:
                        result = subprocess.run([
                            str(binaries / 'fibo-app'), '--runtime', str(root / 'rt'),
                            '--visual-evidence', str(scene),
                        ], env={**env, 'MOORING_AGENT_PICKER_EVIDENCE': phase}, cwd=root,
                            stdout=app_log, stderr=subprocess.STDOUT, timeout=90)
                    log = (scene / 'app.log').read_text()
                    assert result.returncode == 0 and f'agent picker {phase} PASS' in log, log[-3000:]
                    print(f'{phase}: PASS', flush=True)
                finally:
                    host.terminate()
                    try:
                        host.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        host.kill()
                        host.wait()
