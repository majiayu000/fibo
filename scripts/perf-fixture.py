#!/usr/bin/env python3
"""A large isolated workbench for the window's CPU probe: N Codex sessions over 8 spaces,
T terminals in tabs of three panes, each bound to a session so the sidebar lists them
(2000 and 60 by default, near a long-used install), each log with an account-limit
reading as Codex writes one. No model calls, credentials or user
data. Needs `cargo build --release -p fibo-host -p fibo-app --features
fibo-app/visual-test`; BIN_DIR is where those binaries are.

  python3 scripts/perf-fixture.py prepare RUN_DIR BIN_DIR [N] [T] [SPIN]  # SPIN terminals print a spinner
  python3 scripts/perf-fixture.py probe RUN_DIR BIN_DIR OUT frames-pane,frames-main,redraw-check
  python3 scripts/perf-fixture.py live RUN_DIR BIN_DIR [SECS]   # the real window on screen, CPU over SECS
  python3 scripts/perf-fixture.py stop RUN_DIR

Use a fresh RUN_DIR for each prepare; stop it even when a probe fails.
The recorded process groups also cover children left after a probe quits the host. MOORING_NO_CATCH_UP=1
is passed through for redraw-check; NORUN=1 at prepare leaves no turn running, so no
clock ticks the sidebar. FEWNEED=1 leaves little to act on, so the running rows fit the
list's limit. MACHINE=1 at prepare configures a machine that never connects.
"""
import json, os, pathlib, signal, socket, struct, subprocess, sys, time
def read(s, n):
    b = b''
    while len(b) < n:
        c = s.recv(n - len(b)); assert c, 'closed'; b += c
    return b
def send(s, o):
    b = b'\1' + json.dumps(o).encode(); s.sendall(struct.pack('>I', len(b)) + b)
def receive(s): return json.loads(read(s, struct.unpack('>I', read(s, 4))[0])[1:])
def rpc(root, o):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(30); s.connect(str(pathlib.Path(root) / 'runtime/workbench.sock'))
        send(s, {'Hello': {'major': 3}}); receive(s); send(s, o); return receive(s)
def edit(root, o):
    r = rpc(root, {'Edit': {'edit': o}}); assert 'Error' not in r, r; return r
def processes():
    rows = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,pgid=,stat=,lstart='], text=True)
    return {int(p): (int(parent), int(group), status, started.strip())
            for p, parent, group, status, started in (line.split(None, 4) for line in rows.splitlines())}


def owned_processes(table, roots):
    owned = set(roots)
    while True:
        more = {p for p, (parent, *_) in table.items() if parent in owned} - owned
        if not more:
            return {p: table[p] for p in owned if p in table}
        owned.update(more)


def stop_tree(pid, recorded=None):
    """Only this fixture's process groups, including children whose host already exited."""
    table = processes()
    if recorded is None:
        recorded = owned_processes(table, [pid])
    roots = [p for p, row in recorded.items() if p in table and table[p][3] == row[3]]
    targets = owned_processes(table, roots)
    for sig in (signal.SIGTERM, signal.SIGKILL):
        current = processes()
        alive = {p: row for p, row in targets.items()
                 if p in current and current[p][3] == row[3] and 'Z' not in current[p][2]}
        if not alive:
            return
        for group in {row[1] for row in alive.values()}:
            if group == os.getpgrp():
                raise RuntimeError('fixture unexpectedly shares the caller process group')
            try:
                os.killpg(group, sig)
            except ProcessLookupError:
                pass
            except PermissionError:
                # XNU killpg returns EPERM when a group's remaining members became zombies
                # between the ps snapshot and killpg (e.g. PTY SIGHUP as the host exits).
                for member, row in alive.items():
                    if row[1] == group:
                        try:
                            os.kill(member, sig)
                        except ProcessLookupError:
                            pass
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            current = processes()
            if not any(p in current and current[p][3] == row[3] and 'Z' not in current[p][2]
                       for p, row in targets.items()):
                return
            time.sleep(.1)
    raise RuntimeError(f'fixture processes did not exit: {pid}')


def stop(root):
    state_file = root / 'state.json'
    if not state_file.exists():
        return  # prepare failed before it started a host
    state = json.loads(state_file.read_text())
    stop_tree(state['host'], {int(p): row for p, row in state['processes'].items()})
    state_file.unlink()


def interrupted(signum, _frame):
    raise KeyboardInterrupt(f'interrupted by signal {signum}')


names = ['工作空间', 'sys', '模型上线 work', 'mooring', 'codex-mall', 'blog', 'notes', 'infra']
def prepare(root, bindir, N=2000, T=60, SPIN=0):
    root.mkdir(parents=True, exist_ok=True)
    home = root / "home"
    host = None
    try:
        logs = home / '.codex/sessions'; logs.mkdir(parents=True); (home / '.claude/projects').mkdir(parents=True)
        now = int(time.time())
        for n in range(N):
            sp = names[n % 8]; project = root / 'projects' / sp / (f'sub{n % 5}' if n % 3 else ''); project.mkdir(parents=True, exist_ok=True)
            stamp = now - 60 * (n + 1) * 3; cid = f'perf-{n}'
            rec = [(stamp - 3600, 'session_meta', {'id': cid, 'cwd': str(project), 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'}),
                   (stamp - 3600, 'event_msg', {'type': 'user_message', 'message': f'任务 {n}：修一下 {sp} 里的问题'}),
                   (stamp - 3600, 'event_msg', {'type': 'task_started', 'turn_id': cid + '-turn'}),
                   (stamp - 1800, 'event_msg', {'type': 'token_count', 'rate_limits': {'primary': {'used_percent': 40.0, 'window_minutes': 300, 'resets_at': now + 3600}, 'secondary': None}})]
            g = n % 10
            if os.environ.get('FEWNEED') and g != 9: g = 0 if n % 500 == 0 else 8  # few to act on, so the rows running are listed
            if g == 0: rec.append((stamp, 'event_msg', {'type': 'request_user_input', 'message': '要不要继续？', 'questions': [{'question': '要不要继续？', 'options': [{'label': '继续'}, {'label': '停'}]}]}))
            elif g < 7: rec.append((stamp, 'event_msg', {'type': 'task_complete', 'last_agent_message': '已完成，请核对。'}))
            elif g != 9 or os.environ.get('NORUN'): rec.append((stamp, 'event_msg', {'type': 'turn_aborted'}))
            (logs / (cid + '.jsonl')).write_text(''.join(json.dumps({'timestamp': at, 'type': t, 'payload': p}, ensure_ascii=False) + '\n' for at, t, p in rec))
            os.utime(logs / (cid + '.jsonl'), (stamp, stamp))
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
        with (root / 'host.log').open('w') as log:
            host = subprocess.Popen([str(bindir / 'fibo'), 'serve', '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
        state = {'root': str(root), 'home': str(home), 'host': host.pid, 'processes': owned_processes(processes(), [host.pid])}
        (root / 'state.json').write_text(json.dumps(state))
        deadline = time.monotonic() + 300
        while not (root / 'runtime/workbench.sock').exists():
            assert host.poll() is None and time.monotonic() < deadline; time.sleep(.05)
        (root / 'codex').symlink_to('/bin/sh')
        for n, name in enumerate(names): edit(root, {'AddSpace': {'id': f's{n}', 'name': name, 'cwd': str(root / 'projects' / name)}})
        for n in range(T):
            sp = n % 8
            spin = 'i=0; while :; do i=$((i+1)); printf "\\r· Thinking… %d tokens" $i; sleep 0.1; done'
            argv = [str(root / 'codex'), '-c', spin if n < SPIN else 'while :; do sleep 3600; done']
            r = rpc(root, {'Launch': {'id': f'p{n}', 'space': f's{sp}', 'argv': argv, 'cwd': str(root / 'projects' / names[sp]), 'cols': 100, 'rows': 35}}); assert 'Error' not in r, r
        for k in range(T // 4):
            ids = [f'p{4*k+i}' for i in range(4)]
            lay = {'Split': {'axis': 'Right', 'ratio': 5000, 'first': {'Leaf': ids[0]}, 'second': {'Split': {'axis': 'Down', 'ratio': 5000, 'first': {'Leaf': ids[1]}, 'second': {'Leaf': ids[2]}}}}}
            edit(root, {'CreateTab': {'tab': {'id': f'tab{k}', 'space': f's{(4*k) % 8}', 'title': f'标签 {k}', 'default_task': None, 'closed': False, 'layout': lay}}})
        while True:
            w = rpc(root, 'Workspace')['Workspace']
            if len(w['agent_sessions']) >= N: break
            assert time.monotonic() < deadline, ('index timeout', len(w['agent_sessions'])); time.sleep(.5)
        for n in range(min(T, N)):
            t = next(t for t in w['terminals'] if t['id'] == f'p{n}')
            r = rpc(root, {'Hook': {'runtime': 'Codex', 'pid': t['pid'], 'cli_session_id': f'perf-{n}', 'transcript': str(logs / f'perf-{n}.jsonl'), 'ended': False, 'event': None}}); assert 'Error' not in r, r
        w = rpc(root, 'Workspace')['Workspace']
        bound = sum(1 for s in w['agent_sessions'] if s.get('terminal'))
        if os.environ.get('MACHINE'):  # a machine that never connects, for the column's layouts
            config = home / '.config/fibo'; config.mkdir(parents=True, exist_ok=True)
            (config / 'machines.json').write_text('[{"host": "fibo-perf-nowhere.invalid"}]')
        state['processes'] = owned_processes(processes(), [host.pid])
        (root / 'state.json').write_text(json.dumps(state))
        print('bound', bound)
        print({k: state[k] for k in ('root', 'home', 'host')}, len(w['agent_sessions']), 'sessions', len(w['terminals']), 'terminals', len(w['tabs']), 'tabs')
    except BaseException:
        if host is not None:
            stop_tree(host.pid)
            host.wait(timeout=5)
            (root / "state.json").unlink(missing_ok=True)
        raise


def main():
    signal.signal(signal.SIGTERM, interrupted)
    cmd = sys.argv[1]
    root = pathlib.Path(sys.argv[2]).resolve()
    if cmd == 'prepare':
        prepare(root, pathlib.Path(sys.argv[3]).resolve(), *map(int, sys.argv[4:]))
        return
    if cmd == 'stop':
        stop(root)
        return
    s = json.loads((root / 'state.json').read_text()); home = pathlib.Path(s['home'])
    if cmd == 'probe':
        bindir = pathlib.Path(sys.argv[3]); out = pathlib.Path(sys.argv[4]).resolve(); out.mkdir(parents=True, exist_ok=True)
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8', 'GPUI_PAUSE_IDLE_DISPLAY_LINK': '1', 'MOORING_CPU_PROBE': sys.argv[5], 'PERF_DROP': os.environ.get('PERF_DROP', ''), **({'MOORING_PERF_NO_COUNTER': '1'} if os.environ.get('MOORING_PERF_NO_COUNTER') else {}), **({'MOORING_NO_CATCH_UP': '1'} if os.environ.get('MOORING_NO_CATCH_UP') else {})}
        # CI caches target/, where the gate keeps its output: a result from an older run must
        # not stand in for this one when the probe dies before writing its own.
        (out / 'cpu.txt').unlink(missing_ok=True)
        r = subprocess.run([str(bindir / 'fibo-app'), '--runtime', str(root / 'runtime'), '--visual-evidence', str(out)], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
        (out / 'app.log').write_bytes(r.stdout)
        ok = r.returncode == 0 and (out / 'cpu.txt').exists()
        print((out / 'cpu.txt').read_text() if ok else f'probe exited {r.returncode}:\n' + r.stdout.decode()[-3000:])
    elif cmd == 'live':
        bindir = pathlib.Path(sys.argv[3]); secs = int(sys.argv[4]) if len(sys.argv) > 4 else 20
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
        app = subprocess.Popen([str(bindir / 'fibo-app'), '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def cpu():
            t = subprocess.run(['ps', '-o', 'cputime=', '-p', str(app.pid)], capture_output=True, text=True).stdout.strip()
            m, sec = t.split(':'); return int(m) * 60 + float(sec)
        try:
            time.sleep(10); a = cpu(); t0 = time.monotonic(); time.sleep(secs); b = cpu(); dt = time.monotonic() - t0
            print(f'live: {(b - a) / dt * 100:.1f}% of a core over {dt:.0f}s')
        finally:
            app.terminate(); app.wait(timeout=10)


if __name__ == '__main__':
    main()
