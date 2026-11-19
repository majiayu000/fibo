"""Host cost of a window's refresh, without a window: take a perf-fixture.py host over with
BINDIR's build, then COUNT times write a log line and ask Brief(since, Workspace) as a
window does. Counts the host's retired instructions, so a window's pace (it merges
refreshes when the machine is busy) does not move the number.
usage: perf-refresh.py RUN_DIR BINDIR COUNT"""
import ctypes, importlib.util, json, pathlib, socket, subprocess, sys, time

spec = importlib.util.spec_from_file_location('pf', pathlib.Path(__file__).with_name('perf-fixture.py'))
pf = importlib.util.module_from_spec(spec); spec.loader.exec_module(pf)
lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
class V4(ctypes.Structure):
    _fields_ = [('uuid', ctypes.c_uint8 * 16), ('f', ctypes.c_uint64 * 40)]
def instructions(pid):
    v = V4(); assert lib.proc_pid_rusage(pid, 4, ctypes.byref(v)) == 0; return v.f[29]

root = pathlib.Path(sys.argv[1]).resolve(); bindir = pathlib.Path(sys.argv[2]).resolve(); count = int(sys.argv[3])
s = json.loads((root / 'state.json').read_text()); home = pathlib.Path(s['home'])
env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(home), 'CODEX_HOME': str(home / '.codex'),
       'CLAUDE_CONFIG_DIR': str(home / '.claude'), 'LANG': 'en_US.UTF-8'}
runtime = root / 'runtime'
def hosts():
    out = subprocess.run(['ps', '-axo', 'pid=,command='], capture_output=True, text=True).stdout.splitlines()
    return [(l.split()[0], l.split()[1]) for l in out if f'serve --runtime {runtime}' in l and l.split()[1].endswith('/fibo')]
with (root / 'host.log').open('a') as log:
    subprocess.Popen([str(bindir / 'fibo'), 'serve', '--runtime', str(runtime), '--takeover'], env=env,
                     stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
deadline = time.monotonic() + 60
while True:
    h = hosts(); exe = [e for _, e in h]
    if len(h) == 1 and exe[0].startswith(str(bindir)):
        try:
            if 'Workspace' in pf.rpc(root, 'Workspace'): break
        except Exception:
            pass
    assert time.monotonic() < deadline, (h, exe); time.sleep(0.5)
host = int(h[0][0]); print('host', host, flush=True); time.sleep(5)
w = pf.rpc(root, 'Workspace')['Workspace']
log_file = home / '.codex/sessions/perf-1.jsonl'
with socket.socket(socket.AF_UNIX) as c:
    c.settimeout(60); c.connect(str(runtime / 'workbench.sock'))
    pf.send(c, {'Hello': {'major': 3}}); pf.receive(c)
    def brief(since):
        pf.send(c, {'Brief': [since, 'Workspace']}); r = pf.receive(c)['WorkspaceDelta']
        return r['stamp'], len(r['sessions']), len(r['content'])
    since, n0, _ = brief(None)
    i0 = instructions(host); t0 = time.monotonic(); changed = 0
    for i in range(count):
        with log_file.open('a') as f:
            f.write(json.dumps({'timestamp': int(time.time()), 'type': 'event_msg',
                                'payload': {'type': 'agent_message', 'message': f'第 {i} 步'}}, ensure_ascii=False) + '\n')
        time.sleep(0.25)
        since, _, k = brief(since); changed += k
    i1 = instructions(host); dt = time.monotonic() - t0
print(f'{bindir.name} host {host}: {(i1 - i0) / count / 1e6:.1f} M instr per refresh, {(i1 - i0) / dt / 1e6:.0f} M/s; '
      f'{n0} sessions sent whole, {len(w["agent_sessions"])} in snapshot, content changed in {changed} of {count} refreshes')
