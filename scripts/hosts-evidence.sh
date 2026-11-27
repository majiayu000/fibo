#!/bin/bash
# Another machine in this window (one window, many hosts, step 2), end to end without a
# second Mac: this window runs its own isolated host; a second host with its own HOME plays
# machine `far`, reached through a fake ssh whose `-L` is a link to its socket. Picking far's
# space draws far's own workbench in the window, ⌘T opens in that space on far, 「← 本机」
# comes back with this machine's tabs as they were, and far keeps its tab meanwhile. Before
# far is picked the window holds none of its panes and opens nothing there; the tab ⌘T
# opened runs far's own shell (checked inside, the tab is closed again at the end).
# Needs a debug build with visual tests:
#   cargo build -p fibo-host -p fibo-app --features fibo-app/visual-test
# Prints PASS and what the other machine holds afterwards; saves shots to $R/shots.
set -euo pipefail
REPO=$(cd "$(dirname "$0")/.." && pwd)
R=${MOORING_EVIDENCE_TMP:-/private/tmp/mooring-hs}
pkill -f "$R/far" 2>/dev/null || true
FAR=
cleanup() {
  pkill -9 -f "fake-forward-$R" 2>/dev/null || true
  if [ -n "$FAR" ]; then kill "$FAR" 2>/dev/null || true; fi
  pkill -f "$R/far" 2>/dev/null || true
  pkill -f "serve --runtime $R/rt" 2>/dev/null || true
}
trap cleanup EXIT
rm -rf "$R"; mkdir -p "$R/home/.config/fibo" "$R/bin" "$R/shots" "$R/farhome/code"
mkdir -m 700 "$R/far" "$R/rt"
cat > "$R/bin/ssh" <<SH
#!/bin/bash
# Fake ssh: drops options and the host. \`-N -L here:there\` links here to there and stays
# up like a forward; anything else runs on "the other machine": HOME farhome, bash its shell.
forward=""
while [ \$# -gt 0 ]; do
  case "\$1" in
    -L) forward="\$2"; shift 2 ;;
    -o) shift 2 ;;
    -*) shift ;;
    *) shift; break ;;
  esac
done
if [ -n "\$forward" ]; then
  ln -sf "\${forward#*:}" "\${forward%%:*}"
  exec -a "fake-forward-$R" sleep 86400
fi
HOME=$R/farhome SHELL=/bin/bash exec /bin/sh -c "\$*"
SH
chmod +x "$R/bin/ssh"
printf '#!/bin/sh\nMOORING_RUNTIME=%s/far exec %s/target/debug/fibo "$@"\n' "$R" "$REPO" > "$R/far-fibo"
chmod +x "$R/far-fibo"
printf '[{"host":"far","path":"%s/far-fibo"}]' "$R" > "$R/home/.config/fibo/machines.json"
env -i PATH=/usr/bin:/bin HOME="$R/farhome" LANG=en_US.UTF-8 \
  "$REPO/target/debug/fibo" serve --runtime "$R/far" > "$R/far.log" 2>&1 &
FAR=$!
rpc() {
  python3 -I - "$R/far/workbench.sock" "$1" <<'PY'
import json, socket, struct, sys, time
def read(s, n):
    b = b''
    while len(b) < n:
        c = s.recv(n - len(b)); assert c; b += c
    return b
def send(s, o):
    b = b'\1' + json.dumps(o).encode(); s.sendall(struct.pack('>I', len(b)) + b)
def recv(s): return json.loads(read(s, struct.unpack('>I', read(s, 4))[0])[1:])
for _ in range(100):
    try:
        with socket.socket(socket.AF_UNIX) as s:
            s.settimeout(10); s.connect(sys.argv[1]); send(s, {'Hello': {'major': 3}}); recv(s)
            send(s, json.loads(sys.argv[2])); print(json.dumps(recv(s)))
        break
    except (FileNotFoundError, ConnectionRefusedError):
        time.sleep(0.1)
PY
}
# A host that has not answered by then (a binary just built waits on macOS checking it) would
# leave far without its space, and the window would fail further on for a reason not its own.
added=$(rpc "{\"Edit\":{\"edit\":{\"AddSpace\":{\"id\":\"far\",\"name\":\"far\",\"cwd\":\"$R/farhome/code\"}}}}")
if [ -z "$added" ]; then
  echo "far's host did not answer within 10 s; run again (its log: $R/far.log)"
  kill $FAR 2>/dev/null
  exit 1
fi
PATH="$R/bin:$PATH" HOME="$R/home" MOORING_HOSTS_EVIDENCE=1 MOORING_HOSTS_FARHOME="$R/farhome" \
  perl -e 'alarm 180; exec @ARGV' "$REPO/target/debug/fibo-app" --runtime "$R/rt" --visual-evidence "$R/shots" > "$R/app.log" 2>&1
APP_STATUS=$?
echo "app exit $APP_STATUS"
grep '^hosts PASS:' "$R/app.log"
rpc '"Workspace"' | python3 -I -c '
import json, sys
w = json.load(sys.stdin)["Workspace"]
print("far tabs left:", [t["id"] for t in w["tabs"]])
print("far terminals:", sorted({t["state"] for t in w["terminals"]}))
live = [t for t in w["terminals"] if t["state"] == "Running"]
print("far running shells left:", [t["argv"][0] for t in live])
# Exited terminal records are history, not a process the window forgot to close.
assert not w["tabs"] and not live, "the window left a tab or running terminal on far"'
if pgrep -f "fake-forward-$R" >/dev/null; then
  echo "a fake forward outlived the window"; exit 1
fi
echo "forward ended with the window"
ls "$R/shots"

exit "$APP_STATUS"
