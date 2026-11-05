#!/usr/bin/env python3
"""What a theme costs on the real screen, which the gate cannot see: its windows are off
screen, so the system compositor's work on a picture or a see-through window is not in them.

  python3 scripts/theme-live.py BIN_DIR [SECONDS] [ROUNDS] [PICTURES_DIR]

Opens the real window on the 2000-session workspace of perf-fixture.py, four terminals
printing, each theme in turn for some rounds, and reports the median of the rounds: the app's
CPU and WindowServer's GPU time as a share of the time measured. WindowServer composites
everything on screen, so only rounds with our window in front count for it: run this with no
other session opening windows. The app's own GPU time reads 0 from this source and the whole
machine's utilization read 43% for the plain theme and 26% for a picture in one round, which
is why neither is used. Needs an unlocked screen, so this is
a thing to run by hand, not in CI.
"""
import os, pathlib, re, shutil, statistics, subprocess, sys, threading, time

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


def front_app():
    # No permission needed, unlike asking System Events.
    who = subprocess.run(['lsappinfo', 'front'], capture_output=True, text=True).stdout.strip()
    out = subprocess.run(['lsappinfo', 'info', '-only', 'name', who], capture_output=True, text=True).stdout
    m = re.search(r'"LSDisplayName"="([^"]*)"', out)
    return m.group(1) if m else '?'


def main():
    bindir = pathlib.Path(sys.argv[1]).resolve()
    secs = sys.argv[2] if len(sys.argv) > 2 else '20'
    rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    pictures = pathlib.Path(sys.argv[4]) if len(sys.argv) > 4 else None
    seen = {name: ([], []) for name, *_ in THEMES}
    for n in range(rounds):
        for name, plugin, theme, side in THEMES:
            env = {**os.environ, 'NORUN': '1'}
            prepared = subprocess.run(FIXTURE + ['prepare', str(bindir), '2000', '60', '4'], env=env,
                                      check=True, capture_output=True, text=True).stdout
            home = pathlib.Path(re.search(r"'home': '([^']+)'", prepared).group(1))
            config = home / '.config/mooring'
            config.mkdir(parents=True, exist_ok=True)
            if plugin:
                shutil.copytree(ROOT / 'plugins' / plugin, config / 'plugins' / plugin)
                (config / 'appearance').write_text(side)
                (config / f'theme-{side}').write_text(theme)
            fronts, used, done = [], {}, threading.Event()

            def watch():
                time.sleep(10)  # as the fixture's own measurement: past the start
                if pictures and n == 0:
                    pictures.mkdir(parents=True, exist_ok=True)
                    subprocess.run(['screencapture', '-x', str(pictures / f'{name}.png')], capture_output=True)
                server = pid_of('/WindowServer')
                start, began = gpu_seconds(), time.monotonic()
                while not done.is_set():
                    fronts.append(front_app())
                    done.wait(1)
                end, spent = gpu_seconds(), time.monotonic() - began
                used['server'] = (end.get(server, 0) - start.get(server, 0)) / spent * 100

            thread = threading.Thread(target=watch)
            thread.start()
            try:
                live = subprocess.run(FIXTURE + ['live', str(bindir), secs], env=env, check=True,
                                      capture_output=True, text=True).stdout.strip()
            finally:
                done.set()
                thread.join()
                subprocess.run(FIXTURE + ['stop'], capture_output=True)
            cpu = float(re.search(r'([\d.]+)% of a core', live).group(1))
            shown = fronts.count('mooring-app') / max(len(fronts), 1)
            server_gpu = used.get('server', float('nan'))
            seen[name][0].append(cpu)
            if shown >= 0.9:
                seen[name][1].append(server_gpu)
            print(f'round {n + 1} {name}: CPU {cpu:.1f}%; WindowServer GPU {server_gpu:.1f}%; '
                  f'in front {shown:.0%}; load {os.getloadavg()[0]:.0f}', flush=True)
    for name, (cpu, server) in seen.items():
        tail = f'{statistics.median(server):.1f}% ({len(server)} rounds in front)' if server else 'no round in front'
        print(f'{name}: CPU {statistics.median(cpu):.1f}% of a core; WindowServer GPU {tail}', flush=True)


main()
