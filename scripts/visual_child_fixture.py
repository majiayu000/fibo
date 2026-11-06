"""Isolated OpenCode SQLite and Cursor child logs; never calls a model or reads user data."""
import json
import shlex
import sqlite3
import time

PARENT = 'c19a5748-897d-4836-a555-fce8a7797222'
CHILDREN = ['19817791-63b7-441c-ad06-25fe5e5293ba', '43c5c185-4a65-45bb-a8d7-fc45cb8a4907', 'f6d889ad-8931-4baf-8476-a000f368f3b3']
TASKS = ['检查事件读取', '核对索引更新', '验证侧栏状态']


def prepare(root, home, binary_dir, now):
    cwd = root / 'projects/工作空间'
    database = home / '.local/share/opencode/opencode.db'
    database.parent.mkdir(parents=True)
    db = sqlite3.connect(database)
    db.executescript('CREATE TABLE session(id TEXT PRIMARY KEY,parent_id TEXT,directory TEXT,title TEXT,time_updated INTEGER);'
                     'CREATE TABLE message(id TEXT PRIMARY KEY,session_id TEXT,time_updated INTEGER,data TEXT);'
                     'CREATE TABLE part(id TEXT PRIMARY KEY,message_id TEXT,session_id TEXT,time_updated INTEGER,data TEXT);')
    for n, cid in enumerate([PARENT] + CHILDREN):
        at = now - 200 + n * 10
        title = 'OpenCode 并行任务' if n == 0 else TASKS[n - 1]
        db.execute('INSERT INTO session VALUES(?,?,?,?,?)', (cid, None if n == 0 else PARENT, str(cwd), title, at * 1000))
        db.execute('INSERT INTO message VALUES(?,?,?,?)', ('ask-' + cid, cid, at * 1000, json.dumps({'role': 'user'})))
        db.execute('INSERT INTO part VALUES(?,?,?,?,?)', ('ask-' + cid, 'ask-' + cid, cid, at * 1000, json.dumps({'type': 'text', 'text': title})))
        if n >= 2:
            end = now - 60 + n * 10
            data = {'role': 'assistant'}
            if n == 3:
                data['error'] = {'name': 'MessageAbortedError'}
            db.execute('INSERT INTO message VALUES(?,?,?,?)', ('end-' + cid, cid, end * 1000, json.dumps(data)))
            if n == 2:
                db.execute('INSERT INTO part VALUES(?,?,?,?,?)', ('end-' + cid, 'end-' + cid, cid, end * 1000, json.dumps({'type': 'step-finish', 'reason': 'stop'})))
            db.execute('UPDATE session SET time_updated=? WHERE id=?', (end * 1000, cid))
    db.commit()
    db.close()
    # Each build uses its own declaration, against the same source database shapes.
    declaration = binary_dir.parent.parent / 'plugins/opencode/agent.json'
    config = home / '.config/mooring/agents'
    config.mkdir(parents=True, exist_ok=True)
    (config / 'opencode.json').write_bytes(declaration.read_bytes())
    chat = home / f'.cursor/chats/hash/{PARENT}'
    chat.mkdir(parents=True)
    (chat / 'meta.json').write_text(json.dumps({'cwd': str(cwd)}))
    (chat / 'prompt_history.json').write_text('[]')
    logs = home / f'.cursor/projects/fixture/agent-transcripts/{PARENT}'
    (logs / 'subagents').mkdir(parents=True)
    for n, cid in enumerate([PARENT] + CHILDREN):
        title = 'Cursor 并行任务' if n == 0 else TASKS[n - 1]
        records = [{'timestamp': now - 200 + n * 10, 'role': 'user', 'message': {'content': [{'type': 'text', 'text': title}]}}]
        if n >= 2:
            records.append({'timestamp': now - 60 + n * 10, 'type': 'turn_ended', 'status': 'success' if n == 2 else 'aborted'})
        path = logs / ('' if n == 0 else 'subagents') / (cid + '.jsonl')
        path.write_text(''.join(json.dumps(r, ensure_ascii=False) + '\n' for r in records))
    return [('poc', 'opencode', 'opencode', home / f'.local/share/mooring/sources/opencode/{PARENT}.jsonl'),
            ('pcursor', 'cursor-agent', 'Cursor', logs / (PARENT + '.jsonl'))]


def launch(root, sources, rpc):
    deadline = time.monotonic() + 30
    for pane, executable, runtime, transcript in sources:
        cli = root / executable
        cli.symlink_to('/bin/sh')
        while not transcript.exists():
            assert time.monotonic() < deadline, f'missing mirror {transcript}'
            time.sleep(.1)
        script = f'exec 3<{shlex.quote(str(transcript))}; printf "%s\\n" "隔离子 Agent 验收 · 未调用模型"; while IFS= read -r line; do :; done'
        r = rpc(root, {'Launch': {'id': pane, 'space': 's0', 'argv': [str(cli), '-c', script], 'cwd': str(root / 'projects/工作空间'), 'cols': 80, 'rows': 30}})
        assert 'Error' not in r, r
        while True:
            w = rpc(root, 'Workspace')['Workspace']
            t = next(t for t in w['terminals'] if t['id'] == pane)
            if t.get('pid'): break
            assert time.monotonic() < deadline, 'terminal timeout'
            time.sleep(.1)
        r = rpc(root, {'Hook': {'runtime': runtime, 'pid': t['pid'], 'cli_session_id': PARENT, 'transcript': str(transcript), 'ended': False, 'event': None}})
        assert 'Error' not in r, r
