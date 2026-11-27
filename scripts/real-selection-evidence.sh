#!/bin/bash
# One sidebar row lit, against the real machines in ~/.config/fibo/machines.json: this
# window runs its own isolated host here (one Codex session that finished a turn), and
# reaches machine 0 for real over ssh, the way the App does. Never touches this machine's
# own host or its terminals; machine 0's host is attached, which a window never replaces.
# Needs a debug build with visual tests:
#   cargo build -p fibo-host -p fibo-app --features fibo-app/visual-test
# Prints PASS and saves a shot of every step to $R/shots.
set -u
REPO=$(cd "$(dirname "$0")/.." && pwd)
R=${MOORING_EVIDENCE_TMP:-/private/tmp/mooring-real-selection}
rm -rf "$R"; mkdir -p "$R/home/.config/fibo" "$R/home/.codex/sessions" "$R/claude" "$R/shots"
mkdir -m 700 "$R/rt"
cp "$HOME/.config/fibo/machines.json" "$R/home/.config/fibo/"
for f in appearance theme-dark theme-light sidebar; do
  [ -e "$HOME/.config/fibo/$f" ] && cp -R "$HOME/.config/fibo/$f" "$R/home/.config/fibo/"
done
printf '{"cursor_blink": false}\n' > "$R/home/.config/fibo/terminal-text.json"
ln -s /bin/sh "$R/codex"
env -i PATH=/usr/bin:/bin HOME="$R/home" CODEX_HOME="$R/home/.codex" CLAUDE_CONFIG_DIR="$R/claude" LANG=en_US.UTF-8 \
  "$REPO/target/debug/fibo" serve --runtime "$R/rt" > "$R/here.log" 2>&1 &
HERE=$!
python3 -I - "$R" <<'PY'
import json, socket, struct, sys, time
R = sys.argv[1]
def read(s, n):
    b = b''
    while len(b) < n:
        c = s.recv(n - len(b)); assert c; b += c
    return b
def send(s, o):
    b = b'\1' + json.dumps(o).encode(); s.sendall(struct.pack('>I', len(b)) + b)
def recv(s): return json.loads(read(s, struct.unpack('>I', read(s, 4))[0])[1:])
sock, home, name, term, say = R + '/rt/workbench.sock', R + '/home', 'here-codex', 'here', '整理本机的验收清单'
def rpc(o):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(10); s.connect(sock); send(s, {'Hello': {'major': 3}}); recv(s); send(s, o); return recv(s)
for _ in range(100):
    try: rpc('Workspace'); break
    except Exception: time.sleep(0.1)
now = int(time.time()); log = home + '/.codex/sessions/' + name + '.jsonl'
records = [(now - 600, 'session_meta', {'id': name, 'cwd': '/tmp', 'source': 'cli', 'originator': 'codex-tui', 'thread_source': 'user'}),
           (now - 600, 'event_msg', {'type': 'user_message', 'message': say}),
           (now - 600, 'event_msg', {'type': 'task_started', 'turn_id': name + '-turn'}),
           (now - 60, 'event_msg', {'type': 'task_complete', 'last_agent_message': '已整理好，请核对。'})]
open(log, 'w').write(''.join(json.dumps({'timestamp': at, 'type': t, 'payload': p}, ensure_ascii=False) + '\n' for at, t, p in records))
rpc({'Edit': {'edit': {'AddSpace': {'id': 's', 'name': name, 'cwd': '/tmp'}}}})
script = 'exec 3<' + log + '; printf "%s\\n" "› ' + say + '" "已整理好，请核对。"; while IFS= read -r l; do :; done'
rpc({'Launch': {'id': term, 'space': 's', 'argv': [R + '/codex', '-c', script], 'cwd': '/tmp', 'cols': 100, 'rows': 30}})
rpc({'Edit': {'edit': {'CreateTab': {'tab': {'id': 't', 'space': 's', 'title': '', 'default_task': None, 'layout': {'Leaf': term}}}}}})
deadline = time.time() + 30
while not rpc('Workspace')['Workspace']['agent_sessions']:
    assert time.time() < deadline, 'never indexed'; time.sleep(0.2)
pid = next(t for t in rpc('Workspace')['Workspace']['terminals'] if t['id'] == term)['pid']
rpc({'Hook': {'runtime': 'Codex', 'pid': pid, 'cli_session_id': name, 'transcript': log, 'ended': False, 'event': None}})
PY
HOME="$R/home" CLAUDE_CONFIG_DIR="$R/claude" CODEX_HOME="$R/home/.codex" MOORING_REAL_SELECTION_EVIDENCE=1 \
  perl -e 'alarm 600; exec @ARGV' "$REPO/target/debug/fibo-app" --runtime "$R/rt" --visual-evidence "$R/shots" > "$R/app.log" 2>&1
echo "app exit $?"
grep -E "walking|lit \[|PASS|panicked" -A1 "$R/app.log"
kill $HERE 2>/dev/null
ls "$R/shots"
# The forwards the window made to machine 0 go with it.
pkill -f "ssh.*$R/" 2>/dev/null || true
