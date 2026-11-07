#!/bin/bash
# The left column's layouts for other machines, end to end without a second Mac: a second
# host here, reached through a fake ssh, and a third machine that never connects. Each
# host has one Codex session that finished a turn.
# Needs a debug build with visual tests:
#   cargo build -p mooring-host -p mooring-app --features mooring-app/visual-test
# Prints PASS and saves every step to $R/shots.
set -u
REPO=$(cd "$(dirname "$0")/.." && pwd)
R=${MOORING_EVIDENCE_TMP:-/private/tmp/mooring-rs}
rm -rf $R; mkdir -p $R/home/.config/mooring $R/home/.codex/sessions $R/claude $R/shots $R/bin $R/farhome/.codex/sessions $R/farhome/.claude/projects
# A blinking cursor would read as something changing right of the column.
printf '{"cursor_blink": false}\n' > $R/home/.config/mooring/terminal-text.json
# MOORING_EVIDENCE_THEME=<mode>:<theme id> looks at everything in that theme, e.g. light:paper.
if [ -n "${MOORING_EVIDENCE_THEME:-}" ]; then
  printf '%s' "${MOORING_EVIDENCE_THEME%%:*}" > $R/home/.config/mooring/appearance
  printf '%s' "${MOORING_EVIDENCE_THEME#*:}" > $R/home/.config/mooring/theme-${MOORING_EVIDENCE_THEME%%:*}
fi
mkdir -m 700 $R/far $R/rt
# A folder to make a space of over there. The fake ssh keeps this HOME, so that is where the
# other machine's side of the link looks for ~ and for names.
mkdir -p $R/home/brandnew
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
printf '[{"host":"me@far","path":"%s/far-mooring"},{"host":"gone","path":"/nonexistent/mooring"}]' "$R" > $R/home/.config/mooring/machines.json
ln -s /bin/sh $R/codex
env -i PATH=/usr/bin:/bin HOME=$R/farhome CODEX_HOME=$R/farhome/.codex CLAUDE_CONFIG_DIR=$R/farhome/.claude LANG=en_US.UTF-8 \
  $REPO/target/debug/mooring serve --runtime $R/far > $R/far.log 2>&1 &
FAR=$!
env -i PATH=/usr/bin:/bin HOME=$R/home CODEX_HOME=$R/home/.codex CLAUDE_CONFIG_DIR=$R/claude LANG=en_US.UTF-8 \
  $REPO/target/debug/mooring serve --runtime $R/rt > $R/here.log 2>&1 &
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
def setup(sock, home, name, term, say):
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
    rpc({'Edit': {'edit': {'AddSpace': {'id': 'empty', 'name': '空的', 'cwd': '/tmp'}}}})
    script = 'exec 3<' + log + '; printf "%s\\n" "› ' + say + '" "已整理好，请核对。"; while IFS= read -r l; do :; done'
    rpc({'Launch': {'id': term, 'space': 's', 'argv': [R + '/codex', '-c', script], 'cwd': '/tmp', 'cols': 100, 'rows': 30}})
    rpc({'Edit': {'edit': {'CreateTab': {'tab': {'id': 't', 'space': 's', 'title': '', 'default_task': None, 'layout': {'Leaf': term}}}}}})
    deadline = time.time() + 30
    while not rpc('Workspace')['Workspace']['agent_sessions']:
        assert time.time() < deadline, 'never indexed'; time.sleep(0.2)
    pid = next(t for t in rpc('Workspace')['Workspace']['terminals'] if t['id'] == term)['pid']
    rpc({'Hook': {'runtime': 'Codex', 'pid': pid, 'cli_session_id': name, 'transcript': log, 'ended': False, 'event': None}})
setup(R + '/far/workbench.sock', R + '/farhome', 'far-codex', 'far', '整理远程的验收清单')
setup(R + '/rt/workbench.sock', R + '/home', 'here-codex', 'here', '整理本机的验收清单')
PY
PATH=$R/bin:$PATH HOME=$R/home CLAUDE_CONFIG_DIR=$R/claude CODEX_HOME=$R/home/.codex MOORING_REMOTE_SIDEBAR_EVIDENCE=1 MOORING_FAR_PID=$FAR \
  perl -e 'alarm 300; exec @ARGV' $REPO/target/debug/mooring-app --runtime $R/rt --visual-evidence $R/shots > $R/app.log 2>&1
echo "app exit $?"
grep -E "PASS|panicked|assert|left:|right:" -A1 $R/app.log | head -12
# A new launch reads 先不管 back from machines.json.
PATH=$R/bin:$PATH HOME=$R/home CLAUDE_CONFIG_DIR=$R/claude CODEX_HOME=$R/home/.codex MOORING_REMOTE_SIDEBAR_EVIDENCE=1 MOORING_QUIET_RESTART=1 MOORING_FAR_PID=$FAR \
  perl -e 'alarm 60; exec @ARGV' $REPO/target/debug/mooring-app --runtime $R/rt --visual-evidence $R/shots > $R/relaunch.log 2>&1
echo "relaunch exit $?"
grep -E "PASS|panicked|assert|left:|right:" -A1 $R/relaunch.log | head -6
kill $FAR $HERE 2>/dev/null; sleep 1; pkill -f "$R/" 2>/dev/null
ls $R/shots
