#!/usr/bin/env python3
"""A workbench for the website and promo film: English sample data, five agents side by side.

Same machinery as visual-fixture.py (private HOME and runtime, synthetic logs, no model calls,
credentials or user data), but the sessions, spaces and terminal screens read as a day of work
on an English-speaking team, so screenshots can be shown to strangers. Needs a debug build with
visual tests: `cargo build -p mooring-host -p mooring-app --features mooring-app/visual-test`.

  python3 scripts/promo-fixture.py prepare
  MOORING_LANG=en MOORING_PAGE_LIST=Main,Overview python3 scripts/promo-fixture.py capture DIR
  python3 scripts/promo-fixture.py stop
"""
import json, os, random, uuid, pathlib, shlex, socket, struct, subprocess, sys, tempfile, time

BASE = pathlib.Path(__file__).resolve().parents[1]
STATE = pathlib.Path.home() / 'Library/Caches/MooringVisual/promo-state.json'
BIN = pathlib.Path(os.environ.get('MOORING_VISUAL_BIN', BASE / 'target/debug'))


def read(s, n):
    b = b''
    while len(b) < n:
        chunk = s.recv(n - len(b))
        assert chunk, 'host closed the connection'
        b += chunk
    return b


def rpc(root, obj):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(10)
        s.connect(str(pathlib.Path(root) / 'runtime/workbench.sock'))
        for message in ({'Hello': {'major': 3}}, obj):
            b = b'\1' + json.dumps(message).encode()
            s.sendall(struct.pack('>I', len(b)) + b)
            reply = json.loads(read(s, struct.unpack('>I', read(s, 4))[0])[1:])
        return reply


def edit(root, obj):
    r = rpc(root, {'Edit': {'edit': obj}})
    assert 'Error' not in r, r
    return r


SPACES = ['fibo', 'api-server', 'web', 'docs', 'infra']
# (space, agent, title, state, screen lines). state: needs | review | running | aborted | done
SESSIONS = [
    (0, 'Codex', 'Fix flaky reconnect test in host handover', 'needs',
     ['› Fix flaky reconnect test in host handover', '', '• Read crates/host/src/handover.rs (412 lines)',
      '• Ran cargo test -p host handover -- --nocapture', '  └ test result: FAILED. 23 passed; 1 failed',
      '• The socket is reused before the old host lets go of it.', '',
      '? Two ways to fix this:', '  1. Wait for the old host to close (adds ~200 ms)', '  2. Hand the socket over explicitly',
      '', '  Which one should I take?']),
    (0, 'Claude', 'Add English UI for the settings window', 'running',
     ['> Add English UI for the settings window', '', '⏺ Reading crates/app/src/settings/*.rs',
      '⏺ Update(settings/general.rs)', '  ⎿ Wrapped 38 strings in t!()', '⏺ Update(locales/en/settings.toml)',
      '  ⎿ Added 38 entries', '⏺ Bash(cargo test -p core i18n)', '  ⎿ running 12 tests …', '',
      '✻ Translating the plugins page… (2m 14s · ↓ 8.1k tokens)']),
    (0, 'Codex', 'Speed up sidebar redraw on large workspaces', 'review',
     ['› Speed up sidebar redraw on large workspaces', '', '• Profiled frame time with 400 sessions: 31 ms → 9 ms',
      '• Cached per-second derived values in frame_facts', '• Ran scripts/perf-gate.py', '  └ sidebar-scroll  p50 8.7 ms  (gate 12 ms)  ✓',
      '• Ran cargo test -p app', '  └ test result: ok. 214 passed; 0 failed', '', '─ Done. Ready for your review.']),
    (1, 'Claude', 'Rate-limit the public upload endpoint', 'running',
     ['> Rate-limit the public upload endpoint', '', '⏺ Read src/routes/upload.ts', '⏺ Write(src/middleware/rateLimit.ts)',
      '  ⎿ Token bucket, 60 req/min per key', '⏺ Bash(bun test upload)', '  ⎿ 31 pass  0 fail', '',
      '✻ Writing the load test… (48s · ↓ 3.2k tokens)']),
    (2, 'Codex', 'Dark mode for the pricing page', 'review',
     ['› Dark mode for the pricing page', '', '• Added color tokens under prefers-color-scheme', '• Checked contrast: 7.1:1 body, 4.8:1 muted',
      '• Ran pnpm test', '  └ 58 passed', '', '─ Done. Screenshots in ./shots/pricing-dark.png']),
    (1, 'Codex', 'Migrate sessions table to UUID keys', 'needs', []),
    (2, 'Claude', 'Replace hero animation with a static image on mobile', 'review', []),
    (3, 'Claude', 'Write the getting-started guide', 'done', []),
    (4, 'Codex', 'Pin the Zig toolchain in CI', 'done', []),
    (1, 'Claude', 'Retry webhooks with backoff', 'running', []),
    (3, 'Codex', 'Fix broken links in the changelog', 'review', []),
    (4, 'Claude', 'Rotate the relay certificate automatically', 'aborted', []),
    (2, 'Codex', 'Lazy-load the screenshots gallery', 'done', []),
    (0, 'Claude', 'Notarize the DMG in the release script', 'done', []),
    (3, 'Codex', 'Explain the plugin manifest fields', 'aborted', []),
    (1, 'Codex', 'Add request IDs to every log line', 'review', []),
]
LIVE = [0, 1, 2, 3, 4]


def codex_records(cid, cwd, title, state, stamp):
    r = [(stamp - 420, 'session_meta', {'id': cid, 'cwd': cwd, 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'}),
         (stamp - 420, 'event_msg', {'type': 'user_message', 'message': title}),
         (stamp - 420, 'event_msg', {'type': 'task_started', 'turn_id': cid + '-turn'})]
    if state == 'needs':
        r.append((stamp, 'event_msg', {'type': 'request_user_input', 'message': 'Two ways to fix this. Which one should I take?',
                                       'questions': [{'question': 'Which fix?', 'options': [{'label': 'Wait for the old host'}, {'label': 'Hand the socket over'}]}]}))
    elif state in ('review', 'done'):
        r.append((stamp, 'event_msg', {'type': 'task_complete', 'last_agent_message': 'Done. Ready for your review.'}))
    elif state == 'aborted':
        r.append((stamp, 'event_msg', {'type': 'turn_aborted'}))
    return ''.join(json.dumps({'timestamp': at, 'type': t, 'payload': p}) + '\n' for at, t, p in r)


def iso(t):
    return time.strftime('%Y-%m-%dT%H:%M:%S.000Z', time.gmtime(t))


def claude_records(cid, cwd, title, state, stamp):
    r = [{'type': 'user', 'sessionId': cid, 'uuid': cid + '-u0', 'cwd': cwd, 'entrypoint': 'cli', 'userType': 'external',
          'timestamp': iso(stamp - 420), 'message': {'role': 'user', 'content': title}}]
    if state in ('review', 'done', 'aborted'):
        r.append({'type': 'assistant', 'sessionId': cid, 'uuid': cid + '-a0', 'parentUuid': cid + '-u0', 'cwd': cwd, 'timestamp': iso(stamp),
                  'message': {'role': 'assistant', 'content': [{'type': 'text', 'text': 'Done. Ready for your review.'}], 'stop_reason': 'end_turn'}})
    return ''.join(json.dumps(x) + '\n' for x in r)


if sys.argv[1] == 'prepare':
    STATE.parent.mkdir(parents=True, exist_ok=True)
    # Project folders appear in screenshots: keep the runtime outside the real user's home.
    root = pathlib.Path(tempfile.mkdtemp(prefix='fibo-promo-', dir='/tmp')); home = pathlib.Path(tempfile.mkdtemp(prefix='promo-home-', dir='/tmp'))
    codex_logs = home / '.codex/sessions'; codex_logs.mkdir(parents=True)
    now = int(time.time()); logs = {}
    for n, (space, agent, title, state, _) in enumerate(SESSIONS):
        project = root / 'projects' / SPACES[space]; project.mkdir(parents=True, exist_ok=True)
        stamp = now - 90 * (n + 1) - (5 * 86400 if state == 'done' and n > 10 else 0)
        cid = str(uuid.UUID(int=random.Random(n * 7919).getrandbits(128), version=4))
        if agent == 'Codex':
            path = codex_logs / f'rollout-{cid}.jsonl'; path.write_text(codex_records(cid, str(project), title, state, stamp))
        else:
            folder = home / '.claude/projects' / ('-' + SPACES[space]); folder.mkdir(parents=True, exist_ok=True)
            path = folder / f'{cid}.jsonl'; path.write_text(claude_records(cid, str(project), title, state, stamp))
        os.utime(path, (stamp, stamp)); logs[n] = (cid, path)
    for project in (root / 'projects').iterdir():
        subprocess.run(['git', '-C', str(project), 'init', '-q', '-b', 'main'], check=True)
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
    host = subprocess.Popen([str(BIN / 'mooring'), 'serve', '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=open(root / 'host.log', 'w'), start_new_session=True)
    deadline = time.monotonic() + 30
    while not (root / 'runtime/workbench.sock').exists():
        assert host.poll() is None and time.monotonic() < deadline; time.sleep(.05)
    for n, name in enumerate(SPACES): edit(root, {'AddSpace': {'id': f's{n}', 'name': name, 'cwd': str(root / 'projects' / name)}})
    edit(root, {'AddTask': {'id': 'task0', 'space': 's0', 'title': 'Ship v0.1 to the website', 'description': 'Release build, notarized DMG, Homebrew cask, and the new site at fibo.page.',
                            'acceptance': [{'id': f'c{i}', 'description': d, 'evidence': []} for i, d in enumerate(['DMG opens on a clean Mac', 'brew install --cask fibo works', 'fibo.page shows the download button'])], 'agent': 'Codex'}})
    for name in ('codex', 'claude'): (root / name).symlink_to('/bin/sh')
    for n in LIVE:
        space, agent, title, state, lines = SESSIONS[n]
        cid, path = logs[n]
        # The fake agent holds its log open, as a real one does, and prints its screen.
        script = 'exec 3<' + shlex.quote(str(path)) + '; printf "\\033[?25l"; printf "%s\\n" ' + ' '.join(shlex.quote(s) for s in lines) + '; while IFS= read -r line; do :; done'
        r = rpc(root, {'Launch': {'id': f'p{n}', 'space': f's{space}', 'argv': [str(root / agent.lower()), '-c', script], 'cwd': str(root / 'projects' / SPACES[space]), 'cols': 90, 'rows': 30}}); assert 'Error' not in r, r
    def leaf(p): return {'Leaf': p}
    def split(a, b, axis, ratio=5000): return {'Split': {'axis': axis, 'ratio': ratio, 'first': a, 'second': b}}
    # Space s0 shows three panes; the other two live agents sit in their own spaces' tabs.
    edit(root, {'CreateTab': {'tab': {'id': 'feedback', 'space': 's0', 'title': '', 'default_task': None, 'closed': False,
                                     'layout': split(leaf('p0'), split(leaf('p1'), leaf('p2'), 'Down'), 'Right', 5200)}}})
    edit(root, {'CreateTab': {'tab': {'id': 'api', 'space': 's1', 'title': '', 'default_task': None, 'closed': False, 'layout': leaf('p3')}}})
    edit(root, {'CreateTab': {'tab': {'id': 'web', 'space': 's2', 'title': '', 'default_task': None, 'closed': False, 'layout': leaf('p4')}}})
    while True:
        w = rpc(root, 'Workspace')['Workspace']
        if len(w['agent_sessions']) >= len(SESSIONS) and all(t.get('pid') for t in w['terminals']): break
        assert time.monotonic() < deadline + 30, ('index timeout', len(w['agent_sessions'])); time.sleep(.2)
    time.sleep(1)
    for n in LIVE:
        space, agent, title, state, _ = SESSIONS[n]
        cid, path = logs[n]
        t = next(t for t in w['terminals'] if t['id'] == f'p{n}')
        r = rpc(root, {'Hook': {'runtime': agent, 'pid': t['pid'], 'cli_session_id': cid, 'transcript': str(path), 'ended': False, 'event': None}}); assert 'Error' not in r, r
    time.sleep(1)
    for s in rpc(root, 'Workspace')['Workspace']['agent_sessions']:
        n = next((n for n, (cid, _) in logs.items() if cid == s['cli_session_id']), None)
        if n is None: continue
        if SESSIONS[n][3] == 'done': edit(root, {'SetOutcome': {'id': s['id'], 'outcome': 'Completed', 'seen': None}})
    state = {'root': str(root), 'home': str(home), 'host': host.pid}; STATE.write_text(json.dumps(state)); print(state)
else:
    s = json.loads(STATE.read_text()); root = pathlib.Path(s['root']); home = pathlib.Path(s['home'])
    if sys.argv[1] == 'capture':
        directory = pathlib.Path(sys.argv[2]).resolve(); directory.mkdir(parents=True, exist_ok=True)
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8',
               'GPUI_PAUSE_IDLE_DISPLAY_LINK': '1', 'MOORING_PAGE_EVIDENCE': '1', 'MOORING_PAGE_LIST': os.environ.get('MOORING_PAGE_LIST', 'Main,Overview')}
        if os.environ.get('MOORING_LANG'): env['MOORING_LANG'] = os.environ['MOORING_LANG']
        result = subprocess.run([str(BIN / 'mooring-app'), '--runtime', str(root / 'runtime'), '--visual-evidence', str(directory)], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180)
        (directory / 'native-render.log').write_bytes(result.stdout); assert result.returncode == 0, result.stdout.decode()[-4000:]
        print('native captures', sorted(p.name for p in directory.glob('*-native.png')))
    elif sys.argv[1] == 'workspace':
        w = rpc(root, 'Workspace')['Workspace']
        print(json.dumps([{k: a.get(k) for k in ('cli_session_id', 'runtime', 'title', 'state', 'terminal')} for a in w['agent_sessions']], indent=1)[:4000])
    elif sys.argv[1] == 'stop':
        try:
            for t in rpc(root, 'Workspace')['Workspace']['terminals']: rpc(root, {'Stop': {'id': t['id']}})
            os.kill(s['host'], 15)
        except (ConnectionRefusedError, FileNotFoundError, ProcessLookupError):
            pass
