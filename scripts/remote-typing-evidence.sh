#!/bin/bash
# Typing into another machine's terminal from its peek, end to end without a second Mac:
# a second host here, reached through a fake ssh. Needs a debug build with visual tests:
#   cargo build -p mooring-host -p mooring-app --features mooring-app/visual-test
# Prints PASS and saves the peek to $R/shots/remote-typing.png.
set -u
REPO=$(cd "$(dirname "$0")/.." && pwd)
R=${MOORING_EVIDENCE_TMP:-/private/tmp/mooring-rt}
rm -rf $R; mkdir -p $R/home/.config/mooring $R/claude $R/shots $R/bin; mkdir -m 700 $R/far $R/rt
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
print(list(rpc({'Edit': {'edit': {'AddSpace': {'id': 's', 'name': 'far', 'cwd': '/tmp'}}}})))
print(list(rpc({'Launch': {'id': 'far', 'space': 's', 'argv': ['/bin/sh', '-c', "stty -echo; exec cat >> '%s/typed.txt'" % R], 'cwd': '/tmp', 'cols': 100, 'rows': 30}})))
print(list(rpc({'Edit': {'edit': {'CreateTab': {'tab': {'id': 't', 'space': 's', 'title': '', 'default_task': None, 'layout': {'Leaf': 'far'}}}}}})))
PY
PATH=$R/bin:$PATH HOME=$R/home CLAUDE_CONFIG_DIR=$R/claude MOORING_REMOTE_TYPING_EVIDENCE=1 MOORING_FAR_TYPED=$R/typed.txt \
  perl -e 'alarm 90; exec @ARGV' $REPO/target/debug/mooring-app --runtime $R/rt --visual-evidence $R/shots > $R/app.log 2>&1
echo "app exit $?"
grep -E "PASS|panicked|assert" -A2 $R/app.log | head
echo "typed: $(cat $R/typed.txt 2>/dev/null | tr '\n' '|')"
kill $FAR 2>/dev/null; sleep 1; pkill -f "$R/far" 2>/dev/null
ls $R/shots
