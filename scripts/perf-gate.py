#!/usr/bin/env python3
"""The window's performance gate. Builds nothing: BIN_DIR holds `mooring` and a
`mooring-app` built with `--release --features mooring-app/visual-test`.

  python3 scripts/perf-gate.py BIN_DIR [OUT]

On the 2000-session workspace of scripts/perf-fixture.py, with no turn running:
- a frame that only a terminal asked for, and a full redraw, must stay within BUDGET of
  the instructions in crates/mooring-app/perf-baseline.txt. Retired instructions, not
  time: the same build's CPU time moves 2.5x with machine load, its instructions <1%.
  MOORING_PERF_UPDATE=1 writes the measured numbers there (only for a change you mean).
  A virtual machine (the CI runner) counts no instructions; there a terminal frame's CPU
  time as a share of a full redraw's, measured back to back in one process, must stay
  within SHARE_BUDGET of `pane-share` (0.18 here, 0.19 on the runner; 1.0 with the
  sidebar drawn each frame);
- terminal output must not draw the sidebar again (it drew the whole sidebar each
  frame once: 7x the instructions);
- a frame drawn without refreshing must match one drawn with a full refresh, pixel for
  pixel, after each step of redraw-check;
- with nothing happening, one catch-up must not schedule the next (a loop drew the whole
  window several times a second).
"""
import os, pathlib, re, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / 'crates/mooring-app/perf-baseline.txt'
BUDGET = 0.15
SHARE_BUDGET = 2.0
FIXTURE = [sys.executable, str(ROOT / 'scripts/perf-fixture.py')]


def probe(bindir, out, plan, catch_up):
    env = {**os.environ, 'NORUN': '1'}
    env.pop('MOORING_NO_CATCH_UP', None)
    if not catch_up:
        env['MOORING_NO_CATCH_UP'] = '1'
    subprocess.run(FIXTURE + ['prepare', str(bindir)], env=env, check=True, stdout=subprocess.DEVNULL)
    try:
        r = subprocess.run(FIXTURE + ['probe', str(bindir), str(out), plan], env=env, check=True,
                           capture_output=True, text=True)
    finally:
        subprocess.run(FIXTURE + ['stop'], capture_output=True)
    print(r.stdout.strip())
    return r.stdout


def main():
    bindir = pathlib.Path(sys.argv[1]).resolve()
    out = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else ROOT / 'target/perf-gate').resolve()
    fails = []
    text = probe(bindir, out / 'frames', 'frames-pane,frames-main,redraw-check', catch_up=False)
    measured = {k: float(v) for k, v in re.findall(r'(frames-\w+): [^\n]*?([\d.]+) M instructions', text)}
    ms = {k: float(v) for k, v in re.findall(r'(frames-\w+): ([\d.]+) ms CPU', text)}
    if set(measured) != {'frames-pane', 'frames-main'} or set(ms) != set(measured):
        fails.append(f'no frame numbers in the probe output: {measured} {ms}')
        ms = {'frames-pane': 1, 'frames-main': 1}
    share = ms['frames-pane'] / ms['frames-main']
    # MOORING_PERF_NO_COUNTER=1 takes the runner's path on real hardware, to test it.
    counted = all(v > 0 for v in measured.values()) and not os.environ.get('MOORING_PERF_NO_COUNTER')
    redraw = next((l for l in text.splitlines() if 'redraw-check:' in l), '')
    for step, verdict in re.findall(r'([\w-]+): (same|DIFF[^;]*)', redraw):
        if verdict != 'same':
            fails.append(f'redraw-check {step}: {verdict} (pictures in {out / "frames"})')
    if 'sidebar kept its draw true' not in redraw:
        fails.append('terminal output drew the sidebar again: ' + (redraw or 'no redraw-check line'))
    if not redraw:
        fails.append('no redraw-check line in the probe output')

    quiet = probe(bindir, out / 'catch-up', 'redraw-check', catch_up=True)
    # Counting catch-ups is not enough: a runner with a display draws the window by itself,
    # 15 in 5 s from a working build. What the loop does is a catch-up's own frame setting
    # the next one; a working build never does.
    m = re.search(r"set by a catch-up's own frame: (\d+)", quiet)
    if not m:
        fails.append('no chained catch-up count in the probe output')
    elif int(m.group(1)) > 0:
        fails.append(f"{m.group(1)} catch-ups set by a catch-up's own frame: a catch-up schedules the next")

    if os.environ.get('MOORING_PERF_UPDATE'):
        assert counted, 'no instruction counter here: update the baseline on real hardware'
        BASELINE.write_text(''.join(f'{k} {v:.2f}\n' for k, v in sorted({**measured, 'pane-share': share}.items())))
        print(f'perf-gate: wrote {BASELINE.relative_to(ROOT)}')
    else:
        base = {k: float(v) for k, v in (l.split() for l in BASELINE.read_text().splitlines() if l.strip())}
        s, b = share, base['pane-share']
        print(f'perf-gate: a terminal frame costs {s:.2f} of a full redraw, baseline {b:.2f}')
        if not counted:
            print('perf-gate: no instruction counter (virtual machine): gating the share only')
            if s > b * SHARE_BUDGET:
                fails.append(f'pane-share: {s:.2f}, over {SHARE_BUDGET}x the baseline {b:.2f}')
        for k, b in sorted(base.items()):
            v = measured.get(k)
            if v is None or not counted:
                continue
            change = (v - b) / b
            print(f'perf-gate: {k} {v:.2f} M instructions, baseline {b:.2f} ({change:+.1%})')
            if change > BUDGET:
                fails.append(f'{k}: {v:.2f} M instructions, {change:+.1%} over the baseline {b:.2f} (budget +{BUDGET:.0%})')
            elif change < -BUDGET:
                print(f'perf-gate: {k} is {-change:.0%} under the baseline; lower it with MOORING_PERF_UPDATE=1')
    for f in fails:
        print('perf-gate: FAIL', f)
    sys.exit(1 if fails else 0)


main()
