#!/bin/bash
# Another machine's sessions on the board, end to end without a second Mac: a second
# host here, reached through a fake ssh, with one Codex session that finished a turn.
# Needs a debug build with visual tests:
#   cargo build -p mooring-host -p mooring-app --features mooring-app/visual-test
# Prints PASS, what the other machine recorded, and saves the board to $R/shots.
set -u
REPO=$(cd "$(dirname "$0")/.." && pwd)
R=${MOORING_EVIDENCE_TMP:-/private/tmp/mooring-rb}
rm -rf $R; mkdir -p $R/home/.config/mooring $R/claude $R/shots $R/bin $R/farhome/.codex/sessions $R/farhome/.claude/projects
mkdir -m 700 $R/far $R/rt
cat > $R/bin/ssh <<'SH'
#!/bin/sh
# Fake ssh: drops options and the host, runs the command here.
while [ $# -gt 0 ]; do
  case "$1" in
    -o) shift 2 ;;
    -*) shift ;;
    *) shift; break ;;
  esac
done
exec /bin/sh -c "$*"
SH
chmod +x $R/bin/ssh
printf '#!/bin/sh\nexec %s/target/debug/mooring "$@" --runtime %s/far\n' "$REPO" "$R" > $R/far-mooring; chmod +x $R/far-mooring
printf '[{"host":"far","path":"%s/far-mooring"}]' "$R" > $R/home/.config/mooring/machines.json
ln -s /bin/sh $R/codex
env -i PATH=/usr/bin:/bin HOME=$R/farhome CODEX_HOME=$R/farhome/.codex CLAUDE_CONFIG_DIR=$R/farhome/.claude LANG=en_US.UTF-8 \
  $REPO/target/debug/mooring serve --runtime $R/far > $R/far.log 2>&1 &
FAR=$!
python3 -I - "$R" <<'PY'
import json, socket, struct, sys, time
R = sys.argv[1]; SOCK = R + '/far/workbench.sock'
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
        s.settimeout(10); s.connect(SOCK); send(s, {'Hello': {'major': 3}}); recv(s); send(s, o); return recv(s)
for _ in range(100):
    try: rpc('Workspace'); break
    except Exception: time.sleep(0.1)
now = int(time.time()); log = R + '/farhome/.codex/sessions/far-codex.jsonl'
records = [(now - 600, 'session_meta', {'id': 'far-codex', 'cwd': '/tmp', 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'}),
           (now - 600, 'event_msg', {'type': 'user_message', 'message': '整理远程看板的验收清单'}),
           (now - 600, 'event_msg', {'type': 'task_started', 'turn_id': 'far-turn'}),
           (now - 60, 'event_msg', {'type': 'task_complete', 'last_agent_message': '清单已整理好，请核对。'})]
open(log, 'w').write(''.join(json.dumps({'timestamp': at, 'type': t, 'payload': p}, ensure_ascii=False) + '\n' for at, t, p in records))
print(list(rpc({'Edit': {'edit': {'AddSpace': {'id': 's', 'name': 'far', 'cwd': '/tmp'}}}})))
script = 'exec 3<' + log + '; printf "%s\\n" "› 整理远程看板的验收清单" "清单已整理好，请核对。"; while IFS= read -r l; do :; done'
print(list(rpc({'Launch': {'id': 'far', 'space': 's', 'argv': [R + '/codex', '-c', script], 'cwd': '/tmp', 'cols': 100, 'rows': 30}})))
print(list(rpc({'Edit': {'edit': {'CreateTab': {'tab': {'id': 't', 'space': 's', 'title': '', 'default_task': None, 'layout': {'Leaf': 'far'}}}}}})))
deadline = time.time() + 30
while not rpc('Workspace')['Workspace']['agent_sessions']:
    assert time.time() < deadline, 'never indexed'; time.sleep(0.2)
pid = next(t for t in rpc('Workspace')['Workspace']['terminals'] if t['id'] == 'far')['pid']
print(list(rpc({'Hook': {'runtime': 'Codex', 'pid': pid, 'cli_session_id': 'far-codex', 'transcript': log, 'ended': False, 'event': None}})))
PY
PATH=$R/bin:$PATH HOME=$R/home CLAUDE_CONFIG_DIR=$R/claude CODEX_HOME=$R/home/.codex MOORING_REMOTE_BOARD_EVIDENCE=1 \
  perl -e 'alarm 120; exec @ARGV' $REPO/target/debug/mooring-app --runtime $R/rt --visual-evidence $R/shots > $R/app.log 2>&1
echo "app exit $?"
grep -E "PASS|panicked|assert" -A2 $R/app.log | head
python3 -I - "$R" <<'PY'
import json, socket, struct, sys
R = sys.argv[1]
def read(s, n):
    b = b''
    while len(b) < n:
        c = s.recv(n - len(b)); assert c; b += c
    return b
def send(s, o):
    b = b'\1' + json.dumps(o).encode(); s.sendall(struct.pack('>I', len(b)) + b)
def recv(s): return json.loads(read(s, struct.unpack('>I', read(s, 4))[0])[1:])
with socket.socket(socket.AF_UNIX) as s:
    s.settimeout(10); s.connect(R + '/far/workbench.sock'); send(s, {'Hello': {'major': 3}}); recv(s)
    send(s, 'Workspace'); w = recv(s)['Workspace']
print('far outcome:', [x['outcome'] for x in w['agent_sessions']])
print('far terminals:', [(t['id'], t['state']) for t in w['terminals']])
print('far tabs:', [t['id'] for t in w['tabs']])
PY
kill $FAR 2>/dev/null; sleep 1; pkill -f "$R/far" 2>/dev/null
ls $R/shots
