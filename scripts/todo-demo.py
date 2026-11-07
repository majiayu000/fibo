#!/usr/bin/env python3
"""A throwaway window for trying todos and sessions by hand: its own host, data and HOME, so
your Mooring and its data are not touched, with simulated agents that call no model.

  python3 scripts/todo-demo.py start target/release
  python3 scripts/todo-demo.py stop

Needs `cargo build --release -p mooring-host -p mooring-app`. `claude`, `codex` and `grok`
in its terminals are a script that writes a Codex log, so every card shows Codex's mark.
"""
import json, os, pathlib, shutil, socket, struct, subprocess, sys, tempfile, time
STATE = pathlib.Path.home() / 'Library/Caches/MooringTodoDemo/state.json'
def read(s, n):
    b = b''
    while len(b) < n:
        c = s.recv(n - len(b)); assert c, 'closed'; b += c
    return b
def rpc(root, o):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(30); s.connect(str(pathlib.Path(root) / 'runtime/workbench.sock'))
        for m in ({'Hello': {'major': 3}}, o):
            b = b'\1' + json.dumps(m).encode(); s.sendall(struct.pack('>I', len(b)) + b)
            r = json.loads(read(s, struct.unpack('>I', read(s, 4))[0])[1:])
        return r
def edit(root, o):
    r = rpc(root, {'Edit': {'edit': o}}); assert 'Error' not in r, r
AGENT = r'''#!/usr/bin/python3
import json, os, pathlib, socket, struct, sys, time, uuid
ROOT, HOME = %r, %r
cid = 'sim-' + uuid.uuid4().hex[:8]
log = pathlib.Path(HOME) / '.codex/sessions' / (cid + '.jsonl')
def put(t, p):
    with log.open('a') as f:
        f.write(json.dumps({'timestamp': int(time.time()), 'type': t, 'payload': p}, ensure_ascii=False) + '\n')
def hook():
    with socket.socket(socket.AF_UNIX) as s:
        s.connect(ROOT + '/runtime/workbench.sock')
        for m in ({'Hello': {'major': 3}}, {'Hook': {'runtime': 'Codex', 'pid': os.getppid(), 'cli_session_id': cid, 'transcript': str(log), 'ended': False, 'event': None, 'terminal': os.environ.get('MOORING_TERMINAL')}}):
            b = b'\1' + json.dumps(m).encode(); s.sendall(struct.pack('>I', len(b)) + b)
            n = struct.unpack('>I', s.recv(4))[0]; s.recv(n)
put('session_meta', {'id': cid, 'cwd': os.getcwd(), 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'})
hook()
def todo(*args):
    import subprocess
    return subprocess.run(['mooring', 'todo', *args, '--runtime', ROOT + '/runtime'], capture_output=True, text=True).stdout
print('模拟 Agent：不调用模型。这条待办：')
print(todo('show'))
print('按回车开始。')
prompt = input('› ').strip() or '随便看看'
put('event_msg', {'type': 'user_message', 'message': prompt})
put('event_msg', {'type': 'task_started', 'turn_id': cid + '-t'})
hook()
print('收到：' + prompt)
print('在跑 20 秒，之后会变成「要你处理」。Ctrl-C 退出。')
time.sleep(8)
todo('note', '读完了代码，准备先改接口。')
time.sleep(12)
todo('note', '接口和测试都写好了，等你看导出的格式。')
put('event_msg', {'type': 'task_complete', 'last_agent_message': '做好了，请核对。'})
print('跑完了，回鸟瞰看卡片。')
time.sleep(100000)
'''
SESSIONS = [  # title, space, minutes ago, state
    ('给列表加键盘导航', 0, 4, 'run'), ('跑一遍完整回归', 1, 1, 'run'),
    ('推送前确认 PR 标题', 0, 3, 'ask'), ('确认迁移脚本权限', 2, 40, 'ask'),
    ('补快捷键文档', 0, 6, 'done'), ('分析测速算法差异', 3, 60, 'done'), ('对照远端检查代码库', 0, 70, 'done'),
    ('整理失败项', 1, 120, 'done'), ('检查图标缺失问题', 1, 180, 'done'), ('发布前核对更新日志', 0, 300, 'done'),
    ('迁移旧会话表', 3, 240, 'stop'), ('空状态文案', 1, 400, 'stop'), ('试跑新模型', 2, 500, 'stop'),
    ('清理临时分支', 3, 3000, 'stop'), ('读日志找超时', 0, 4000, 'stop'), ('看依赖更新', 2, 6000, 'stop'),
]
SPACES = ['mooring', '工作空间', 'her', 'codex 专用']
TODOS = [('t0', '键盘操作补齐', 0), ('t1', '给 CLI 加离线模式', 2), ('t2', '周五前交报销单', 1)]
cmd = sys.argv[1]
if cmd == 'start':
    bindir = pathlib.Path(sys.argv[2]).resolve()
    STATE.parent.mkdir(parents=True, exist_ok=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix='d-', dir=STATE.parent)); home = pathlib.Path(tempfile.mkdtemp(prefix='mtd-', dir='/tmp'))
    logs = home / '.codex/sessions'; logs.mkdir(parents=True); (home / '.claude/projects').mkdir(parents=True)
    tools = root / 'bin'; tools.mkdir()
    for name in ('claude', 'codex', 'grok'):
        (tools / name).write_text(AGENT % (str(root), str(home))); (tools / name).chmod(0o755)
    (tools / 'mooring').symlink_to(bindir / 'mooring')
    now = int(time.time())
    for n, (title, sp, ago, state) in enumerate(SESSIONS):
        project = root / 'projects' / SPACES[sp]; project.mkdir(parents=True, exist_ok=True)
        at = now - 60 * ago; cid = f'demo-{n}'
        rec = [(at - 600, 'session_meta', {'id': cid, 'cwd': str(project), 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'}),
               (at - 600, 'event_msg', {'type': 'user_message', 'message': title}),
               (at - 600, 'event_msg', {'type': 'task_started', 'turn_id': cid + '-t'})]
        if state == 'ask': rec.append((at, 'event_msg', {'type': 'request_user_input', 'message': '要不要继续？', 'questions': [{'question': '要不要继续？', 'options': [{'label': '继续'}, {'label': '停'}]}]}))
        elif state == 'done': rec.append((at, 'event_msg', {'type': 'task_complete', 'last_agent_message': '做好了，请核对。'}))
        elif state == 'stop': rec.append((at, 'event_msg', {'type': 'turn_aborted'}))
        (logs / (cid + '.jsonl')).write_text(''.join(json.dumps({'timestamp': t, 'type': k, 'payload': p}, ensure_ascii=False) + '\n' for t, k, p in rec))
        os.utime(logs / (cid + '.jsonl'), (at, at))
    env = {'PATH': f'{tools}:/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'), 'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8', 'SHELL': '/bin/zsh'}
    (home / '.zshrc').write_text(f'export PATH={tools}:$PATH\n')
    host = subprocess.Popen([str(bindir / 'mooring'), 'serve', '--runtime', str(root / 'runtime')], env=env, stdout=subprocess.DEVNULL, stderr=open(root / 'host.log', 'w'), start_new_session=True)
    deadline = time.monotonic() + 120
    while not (root / 'runtime/workbench.sock').exists():
        assert host.poll() is None and time.monotonic() < deadline; time.sleep(.05)
    for n, name in enumerate(SPACES): edit(root, {'AddSpace': {'id': f's{n}', 'name': name, 'cwd': str(root / 'projects' / name)}})
    (root / 'codex').symlink_to('/bin/sh')
    live = list(range(len(SESSIONS)))
    for n in live:
        sp = SESSIONS[n][1]
        r = rpc(root, {'Launch': {'id': f'p{n}', 'space': f's{sp}', 'argv': [str(root / 'codex'), '-c', 'while :; do sleep 3600; done'], 'cwd': str(root / 'projects' / SPACES[sp]), 'cols': 100, 'rows': 35}}); assert 'Error' not in r, r
        edit(root, {'CreateTab': {'tab': {'id': f'tab{n}', 'space': f's{sp}', 'title': SESSIONS[n][0], 'default_task': None, 'closed': False, 'layout': {'Leaf': f'p{n}'}}}})
    while True:
        w = rpc(root, 'Workspace')['Workspace']
        if len(w['agent_sessions']) >= len(SESSIONS): break
        assert time.monotonic() < deadline, 'index timeout'; time.sleep(.2)
    for n in live:
        t = next(t for t in w['terminals'] if t['id'] == f'p{n}')
        r = rpc(root, {'Hook': {'runtime': 'Codex', 'pid': t['pid'], 'cli_session_id': f'demo-{n}', 'transcript': str(logs / f'demo-{n}.jsonl'), 'ended': False, 'event': None}}); assert 'Error' not in r, r
    for tid, title, sp in TODOS: edit(root, {'AddTask': {'id': tid, 'space': f's{sp}', 'title': title, 'description': '', 'acceptance': [], 'agent': 'Codex'}})
    w = rpc(root, 'Workspace')['Workspace']
    by = {s['cli_session_id']: s['id'] for s in w['agent_sessions']}
    for n in (0, 4): edit(root, {'AssignTask': {'id': by[f'demo-{n}'], 'task': 't0'}})
    for text in ('键盘导航的方案定了：先做列表，再做看板。', '列表的 j/k 做完了，还差看板。'): edit(root, {'AddTaskNote': {'task': 't0', 'text': text, 'session': None}})
    if os.environ.get('NOAPP'):
        STATE.write_text(json.dumps({'root': str(root), 'home': str(home), 'host': host.pid, 'app': 0})); print('host only', root); sys.exit()
    evidence = sys.argv[3:] and ['--visual-evidence', str(pathlib.Path(sys.argv[3]).resolve())]
    if evidence: env['MOORING_REVIEW_QUEUE_EVIDENCE' if os.environ.get('MOORING_REVIEW_QUEUE_EVIDENCE') else 'MOORING_TODO_DEMO_EVIDENCE'] = '1'
    if os.environ.get('MOORING_TODO_FUZZ_SEED'): env['MOORING_TODO_FUZZ_SEED'] = os.environ['MOORING_TODO_FUZZ_SEED']
    app = subprocess.Popen([str(bindir / 'mooring-app'), '--runtime', str(root / 'runtime'), *evidence], env=env, stdout=open(root / 'app.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
    STATE.write_text(json.dumps({'root': str(root), 'home': str(home), 'host': host.pid, 'app': app.pid}))
    print('started', 'app', app.pid, 'host', host.pid, 'root', root)
elif cmd == 'stop':
    s = json.loads(STATE.read_text())
    try:
        for t in rpc(s['root'], 'Workspace')['Workspace']['terminals']: rpc(s['root'], {'Stop': {'id': t['id']}})
    except Exception as e: print(e)
    for pid in filter(None, (s['app'], s['host'])):
        try: os.kill(pid, 15)
        except ProcessLookupError: pass
    time.sleep(.5)
    shutil.rmtree(s['root'], ignore_errors=True); shutil.rmtree(s['home'], ignore_errors=True); STATE.unlink()
    print('stopped')
