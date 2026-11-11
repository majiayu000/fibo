#!/usr/bin/env python3
"""Run the native tab geometry regression against an isolated real host (no model calls).

  python3 scripts/tab-scenes.py BIN_DIR OUT

BIN_DIR must contain mooring and mooring-app built with mooring-app/visual-test.
Any assertion, timeout, missing completion marker or process failure fails the run.
"""
import json
import os
import pathlib
import socket
import struct
import subprocess
import sys
import tempfile
import time


def rpc(runtime, request):
    def read(connection, count):
        data = b''
        while len(data) < count:
            chunk = connection.recv(count - len(data))
            assert chunk, 'host closed the connection'
            data += chunk
        return data

    def exchange(connection, message):
        data = b'\1' + json.dumps(message).encode()
        connection.sendall(struct.pack('>I', len(data)) + data)
        size = struct.unpack('>I', read(connection, 4))[0]
        reply = json.loads(read(connection, size)[1:])
        assert 'Error' not in reply, reply
        return reply

    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(10)
        connection.connect(str(runtime / 'workbench.sock'))
        exchange(connection, {'Hello': {'major': 3}})
        return exchange(connection, request)


def main():
    binaries, out = (pathlib.Path(arg).resolve() for arg in sys.argv[1:])
    out.mkdir(parents=True, exist_ok=True)
    result_file = out / 'result.txt'
    result_file.unlink(missing_ok=True)  # A previous success must not mask an empty run.
    # Only runtime data goes here; screenshots and logs stay in OUT. Keep sockets short.
    with tempfile.TemporaryDirectory(prefix='fibo-tabs-', dir='/tmp') as directory:
        root = pathlib.Path(directory)
        for name in ('home/.config/fibo', 'home/.codex/sessions', 'home/.claude/projects', 'rt'):
            (root / name).mkdir(parents=True)
        (root / 'home/.config/fibo/first-run-done').touch()
        runtime = root / 'rt'
        runtime.chmod(0o700)
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'LANG': 'en_US.UTF-8',
               'HOME': str(root / 'home'), 'CODEX_HOME': str(root / 'home/.codex'),
               'CLAUDE_CONFIG_DIR': str(root / 'home/.claude'),
               'MOORING_TAB_GEOMETRY_EVIDENCE': '1', 'GPUI_PAUSE_IDLE_DISPLAY_LINK': '1'}
        with (out / 'host.log').open('w') as log:
            host = subprocess.Popen([str(binaries / 'mooring'), 'serve', '--runtime', str(runtime)],
                                    env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 30
                while not (runtime / 'workbench.sock').exists():
                    assert host.poll() is None, 'fixture host exited'
                    assert time.monotonic() < deadline, 'fixture host timed out'
                    time.sleep(0.05)
                rpc(runtime, {'Edit': {'edit': {'AddSpace': {
                    'id': 's0', 'name': 'Tab regression', 'cwd': str(root)}}}})
                for n, title in enumerate(('zsh', 'tools', 'A long terminal tab title that must truncate')):
                    rpc(runtime, {'Launch': {'id': f'p{n}', 'space': 's0',
                        'argv': ['/bin/sh', '-c', 'printf "Tab fixture\\n"; while IFS= read -r line; do :; done'],
                        'cwd': str(root), 'cols': 80, 'rows': 24}})
                    rpc(runtime, {'Edit': {'edit': {'CreateTab': {'tab': {
                        'id': 'feedback' if n == 0 else f'tab{n}', 'space': 's0', 'title': title,
                        'default_task': None, 'layout': {'Leaf': f'p{n}'}}}}}})
                with (out / 'app.log').open('w') as app_log:
                    run = subprocess.run([str(binaries / 'mooring-app'), '--runtime', str(runtime),
                        '--visual-evidence', str(out)], env=env, stdout=app_log,
                        stderr=subprocess.STDOUT, timeout=180)
                assert run.returncode == 0, f'native tab geometry regression failed; see {out / "app.log"}'
                assert result_file.is_file(), 'native tab geometry regression did not run'
                assert result_file.read_text().startswith('PASS:'), result_file.read_text()
                print(result_file.read_text())
            finally:
                if host.poll() is None:
                    host.terminate()
                try:
                    host.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    host.kill()
                    host.wait()


if __name__ == '__main__':
    main()
