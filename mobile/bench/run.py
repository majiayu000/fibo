#!/usr/bin/env python3
"""Runs the Markdown benchmark on a phone by adb and prints medians. Only touches the
benchmark package, never the real app. Usage: run.py parse | render | scroll | blank ENGINE..."""
import json, os, re, statistics, subprocess, sys, time

ADB = [os.path.expanduser('~/Library/Android/sdk/platform-tools/adb'), '-s', os.environ.get('SERIAL', '3B164B000A000000')]
PKG = os.environ.get('PKG', 'dev.mooring.mobile.bench')


def sh(*args, timeout=60):
    return subprocess.run(ADB + ['shell', *args], capture_output=True, text=True, timeout=timeout).stdout


def swipe(y0, y1):
    """Input goes to whatever is in front, so refuse unless that is the benchmark: the real app must never get it."""
    focus = next((l for l in sh('dumpsys', 'window').splitlines() if 'mCurrentFocus' in l), '')
    if f'{PKG}/' not in focus:
        raise SystemExit(f'not in front, stopping: {focus.strip()}')
    sh('input', 'swipe', '540', str(y0), '540', str(y1), '110')


def start(query):
    sh('am', 'force-stop', PKG)
    time.sleep(0.8)
    subprocess.run(ADB + ['logcat', '-c'], timeout=20)
    sh('am', 'start', '-n', f'{PKG}/dev.mooring.mobile.MainActivity', '-a', 'android.intent.action.VIEW', '-d', f"'moorbench://run?{query}'")


def wait_log(timeout=45):
    end = time.time() + timeout
    while time.time() < end:
        out = subprocess.run(ADB + ['logcat', '-d', '-s', 'ReactNativeJS:*'], capture_output=True, text=True, timeout=20).stdout
        m = re.findall(r'MD_BENCH (\{.*\})', out)
        if m:
            return json.loads(m[-1])
        time.sleep(0.5)
    fatal = subprocess.run(ADB + ['logcat', '-d', '*:E'], capture_output=True, text=True, timeout=20).stdout
    raise SystemExit('no result\n' + '\n'.join(l for l in fatal.splitlines() if PKG in l or 'ReactNative' in l)[-1500:])


def parse_scene():
    runs = []
    for _ in range(5):
        start('scene=parse')
        runs.append(wait_log())
        print(json.dumps(runs[-1], ensure_ascii=False))
    keys = [k for k, v in runs[0].items() if isinstance(v, (int, float)) and not isinstance(v, bool)]
    print('MEDIAN', {k: round(statistics.median(r[k] for r in runs), 2) for k in keys})


def render(engines, rounds=7):
    for count, share in ((50, 0.31), (50, 1), (200, 0.31)):
        for engine in engines:
            rs = []
            for i in range(rounds):
                start(f'engine={engine}&scene=render&count={count}&share={share}&seed={11 + i * 13}')
                rs.append(wait_log())
            f = [r['firstContentMs'] for r in rs]
            s = [r['settledMs'] for r in rs]
            print(f'{engine} n={count} share={share}: first p50={statistics.median(f):.0f} max={max(f):.0f}  settled p50={statistics.median(s):.0f} max={max(s):.0f}', flush=True)


def gfx():
    out = sh('dumpsys', 'gfxinfo', PKG)
    get = lambda pat: (re.search(pat, out) or [None, '-'])[1]
    return {
        'frames': get(r'Total frames rendered: (\d+)'),
        'janky': get(r'Janky frames: (\d+ \([\d.]+%\))'),
        'p50': get(r'50th percentile: (\d+)ms'),
        'p90': get(r'90th percentile: (\d+)ms'),
        'p95': get(r'95th percentile: (\d+)ms'),
        'p99': get(r'99th percentile: (\d+)ms'),
        'missedVsync': get(r'Number Missed Vsync: (\d+)'),
        'slowUi': get(r'Number Slow UI thread: (\d+)'),
        'slowDraw': get(r'Number Slow issue draw commands: (\d+)'),
    }


def scroll(engines, rounds=3):
    for engine in engines:
        for i in range(rounds):
            start(f'engine={engine}&scene=render&count=200&share=0.31&seed={21 + i}')
            wait_log()
            time.sleep(1)
            sh('dumpsys', 'gfxinfo', PKG, 'reset')
            # Upward flings through history, the way someone reads back.
            for _ in range(16):
                swipe(700, 1900)
                time.sleep(0.45)
            for _ in range(5):
                swipe(1900, 700)
                time.sleep(0.45)
            g = gfx()
            mem = re.search(r'TOTAL PSS:\s+(\d+)', sh('dumpsys', 'meminfo', PKG))
            print(engine, i, g, 'pssMB', int(mem[1]) // 1024 if mem else '-', flush=True)


def blank(engines, rounds=3):
    """What a reader sees: grabs the screen again and again during flings and finds the longest
    empty stretch in each grab, where rows were scrolled to before they were drawn. Sampled, as
    this phone blocks screenrecord for the shell. Needs numpy."""
    import numpy as np
    import threading
    for engine in engines:
        all_runs = []
        for i in range(rounds):
            start(f'engine={engine}&scene=render&count=200&share=0.31&seed={21 + i}')
            wait_log()
            time.sleep(1)
            runs, done = [], threading.Event()

            def grab():
                while not done.is_set():
                    raw = subprocess.run(ADB + ['exec-out', 'screencap'], capture_output=True, timeout=20).stdout
                    w, h = int.from_bytes(raw[:4], 'little'), int.from_bytes(raw[4:8], 'little')
                    px = np.frombuffer(raw[-w * h * 4:], np.uint8).reshape(h, w, 4)[:, ::4, :3]
                    px = px[int(h * .06):int(h * .97)]
                    flat = px.std(axis=(1, 2)) < 2  # a row with nothing on it
                    best = cur = 0
                    for v in flat:
                        cur = cur + 1 if v else 0
                        best = max(best, cur)
                    runs.append(best / len(flat))

            t = threading.Thread(target=grab)
            t.start()
            for _ in range(22):
                swipe(700, 1900)
                time.sleep(0.45)
            done.set()
            t.join()
            all_runs += runs
        r = np.array(all_runs)
        print(engine, f'grabs={len(r)} blank>35%: {(r > .35).mean() * 100:.1f}%  blank>60%: {(r > .6).mean() * 100:.1f}%  p50={np.median(r) * 100:.0f}% p95={np.percentile(r, 95) * 100:.0f}% max={r.max() * 100:.0f}%', flush=True)


if __name__ == '__main__':
    what = sys.argv[1]
    engines = sys.argv[2:] or ['A', 'B']
    if what == 'parse':
        parse_scene()
    elif what == 'render':
        render(engines)
    elif what == 'blank':
        blank(engines)
    else:
        scroll(engines)
