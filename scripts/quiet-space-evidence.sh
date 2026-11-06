#!/bin/bash
# 先不管这个空间, end to end: a real host holding two Codex sessions, one finished for you and
# one still working, in a space of their own and the window driven with real mouse events, then a second launch that
# reads the state back (docs/quiet-space/spec.md).
# Needs a debug build with visual tests:
#   cargo build -p mooring-host -p mooring-app --features mooring-app/visual-test
# Prints PASS twice and saves every step to $R/shots.
set -u
REPO=$(cd "$(dirname "$0")/.." && pwd)
R=${MOORING_EVIDENCE_TMP:-/private/tmp/mooring-qs}
rm -rf "$R"; mkdir -p "$R/home/.config/mooring" "$R/home/.codex/sessions" "$R/claude" "$R/shots"
printf '{"cursor_blink": false}\n' > "$R/home/.config/mooring/terminal-text.json"
mkdir -m 700 "$R/rt"
ln -s /bin/sh "$R/codex"
env -i PATH=/usr/bin:/bin HOME="$R/home" CODEX_HOME="$R/home/.codex" CLAUDE_CONFIG_DIR="$R/claude" LANG=en_US.UTF-8 \
  "$REPO/target/debug/mooring" serve --runtime "$R/rt" > "$R/here.log" 2>&1 &
HERE=$!
python3 -I - "$R" <<'PY'
import json, socket, struct, sys, time
R = sys.argv[1]
sock = R + '/rt/workbench.sock'; home = R + '/home'
def read(s, n):
    b = b''
    while len(b) < n:
        c = s.recv(n - len(b)); assert c; b += c
    return b
def send(s, o):
    b = b'\1' + json.dumps(o).encode(); s.sendall(struct.pack('>I', len(b)) + b)
def recv(s): return json.loads(read(s, struct.unpack('>I', read(s, 4))[0])[1:])
def rpc(o):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(10); s.connect(sock); send(s, {'Hello': {'major': 3}}); recv(s); send(s, o); return recv(s)
for _ in range(100):
    try: rpc('Workspace'); break
    except Exception: time.sleep(0.1)
rpc({'Edit': {'edit': {'AddSpace': {'id': 's', 'name': '工作', 'cwd': '/tmp'}}}})
rpc({'Edit': {'edit': {'AddSpace': {'id': 'keep', 'name': '个人', 'cwd': '/usr'}}}})
rpc({'Edit': {'edit': {'AddSpace': {'id': 'a', 'name': '官网', 'cwd': '/usr/bin'}}}})
rpc({'Edit': {'edit': {'AddSpace': {'id': 'b', 'name': '周报', 'cwd': '/usr/lib'}}}})
def session(name, term, tab, say, finished):
    now = int(time.time()); log = home + '/.codex/sessions/' + name + '.jsonl'
    records = [(now - 600, 'session_meta', {'id': name, 'cwd': '/tmp', 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'}),
               (now - 600, 'event_msg', {'type': 'user_message', 'message': say}),
               (now - 600, 'event_msg', {'type': 'task_started', 'turn_id': name + '-turn'})]
    if finished:
        records.append((now - 60, 'event_msg', {'type': 'task_complete', 'last_agent_message': '已整理好，请核对。'}))
    open(log, 'w').write(''.join(json.dumps({'timestamp': at, 'type': t, 'payload': p}, ensure_ascii=False) + '\n' for at, t, p in records))
    script = 'exec 3<' + log + '; printf "%s\\n" "› ' + say + '"; while IFS= read -r l; do :; done'
    rpc({'Launch': {'id': term, 'space': 's', 'argv': [R + '/codex', '-c', script], 'cwd': '/tmp', 'cols': 100, 'rows': 30}})
    rpc({'Edit': {'edit': {'CreateTab': {'tab': {'id': tab, 'space': 's', 'title': '', 'default_task': None, 'layout': {'Leaf': term}}}}}})
    deadline = time.time() + 30
    while not any(s['cli_session_id'] == name for s in rpc('Workspace')['Workspace']['agent_sessions']):
        assert time.time() < deadline, 'never indexed'; time.sleep(0.2)
    pid = next(t for t in rpc('Workspace')['Workspace']['terminals'] if t['id'] == term)['pid']
    rpc({'Hook': {'runtime': 'Codex', 'pid': pid, 'cli_session_id': name, 'transcript': log, 'ended': False, 'event': None}})
session('done-codex', 'done', 't1', '整理工作空间的验收清单', True)
session('busy-codex', 'busy', 't2', '继续跑工作空间的迁移', False)
PY
run() {
  HOME="$R/home" CLAUDE_CONFIG_DIR="$R/claude" CODEX_HOME="$R/home/.codex" MOORING_QUIET_SPACE_EVIDENCE=1 "$@" \
    perl -e 'alarm 120; exec @ARGV' "$REPO/target/debug/mooring-app" --runtime "$R/rt" --visual-evidence "$R/shots"
}
run env > "$R/app.log" 2>&1
echo "app exit $?"
grep -E "PASS|panicked|assert|left:|right:|looking at" -A1 "$R/app.log" | head -20
run env MOORING_QUIET_SPACE_RESTART=1 > "$R/relaunch.log" 2>&1
echo "relaunch exit $?"
grep -E "PASS|panicked|assert|left:|right:" -A1 "$R/relaunch.log" | head -8
kill "$HERE" 2>/dev/null; sleep 1; pkill -f "$R/" 2>/dev/null
ls "$R/shots"
