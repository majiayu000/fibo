#!/usr/bin/env python3
"""What a theme costs on the real screen, which the gate cannot see: its windows are off
screen, so the system compositor's work on a picture or a see-through window is not in them.

  python3 scripts/theme-live.py BIN_DIR [SECONDS] [ROUNDS] [PICTURES_DIR]

Opens the real window on the 2000-session workspace of perf-fixture.py, four terminals
printing, each theme in turn for some rounds, and reports the app's CPU and the whole
machine's GPU use with the median of the rounds. GPU use is system-wide and swings with what
else is on screen (one round read 43% for the plain theme and 26% for a picture), so only
differences that hold across the rounds mean anything. Needs an unlocked screen, so this is
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


def gpu_percent():
    out = subprocess.run(['ioreg', '-r', '-d', '1', '-c', 'IOAccelerator'], capture_output=True, text=True).stdout
    m = re.search(r'"Device Utilization %"=(\d+)', out)
    return int(m.group(1)) if m else None


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
            samples, done = [], threading.Event()

            def watch():
                time.sleep(10)  # as the fixture's own measurement: past the start
                if pictures and n == 0:
                    pictures.mkdir(parents=True, exist_ok=True)
                    subprocess.run(['screencapture', '-x', str(pictures / f'{name}.png')], capture_output=True)
                while not done.is_set():
                    value = gpu_percent()
                    if value is not None:
                        samples.append(value)
                    time.sleep(1)

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
            gpu = statistics.mean(samples) if samples else float('nan')
            seen[name][0].append(cpu)
            seen[name][1].append(gpu)
            print(f'round {n + 1} {name}: CPU {cpu:.1f}% of a core; GPU {gpu:.0f}%', flush=True)
    for name, (cpu, gpu) in seen.items():
        print(f'{name}: CPU median {statistics.median(cpu):.1f}% of a core; GPU median {statistics.median(gpu):.0f}%', flush=True)


main()
