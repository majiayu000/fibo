#!/usr/bin/env python3
"""What a theme costs on the real screen, which the gate cannot see: its windows are off
screen, so the system compositor's work on a picture or a see-through window is not in them.

  python3 scripts/theme-live.py BIN_DIR [SECONDS] [ROUNDS] [PICTURES_DIR]

Opens the real window on the 2000-session workspace of perf-fixture.py, four terminals
printing, each theme in turn for some rounds, and reports the median of the rounds: the app's
CPU and WindowServer's GPU time as a share of the time measured. WindowServer composites
everything on screen, so a round contributes to either median only if the exact fixture
process is in front at every sample. Run with an unlocked screen and no other session
opening windows. The driver counter used here measures WindowServer's GPU time.
This probe runs by hand, outside CI.
"""
import os, pathlib, re, runpy, shutil, signal, statistics, subprocess, sys, tempfile, time

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIXTURE = [sys.executable, str(ROOT / 'scripts/perf-fixture.py')]
THEMES = [
    ('默认', None, None, 'dark'),
    ('夜港', 'theme-harbor', 'plugin:theme-harbor/harbor', 'dark'),
    ('旧纸', 'theme-paper', 'plugin:theme-paper/paper', 'light'),
    ('暗玻璃', 'theme-glass', 'plugin:theme-glass/glass-dark', 'dark'),
]


def gpu_seconds():
    """Each process's accumulated GPU time, which the driver keeps per client."""
    text = subprocess.run(['ioreg', '-l', '-w0'], capture_output=True, text=True, errors='replace').stdout
    owner, seen = None, {}
    for line in text.splitlines():
        m = re.search(r'"IOUserClientCreator" = "pid (\d+), ', line)
        if m:
            owner = int(m.group(1))
            continue
        m = re.search(r'"AppUsage" = \((.*)\)', line)
        if m and owner is not None:
            used = sum(int(n) for n in re.findall(r'"accumulatedGPUTime"=(\d+)', m.group(1)))
            seen[owner] = seen.get(owner, 0) + used / 1e9
    return seen


def pid_of(pattern):
    found = subprocess.run(['pgrep', '-f', pattern], capture_output=True, text=True).stdout.split()
    return int(found[0]) if found else None


def front_pid():
    who = subprocess.run(['lsappinfo', 'front'], capture_output=True, text=True).stdout.strip()
    if not who:
        return None
    info = subprocess.run(['lsappinfo', 'info', '-only', 'pid', who], capture_output=True, text=True).stdout
    match = re.search(r'"pid"=(\d+)', info)
    return int(match.group(1)) if match else None


def main():
    bindir = pathlib.Path(sys.argv[1]).resolve()
    secs = int(sys.argv[2]) if len(sys.argv) > 2 else 20
    rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    pictures = pathlib.Path(sys.argv[4]) if len(sys.argv) > 4 else None
    live = runpy.run_path(FIXTURE[1])['live']
    seen = {name: ([], []) for name, *_ in THEMES}
    for n in range(rounds):
        for name, plugin, theme, side in THEMES:
            env = {**os.environ, 'NORUN': '1'}
            cache = pathlib.Path.home() / 'Library/Caches/MooringPerf'
            cache.mkdir(parents=True, exist_ok=True)
            root = pathlib.Path(tempfile.mkdtemp(prefix='p-', dir=cache))
            try:
                subprocess.run(FIXTURE + ['prepare', str(root), str(bindir), '2000', '60', '4'], env=env,
                                          check=True, capture_output=True, text=True)
                home = root / 'home'
                config = home / '.config/fibo'
                config.mkdir(parents=True, exist_ok=True)
                if plugin:
                    shutil.copytree(ROOT / 'plugins' / plugin, config / 'plugins' / plugin)
                    (config / 'appearance').write_text(side)
                    (config / f'theme-{side}').write_text(theme)
                fronts = []
                server_gpu = None

                def watch(app_pid, seconds):
                    nonlocal server_gpu
                    if pictures and n == 0 and front_pid() == app_pid:
                        pictures.mkdir(parents=True, exist_ok=True)
                        subprocess.run(['screencapture', '-x', str(pictures / f'{name}.png')], capture_output=True)
                    server = pid_of('/WindowServer')
                    start, began = gpu_seconds(), time.monotonic()
                    deadline = began + seconds
                    while True:
                        fronts.append(front_pid() == app_pid)
                        remaining = deadline - time.monotonic()
                        if remaining <= 0:
                            break
                        time.sleep(min(1, remaining))
                    end, elapsed = gpu_seconds(), time.monotonic() - began
                    if server in start and server in end and end[server] >= start[server]:
                        server_gpu = (end[server] - start[server]) / elapsed * 100

                cpu = live(root, bindir, secs, watch)
            finally:
                subprocess.run(FIXTURE + ['stop', str(root)], check=True)
            front_count = sum(fronts)
            label = f'round {n + 1} {name}'
            if not fronts or front_count != len(fronts):
                print(f'{label}: rejected, fixture in front {front_count}/{len(fronts)} samples', flush=True)
                continue
            seen[name][0].append(cpu)
            if server_gpu is not None:
                seen[name][1].append(server_gpu)
            gpu = 'unavailable' if server_gpu is None else f'{server_gpu:.1f}%'
            print(f'{label}: CPU {cpu:.1f}%; WindowServer GPU {gpu}; '
                  f'in front {front_count}/{len(fronts)} samples; load {os.getloadavg()[0]:.0f}', flush=True)
    for name, (cpu, server) in seen.items():
        cpu_summary = f'{statistics.median(cpu):.1f}% of a core ({len(cpu)} rounds)' if cpu else 'no foreground rounds'
        gpu_summary = f'{statistics.median(server):.1f}% ({len(server)} rounds)' if server else 'unavailable'
        print(f'{name}: CPU {cpu_summary}; WindowServer GPU {gpu_summary}', flush=True)
    if any(not cpu or not gpu for cpu, gpu in seen.values()):
        raise SystemExit('Some themes have no valid CPU or GPU samples')


if __name__ == '__main__':
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f'interrupted by signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    main()
