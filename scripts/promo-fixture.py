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
    (0, 'Claude', 'Ship v0.1: review the branch, write the notes', 'running',
     ['> Ship v0.1: review the branch, write the notes', '',
      '⏺ Bash(fibo run codex "review this branch")',
      '  ⎿ started Codex in pane review-1',
      '⏺ Bash(fibo run claude "write the release notes")',
      '  ⎿ started Claude Code in pane notes-1', '',
      '<mooring from="review-1" agent="Codex" event="done">',
      'Reviewed 14 files. One real bug: the updater skips SHA256SUMS',
      'when an asset name has a space. Patch in crates/update/verify.rs.',
      '</mooring>', '',
      '⏺ Applying the reviewer\'s patch, then re-running cargo test',
      '✻ Waiting for the release notes… (1m 02s · ↓ 6.4k tokens)']),
    (0, 'Claude', 'Speed up sidebar redraw on large workspaces', 'review',
     ['> Speed up sidebar redraw on large workspaces', '', '⏺ Profiled frame time with 400 sessions: 31 ms → 9 ms',
      '⏺ Update(crates/app/src/sidebar/rows.rs)', '  ⎿ Cache per-second derived values in frame_facts',
      '⏺ Bash(python3 scripts/perf-gate.py)', '  ⎿ sidebar-scroll  p50 8.7 ms  (gate 12 ms)  ✓',
      '⏺ Bash(cargo test -p app)', '  ⎿ test result: ok. 214 passed; 0 failed', '', '⏺ Done. Ready for your review.']),
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
    (2, 'Gemini', 'Compress the hero images', 'running',
     ['Gemini CLI', '', ' ✦ Found 23 images over 200 KB', ' ✦ Converting to WebP at q=82', '', ' ⠧ 14/23 done']),
]
LIVE = [0, 1, 2, 3, 4, 16]
CLI = {'Codex': 'codex', 'Claude': 'claude', 'Grok': 'grok', 'Cursor': 'cursor-agent', 'Gemini': 'gemini'}


def codex_records(cid, cwd, title, state, stamp):
    r = [(stamp - 420, 'session_meta', {'id': cid, 'cwd': cwd, 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'}),
         (stamp - 420, 'event_msg', {'type': 'user_message', 'message': title}),
         (stamp - 420, 'event_msg', {'type': 'task_started', 'turn_id': cid + '-turn'}),
         (stamp - 300, 'event_msg', {'type': 'token_count', 'info': {'last_token_usage': {'input_tokens': 118000}, 'model_context_window': 258000},
                                     'rate_limits': {'plan_type': 'pro', 'primary': {'window_minutes': 300, 'used_percent': 38, 'resets_at': stamp + 7200},
                                                     'secondary': {'window_minutes': 10080, 'used_percent': 61, 'resets_at': stamp + 3 * 86400}}})]
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


EDITS = {  # the review pane's turn: (file, line, old, new)
    2: [('crates/app/src/sidebar/rows.rs', 41, '        let age = session_updated(s).elapsed();', '        let age = facts.age(&s.id);'),
        ('crates/app/src/sidebar/rows.rs', 88, '        for s in sessions.iter() { rows.push(row(s, cx)); }', '        rows.extend(sessions.iter().map(|s| row(s, &facts, cx)));'),
        ('crates/app/src/frame_facts.rs', 12, '    pub titles: HashMap<String, String>,', '    pub titles: HashMap<String, SharedString>,\n    pub ages: HashMap<String, Duration>,'),
        ('crates/app/src/sidebar/view.rs', 203, '        let rows = rows(&self.sessions, cx);', '        let facts = self.frame_facts(cx);\n        let rows = rows(&self.sessions, &facts, cx);'),
        ('scripts/perf-gate.json', 7, '  "sidebar-scroll": 16,', '  "sidebar-scroll": 12,')],
}


def edit_records(cid, cwd, edits, stamp):
    out = []
    for i, (rel, line, old, new) in enumerate(edits):
        path = pathlib.Path(cwd) / rel; path.parent.mkdir(parents=True, exist_ok=True)
        body = path.read_text().splitlines() if path.exists() else [f'// {rel} line {k}' for k in range(1, 260)]
        new_lines = new.split('\n'); body[line - 1:line] = new_lines; path.write_text('\n'.join(body) + '\n')
        patch = [{'oldStart': line, 'oldLines': 1, 'newStart': line, 'newLines': len(new_lines), 'lines': ['-' + old] + ['+' + x for x in new_lines]}]
        out.append({'type': 'assistant', 'sessionId': cid, 'uuid': f'{cid}-c{i}', 'cwd': cwd, 'timestamp': iso(stamp - 200 + i),
                    'message': {'role': 'assistant', 'content': [{'type': 'tool_use', 'id': f't{i}', 'name': 'Edit', 'input': {'file_path': str(path), 'old_string': old, 'new_string': new}}]}})
        out.append({'type': 'user', 'sessionId': cid, 'uuid': f'{cid}-r{i}', 'cwd': cwd, 'timestamp': iso(stamp - 200 + i),
                    'toolUseResult': {'filePath': str(path), 'originalFile': None, 'structuredPatch': patch},
                    'message': {'role': 'user', 'content': [{'type': 'tool_result', 'tool_use_id': f't{i}', 'content': 'ok'}]}})
    return out


def claude_records(cid, cwd, title, state, stamp, edits=()):
    r = [{'type': 'user', 'sessionId': cid, 'uuid': cid + '-u0', 'cwd': cwd, 'entrypoint': 'cli', 'userType': 'external',
          'timestamp': iso(stamp - 420), 'message': {'role': 'user', 'content': title}}]
    r += edit_records(cid, cwd, edits, stamp)
    if state in ('review', 'done', 'aborted'):
        r.append({'type': 'assistant', 'sessionId': cid, 'uuid': cid + '-a0', 'parentUuid': cid + '-u0', 'cwd': cwd, 'timestamp': iso(stamp),
                  'message': {'role': 'assistant', 'content': [{'type': 'text', 'text': 'Done. Ready for your review.'}], 'stop_reason': 'end_turn'}})
    return ''.join(json.dumps(x) + '\n' for x in r)


if sys.argv[1] == 'prepare':
    STATE.parent.mkdir(parents=True, exist_ok=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix='promo-', dir=STATE.parent)); home = pathlib.Path(tempfile.mkdtemp(prefix='promo-home-', dir='/tmp'))
    codex_logs = home / '.codex/sessions'; codex_logs.mkdir(parents=True)
    now = int(time.time()); logs = {}
    for n, (space, agent, title, state, _) in enumerate(SESSIONS):
        project = home / 'projects' / SPACES[space]; project.mkdir(parents=True, exist_ok=True)
        stamp = now - 90 * (n + 1) - (5 * 86400 if state == 'done' and n > 10 else 0)
        cid = str(uuid.UUID(int=random.Random(n * 7919).getrandbits(128), version=4))
        if agent == 'Gemini':
            chats = home / '.gemini/tmp' / SPACES[space] / 'chats'; chats.mkdir(parents=True, exist_ok=True)
            (chats.parent / '.project_root').write_text(str(project))
            path = chats / f'session-{cid}.jsonl'
            path.write_text(''.join(json.dumps(x) + '\n' for x in [
                {'sessionId': cid, 'kind': 'main', 'projectHash': SPACES[space], 'startTime': iso(stamp - 300)},
                {'type': 'user', 'id': cid + '-u', 'timestamp': iso(stamp - 300), 'content': title},
                {'type': 'gemini', 'id': cid + '-g', 'timestamp': iso(stamp), 'content': '',
                 'toolCalls': [{'name': 'run_shell_command', 'status': 'executing', 'timestamp': iso(stamp)}]}]))
            os.utime(path, (stamp, stamp)); logs[n] = (cid, path); continue
        if agent == 'Codex':
            path = codex_logs / f'rollout-{cid}.jsonl'; path.write_text(codex_records(cid, str(project), title, state, stamp))
        else:
            folder = home / '.claude/projects' / ('-' + SPACES[space]); folder.mkdir(parents=True, exist_ok=True)
            path = folder / f'{cid}.jsonl'; path.write_text(claude_records(cid, str(project), title, state, stamp, EDITS.get(n, ())))
        os.utime(path, (stamp, stamp)); logs[n] = (cid, path)
    for project in (home / 'projects').iterdir():
        subprocess.run(['git', '-C', str(project), 'init', '-q', '-b', 'main'], check=True)
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
    config = home / '.config/fibo'; config.mkdir(parents=True, exist_ok=True)
    (config / 'first-run-done').write_text('')
    (config / 'terminal-text.json').write_text('{"cursor_blink": false}\n')
    if os.environ.get('PROMO_THEME'):  # <mode>:<theme id>, e.g. light:paper
        mode, theme = os.environ['PROMO_THEME'].split(':', 1)
        (config / 'appearance').write_text(mode); (config / f'theme-{mode}').write_text(theme)
    far_pid = None
    if os.environ.get('PROMO_FAR'):
        far, farhome = root / 'far', home / 'studio'; far.mkdir(mode=0o700); (farhome / '.codex/sessions').mkdir(parents=True)
        (root / 'bin').mkdir(exist_ok=True)
        (root / 'bin/ssh').write_text('#!/bin/sh\nwhile [ $# -gt 0 ]; do case "$1" in -o) shift 2 ;; -*) shift ;; *) shift; break ;; esac; done\nexec /bin/sh -c "$*"\n'); (root / 'bin/ssh').chmod(0o755)
        (root / 'far-fibo').write_text(f'#!/bin/sh\nexec {BIN}/mooring "$@" --runtime {far}\n'); (root / 'far-fibo').chmod(0o755)
        (config / 'machines.json').write_text(json.dumps([{'host': 'me@studio', 'path': str(root / 'far-fibo')}]))
        fenv = dict(env, HOME=str(farhome), CODEX_HOME=str(farhome / '.codex'), CLAUDE_CONFIG_DIR=str(farhome / '.claude'))
        far_pid = subprocess.Popen([str(BIN / 'mooring'), 'serve', '--runtime', str(far)], env=fenv, stdout=subprocess.DEVNULL, stderr=open(root / 'far.log', 'w'), start_new_session=True).pid
    host = subprocess.Popen([str(BIN / 'mooring'), 'serve', '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=open(root / 'host.log', 'w'), start_new_session=True)
    deadline = time.monotonic() + 30
    while not (root / 'runtime/workbench.sock').exists():
        assert host.poll() is None and time.monotonic() < deadline; time.sleep(.05)
    for n, name in enumerate(SPACES): edit(root, {'AddSpace': {'id': f's{n}', 'name': name, 'cwd': str(home / 'projects' / name)}})
    edit(root, {'AddTask': {'id': 'task0', 'space': 's0', 'title': 'Ship v0.1 to the website', 'description': 'Release build, notarized DMG, Homebrew cask, and the new site at fibo.page.',
                            'acceptance': [{'id': f'c{i}', 'description': d, 'evidence': []} for i, d in enumerate(['DMG opens on a clean Mac', 'brew install --cask fibo works', 'fibo.page shows the download button'])], 'agent': 'Codex'}})
    for name in CLI.values(): (root / name).symlink_to('/bin/sh')
    for n in LIVE:
        space, agent, title, state, lines = SESSIONS[n]
        cid, path = logs[n]
        # The fake agent holds its log open, as a real one does, and prints its screen.
        script = ('exec 3<' + shlex.quote(str(path)) + '; ' if path else '') + 'printf "\\033[?25l"; printf "%s\\n" ' + ' '.join(shlex.quote(s) for s in lines) + '; printf "\\033[%d;90H" ' + str(max(len(lines), 1)) + '; while IFS= read -r line; do :; done'
        r = rpc(root, {'Launch': {'id': f'p{n}', 'space': f's{space}', 'argv': [str(root / CLI[agent]), '-c', script], 'cwd': str(home / 'projects' / SPACES[space]), 'cols': 90, 'rows': 30}}); assert 'Error' not in r, r
    def leaf(p): return {'Leaf': p}
    def split(a, b, axis, ratio=5000): return {'Split': {'axis': axis, 'ratio': ratio, 'first': a, 'second': b}}
    # Space s0 shows three panes; the other two live agents sit in their own spaces' tabs.
    edit(root, {'CreateTab': {'tab': {'id': 'feedback', 'space': 's0', 'title': '', 'default_task': None, 'closed': False,
                                     'layout': split(leaf('p0'), split(leaf('p1'), leaf('p2'), 'Down'), 'Right', 5200)}}})
    edit(root, {'CreateTab': {'tab': {'id': 'api', 'space': 's1', 'title': '', 'default_task': None, 'closed': False, 'layout': leaf('p3')}}})
    edit(root, {'CreateTab': {'tab': {'id': 'web', 'space': 's2', 'title': '', 'default_task': None, 'closed': False, 'layout': split(leaf('p4'), leaf('p16'), 'Right')}}})
    while True:
        w = rpc(root, 'Workspace')['Workspace']
        if len(w['agent_sessions']) >= len(SESSIONS) and all(t.get('pid') for t in w['terminals']): break
        assert time.monotonic() < deadline + 30, ('index timeout', len(w['agent_sessions'])); time.sleep(.2)
    time.sleep(1)
    for n in LIVE:
        space, agent, title, state, _ = SESSIONS[n]
        cid, path = logs[n]
        if cid is None: continue
        t = next(t for t in w['terminals'] if t['id'] == f'p{n}')
        r = rpc(root, {'Hook': {'runtime': agent, 'pid': t['pid'], 'cli_session_id': cid, 'transcript': str(path), 'ended': False, 'event': None}}); assert 'Error' not in r, r
    time.sleep(1)
    for s in rpc(root, 'Workspace')['Workspace']['agent_sessions']:
        n = next((n for n, (cid, _) in logs.items() if cid == s['cli_session_id']), None)
        if n is None: continue
        if SESSIONS[n][3] == 'done': edit(root, {'SetOutcome': {'id': s['id'], 'outcome': 'Completed', 'seen': None}})
    if far_pid:
        far = root / 'far'; farhome = home / 'studio'
        while not (far / 'workbench.sock').exists(): time.sleep(.05)
        fp = farhome / 'projects/ml'; fp.mkdir(parents=True); far_logs = []
        for k, (title, state_, lines) in enumerate([
                ('Train the ranking model on the new data', 'running', ['› Train the ranking model on the new data', '', '• epoch 7/20  loss 0.412  val 0.438', '• GPU 0: 94%  · 71 °C', '', '  ⠼ training…']),
                ('Benchmark the int8 build against fp16', 'review', ['› Benchmark the int8 build against fp16', '', '• int8: 1.9× faster, recall −0.3%', '', '─ Done. Ready for your review.'])]):
            cid = str(uuid.UUID(int=random.Random(9000 + k).getrandbits(128), version=4)); stamp = now - 200 * (k + 1)
            log = farhome / '.codex/sessions' / f'rollout-{cid}.jsonl'; log.write_text(codex_records(cid, str(fp), title, state_, stamp)); os.utime(log, (stamp, stamp)); far_logs.append((cid, log))
            far_root = root / 'farctl'; far_root.mkdir(exist_ok=True)
            if not (far_root / 'runtime').exists(): (far_root / 'runtime').symlink_to(far)
            if k == 0: edit(far_root, {'AddSpace': {'id': 'm0', 'name': 'ml', 'cwd': str(fp)}})
            script = 'exec 3<' + shlex.quote(str(log)) + '; printf "\\033[?25l"; printf "%s\\n" ' + ' '.join(shlex.quote(x) for x in lines) + '; while IFS= read -r line; do :; done'
            r = rpc(far_root, {'Launch': {'id': f'far{k}', 'space': 'm0', 'argv': [str(root / 'codex'), '-c', script], 'cwd': str(fp), 'cols': 90, 'rows': 30}}); assert 'Error' not in r, r
        edit(far_root, {'CreateTab': {'tab': {'id': 'ml', 'space': 'm0', 'title': '', 'default_task': None, 'closed': False, 'layout': split(leaf('far0'), leaf('far1'), 'Right')}}})
        time.sleep(3)
        fw = rpc(far_root, 'Workspace')['Workspace']
        for k, (cid, log) in enumerate(far_logs):
            pid = next(t for t in fw['terminals'] if t['id'] == f'far{k}')['pid']
            rpc(far_root, {'Hook': {'runtime': 'Codex', 'pid': pid, 'cli_session_id': cid, 'transcript': str(log), 'ended': False, 'event': None}})
    state = {'root': str(root), 'home': str(home), 'host': host.pid, 'far': far_pid}; STATE.write_text(json.dumps(state)); print(state)
else:
    s = json.loads(STATE.read_text()); root = pathlib.Path(s['root']); home = pathlib.Path(s['home'])
    if sys.argv[1] == 'capture':
        directory = pathlib.Path(sys.argv[2]).resolve(); directory.mkdir(parents=True, exist_ok=True)
        env = {'PATH': f"{root / 'bin'}:/usr/bin:/bin:/usr/sbin:/sbin", 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8',
               'GPUI_PAUSE_IDLE_DISPLAY_LINK': '1', 'MOORING_PAGE_EVIDENCE': '1', 'MOORING_PAGE_LIST': os.environ.get('MOORING_PAGE_LIST', 'Main,Overview')}
        if os.environ.get('MOORING_LANG'): env['MOORING_LANG'] = os.environ['MOORING_LANG']
        # PROMO_HARNESS=CHANGES runs that harness (MOORING_CHANGES_EVIDENCE) over this workbench instead of the pages.
        if os.environ.get('PROMO_HARNESS'):
            del env['MOORING_PAGE_EVIDENCE']; env[f"MOORING_{os.environ['PROMO_HARNESS']}_EVIDENCE"] = '1'
        result = subprocess.run([str(BIN / 'mooring-app'), '--runtime', str(root / 'runtime'), '--visual-evidence', str(directory)], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180)
        (directory / 'native-render.log').write_bytes(result.stdout); assert result.returncode == 0, result.stdout.decode()[-4000:]
        print('native captures', sorted(p.name for p in directory.glob('*-native.png')))
    elif sys.argv[1] == 'workspace':
        w = rpc(root, 'Workspace')['Workspace']
        print(json.dumps([{k: a.get(k) for k in ('cli_session_id', 'runtime', 'title', 'state', 'terminal')} for a in w['agent_sessions']], indent=1)[:4000])
    elif sys.argv[1] == 'crash':
        os.kill(s['host'], 9); time.sleep(1)
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
        for p in (root / 'runtime').glob('workbench.sock'): p.unlink()
        host = subprocess.Popen([str(BIN / 'mooring'), 'serve', '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=open(root / 'host2.log', 'w'), start_new_session=True)
        deadline = time.monotonic() + 30
        while not (root / 'runtime/workbench.sock').exists():
            assert host.poll() is None and time.monotonic() < deadline; time.sleep(.05)
        time.sleep(2); s['host'] = host.pid; STATE.write_text(json.dumps(s)); print('host restarted', host.pid)
    elif sys.argv[1] == 'cli':
        env = {'PATH': '/usr/bin:/bin', 'HOME': str(home), 'MOORING_RUNTIME': str(root / 'runtime'), 'LANG': 'en_US.UTF-8', 'MOORING_LANG': 'en'}
        subprocess.run([str(BIN / 'fibo'), *sys.argv[2:]], env=env)
    elif sys.argv[1] == 'stop':
        try:
            for t in rpc(root, 'Workspace')['Workspace']['terminals']: rpc(root, {'Stop': {'id': t['id']}})
            os.kill(s['host'], 15)
            if s.get('far'): os.kill(s['far'], 15)
        except (ConnectionRefusedError, FileNotFoundError, ProcessLookupError):
            pass
