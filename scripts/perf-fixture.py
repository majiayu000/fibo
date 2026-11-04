#!/usr/bin/env python3
"""A large isolated workbench for the window's CPU probe: N Codex sessions over 8 spaces,
T terminals in tabs of three panes, each bound to a session so the sidebar lists them
(2000 and 60 by default, near a long-used install), each log with an account-limit
reading as Codex writes one. No model calls, credentials or user
data. Needs `cargo build --release -p mooring-host -p mooring-app --features
mooring-app/visual-test`; BIN_DIR is where those binaries are.

  python3 scripts/perf-fixture.py prepare BIN_DIR [N] [T] [SPIN]  # SPIN terminals print a spinner
  python3 scripts/perf-fixture.py probe BIN_DIR OUT frames-pane,frames-main,redraw-check
  python3 scripts/perf-fixture.py live BIN_DIR [SECS]   # the real window on screen, CPU over SECS
  python3 scripts/perf-fixture.py stop

The probe quits the host it used: prepare again before each probe. MOORING_NO_CATCH_UP=1
is passed through for redraw-check; NORUN=1 at prepare leaves no turn running, so no
clock ticks the sidebar. MACHINE=1 at prepare configures a machine that never connects.
"""
import json, os, pathlib, shlex, socket, struct, subprocess, sys, tempfile, time
STATE = pathlib.Path.home() / 'Library/Caches/MooringPerf/state.json'
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
names = ['工作空间', 'sys', '模型上线 work', 'mooring', 'codex-mall', 'blog', 'notes', 'infra']
cmd = sys.argv[1]
if cmd == 'prepare':
    bindir = pathlib.Path(sys.argv[2]); N = int(sys.argv[3]) if len(sys.argv) > 3 else 2000; T = int(sys.argv[4]) if len(sys.argv) > 4 else 60; SPIN = int(sys.argv[5]) if len(sys.argv) > 5 else 0
    STATE.parent.mkdir(parents=True, exist_ok=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix='p-', dir=STATE.parent)); home = pathlib.Path(tempfile.mkdtemp(prefix='mph-', dir='/tmp'))
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
        if g == 0: rec.append((stamp, 'event_msg', {'type': 'request_user_input', 'message': '要不要继续？', 'questions': [{'question': '要不要继续？', 'options': [{'label': '继续'}, {'label': '停'}]}]}))
        elif g < 7: rec.append((stamp, 'event_msg', {'type': 'task_complete', 'last_agent_message': '已完成，请核对。'}))
        elif g != 9 or os.environ.get('NORUN'): rec.append((stamp, 'event_msg', {'type': 'turn_aborted'}))
        (logs / (cid + '.jsonl')).write_text(''.join(json.dumps({'timestamp': at, 'type': t, 'payload': p}, ensure_ascii=False) + '\n' for at, t, p in rec))
        os.utime(logs / (cid + '.jsonl'), (stamp, stamp))
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
    host = subprocess.Popen([str(bindir / 'mooring'), 'serve', '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=open(root / 'host.log', 'w'), start_new_session=True)
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
        config = home / '.config/mooring'; config.mkdir(parents=True, exist_ok=True)
        (config / 'machines.json').write_text('[{"host": "mooring-perf-nowhere.invalid"}]')
    state = {'root': str(root), 'home': str(home), 'host': host.pid}; STATE.write_text(json.dumps(state))
    print('bound', bound)
    print(state, len(w['agent_sessions']), 'sessions', len(w['terminals']), 'terminals', len(w['tabs']), 'tabs')
else:
    s = json.loads(STATE.read_text()); root = pathlib.Path(s['root']); home = pathlib.Path(s['home'])
    if cmd == 'probe':
        bindir = pathlib.Path(sys.argv[2]); out = pathlib.Path(sys.argv[3]).resolve(); out.mkdir(parents=True, exist_ok=True)
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8', 'GPUI_PAUSE_IDLE_DISPLAY_LINK': '1', 'MOORING_CPU_PROBE': sys.argv[4], 'PERF_DROP': os.environ.get('PERF_DROP', ''), **({'MOORING_NO_CATCH_UP': '1'} if os.environ.get('MOORING_NO_CATCH_UP') else {})}
        r = subprocess.run([str(bindir / 'mooring-app'), '--runtime', str(root / 'runtime'), '--visual-evidence', str(out)], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
        (out / 'app.log').write_bytes(r.stdout)
        print((out / 'cpu.txt').read_text() if (out / 'cpu.txt').exists() else r.stdout.decode()[-3000:])
    elif cmd == 'live':
        bindir = pathlib.Path(sys.argv[2]); secs = int(sys.argv[3]) if len(sys.argv) > 3 else 20
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
        app = subprocess.Popen([str(bindir / 'mooring-app'), '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def cpu():
            t = subprocess.run(['ps', '-o', 'cputime=', '-p', str(app.pid)], capture_output=True, text=True).stdout.strip()
            m, sec = t.split(':'); return int(m) * 60 + float(sec)
        time.sleep(10); a = cpu(); t0 = time.monotonic(); time.sleep(secs); b = cpu(); dt = time.monotonic() - t0
        print(f'live: {(b - a) / dt * 100:.1f}% of a core over {dt:.0f}s'); app.terminate(); app.wait()
    elif cmd == 'stop':
        try:
            for t in rpc(root, 'Workspace')['Workspace']['terminals']: rpc(root, {'Stop': {'id': t['id']}})
        except Exception as e: print(e)
        try: os.kill(s['host'], 15)
        except ProcessLookupError: pass
