#!/usr/bin/env python3
"""The window's performance gate. Builds nothing: BIN_DIR holds `fibo` and a
`fibo-app` built with `--release --features fibo-app/visual-test`.

  python3 scripts/perf-gate.py BIN_DIR [OUT]

On the 2000-session workspace of scripts/perf-fixture.py, with no turn running:
- a frame that only a terminal asked for, and a full redraw, must stay within BUDGET of
  the instructions in crates/fibo-app/perf-baseline.txt. Retired instructions, not
  time: the same build's CPU time moves 2.5x with machine load, its instructions <1%.
  MOORING_PERF_UPDATE=1 writes the measured numbers there (only for a change you mean).
  A virtual machine (the CI runner) counts no instructions; there a terminal frame's CPU
  time as a share of a full redraw's, measured back to back in one process, must stay
  within SHARE_BUDGET of `pane-share` (0.14 here; 1.0 with the
  sidebar drawn each frame);
- terminal output must not draw the sidebar again (it drew the whole sidebar each
  frame once: 7x the instructions), nor the tab strip (about 0.24 M a tab);
- a frame of the left column's motion (scripts' frames-motion: 今天 opening, held midway)
  must stay within BUDGET too: the column's motion draws the sidebar each display frame
  for a third of a second a click;
- so must the same motion in the layouts that need a machine: frames-section (本机's whole
  section folding under 按机器), frames-rail and frames-railrest (the rail with its list
  arriving, and at rest; a rail beside the column instead of over it cost 129 M at rest);
- terminal output with 看改动 open on 5000 files (scripts' frames-changes) must stay within
  BUDGET and must not draw the panel again: it is a cached view, and laying it out for every
  pane's output would cost several times the frame. Its own draw (frames-changesdraw) must
  too: copying every file on each draw once took 14 ms at this size;
- the running mark turning (scripts' frames-turning): a step keeps the sidebar's last draw and
  the mark really moves; with no mark on screen, or the setting off, the view stops drawing
- 设置 › 插件 (scripts' frames-plugins): a frame with one animation card playing, opening
  settings, and a terminal's frame under themes with a picture or a see-through window must
  stay within BUDGET (or, without a counter, within SHARE_BUDGET of a full redraw or a plain
  terminal frame in the same process). Pointing at a card once drew 79 M a frame, the whole
  page laid out again. Opening 插件 the first time (its catalog read, its words shaped and
  drawn for the first time) must stay within BUDGET too: one read is a few M of some 350,
  so a cold file is not what moves it. A stopped page must ask for no frames, a reused page must match a
  fresh one pixel for pixel, and every animation and theme must really have played or
  applied: a gate over a static picture passes whatever the code does. What the compositor
  spends on a see-through window is not here: see scripts/theme-live.py;
- a frame drawn without refreshing must match one drawn with a full refresh, pixel for
  pixel, after each step of redraw-check;
- a split-pane jump must request frames while its ring is visible and none after it ends,
  even when the window is forced to redraw after the fade;
- with nothing happening, one catch-up must not schedule the next (a loop drew the whole
  window several times a second).
"""
import fcntl, os, pathlib, re, signal, subprocess, sys, tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / 'crates/fibo-app/perf-baseline.txt'
BUDGET = 0.15
SHARE_BUDGET = 2.0
FIXTURE = [sys.executable, str(ROOT / 'scripts/perf-fixture.py')]
LOCK = pathlib.Path.home() / 'Library/Caches/MooringPerf/gate.lock'


def wait_for_other_gates():
    """One gate at a time: competing load moves the CPU times a VM run goes by.
    The system lets go of the lock when this process ends, however it ends."""
    LOCK.parent.mkdir(parents=True, exist_ok=True)
    held = LOCK.open('w')
    try:
        fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        print('perf-gate: another gate is running on this machine, waiting for it', flush=True)
        fcntl.flock(held, fcntl.LOCK_EX)
    return held


def probe(bindir, out, plan, catch_up, machine=False, running=False):
    env = {**os.environ, 'NORUN': '1'}
    if running:
        env.pop('NORUN')
        env['FEWNEED'] = '1'
    if machine:
        env['MACHINE'] = '1'
    env.pop('MOORING_NO_CATCH_UP', None)
    if not catch_up:
        env['MOORING_NO_CATCH_UP'] = '1'
    LOCK.parent.mkdir(parents=True, exist_ok=True)
    root = tempfile.mkdtemp(prefix='p-', dir=LOCK.parent)
    try:
        subprocess.run(FIXTURE + ['prepare', root, str(bindir)], env=env, check=True, stdout=subprocess.DEVNULL)
        r = subprocess.run(FIXTURE + ['probe', root, str(bindir), str(out), plan], env=env, check=True,
                           capture_output=True, text=True)
    finally:
        subprocess.run(FIXTURE + ['stop', root], check=True)
    print(r.stdout.strip())
    return r.stdout


def plugin_shares(ms, text):
    """Costs a machine without a counter can compare: within one process, against a frame
    the same process drew, so the machine's speed cancels out. A theme's share is the probe's
    own, against the plain look drawn in turns with it."""
    themes = [float(v) for v in re.findall(r'^frames-theme\w+: [^\n]*; ([\d.]+) of the plain look', text, re.M)]
    return {'cardplay-share': ms.get('frames-cardplay', 0) / ms.get('frames-probefull', 1), 'theme-share': max(themes, default=0)}


def main():
    lock = wait_for_other_gates()  # Held until this process ends.
    bindir = pathlib.Path(sys.argv[1]).resolve()
    out = pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else ROOT / 'target/perf-gate').resolve()
    fails = []
    text = probe(bindir, out / 'frames', 'title-check,frames-pane,frames-changes,frames-main,frames-motion,frames-arrive,redraw-check,recognition-check,history-check', catch_up=False)
    if 'history-check:' not in text:
        fails.append('no history-check line in the probe output')
    measured = {k: float(v) for k, v in re.findall(r'(frames-\w+): [^\n]*?([\d.]+) M instructions', text)}
    ms = {k: float(v) for k, v in re.findall(r'(frames-\w+): ([\d.]+) ms CPU', text)}
    if set(measured) != {'frames-pane', 'frames-changes', 'frames-changesdraw', 'frames-main', 'frames-motion', 'frames-arrive'} or set(ms) != set(measured):
        fails.append(f'no frame numbers in the probe output: {measured} {ms}')
        ms = {'frames-pane': 1, 'frames-main': 1}
    share = ms['frames-pane'] / ms['frames-main']
    # Counted, not timed, so a virtual machine gates it too: a line of output brings 18 new
    # runs, and a cache that loses the runs that only moved shapes the whole screen (774).
    # Its own workspace: after the others the shell in the shown pane no longer runs what
    # the probe types.
    scroll_text = probe(bindir, out / 'scroll', 'frames-scroll', catch_up=False)
    measured.update({k: float(v) for k, v in re.findall(r'(frames-scroll): [^\n]*?([\d.]+) M instructions', scroll_text)})
    scrolled = re.search(r'frames-scroll: [^\n]*?([\d.]+) runs shaped', scroll_text)
    if not scrolled:
        fails.append('no frames-scroll line in the probe output')
    elif not 1 <= float(scrolled.group(1)) <= 40:
        fails.append(f'a scrolled line shaped {scrolled.group(1)} runs, 18 are new: '
                     'runs that only moved must keep their shapes, and 0 means nothing scrolled')
    scroll_wakes = re.search(r'idle pane wakes (\d+)', scroll_text)
    if not scroll_wakes or int(scroll_wakes[1]):
        fails.append('scrolling a shell with no floating ask must leave its pane quiet: ' +
                     (scroll_wakes[0] if scroll_wakes else 'no idle wake count'))
    panel = re.search(r'frames-changes: [^\n]*drawn before (\d+); drew again (\d+)', text)
    if not panel:
        fails.append('no frames-changes line in the probe output')
    else:
        if not int(panel.group(1)):
            fails.append('看改动 was never drawn: the gate measured a pane without it')
        if int(panel.group(2)):
            fails.append(f'terminal output drew the open 看改动 panel {panel.group(2)} times: it must stay cached')
    # MOORING_PERF_NO_COUNTER=1 takes the runner's path on real hardware, to test it.
    counted = all(v > 0 for v in measured.values()) and not os.environ.get('MOORING_PERF_NO_COUNTER')
    # 按机器 and the rail need a machine configured, which changes the column for the rest.
    with_machine = probe(bindir, out / 'machine', 'frames-section,frames-rail', catch_up=False, machine=True)
    found = {k: float(v) for k, v in re.findall(r'(frames-\w+): [^\n;]*?([\d.]+) M instructions', with_machine)}
    if set(found) != {'frames-section', 'frames-rail', 'frames-railrest'}:
        fails.append(f'no machine layout numbers in the probe output: {found}')
    measured.update(found)
    # The turning mark needs rows running on screen, which changes the column for the rest.
    turning = probe(bindir, out / 'turning', 'frames-turning', catch_up=False, running=True)
    t = re.search(r'frames-turning: ([\d.]+) ms CPU per step, ([\d.]+) M instructions; at rest (\d+) rows (\d+) draws; rows (\d+); moved (\d+) px; sidebar kept its draw (\w+); folded (\d+) drew (\d+) main (\d+) opened (\d+); off (\d+) rows (\d+) draws', turning)
    if not t:
        fails.append('no frames-turning line in the probe output')
    else:
        used, cost = float(t[1]), float(t[2])
        rest_rows, rest_draws, rows, moved, kept, folded, drew, main_rows, opened, off_rows, off_draws = int(t[3]), int(t[4]), int(t[5]), int(t[6]), t[7], int(t[8]), int(t[9]), int(t[10]), int(t[11]), int(t[12]), int(t[13])
        measured['frames-turning'] = cost
        if rest_rows or rest_draws:
            fails.append(f'the setting off still drew turning marks: {rest_rows} rows, {rest_draws} draws')
        if not rows or not moved or used <= 0:
            fails.append(f'the marks did not turn: {rows} rows, {moved} px, {used} ms: the gate measured nothing')
        if kept != 'true':
            fails.append('a turning step drew the sidebar again: it must stay cached')
        if folded or opened != rows:
            fails.append(f'marks did not follow their rows: {rows} rows, folded {folded}, opened {opened}')
        if bool(drew) != bool(main_rows):
            fails.append(f'folded sidebar: {main_rows} main-area marks but {drew} steps')
        if off_rows or off_draws:
            fails.append(f'turned off, the marks kept drawing: {off_rows} rows, {off_draws} draws')
    plugin_text = probe(bindir, out / 'plugins', 'frames-plugins', catch_up=False)
    costs = dict(re.findall(r'^(frames-\w+|settings-open|settings-plugins-first-frame): [^\n]*?([\d.]+) M instructions', plugin_text, re.M))
    plugin_ms = {k: float(v) for k, v in re.findall(r'^(frames-\w+): ([\d.]+) ms CPU', plugin_text, re.M)}
    wanted = {'frames-cardplay', 'frames-cardrest', 'settings-open', 'settings-plugins-first-frame', 'frames-themeimage', 'frames-themepaper', 'frames-themeglass'}
    if not wanted <= set(costs) or not {'frames-probefull', 'frames-cardplay'} <= set(plugin_ms) or len(re.findall(r'of the plain look', plugin_text)) != 3:
        fails.append(f'no plugin page numbers in the probe output: {sorted(costs)}')
    measured.update({k: float(costs[k]) for k in wanted & set(costs)})
    if plugin_ms.get('frames-cardplay', 0) <= 0:
        fails.append(f"a playing card measured {plugin_ms.get('frames-cardplay')} ms: the gate measured nothing")
    played = re.search(r'the costliest of (\d+):', plugin_text)
    if not played or int(played.group(1)) < 16:
        fails.append(f'animation cards did not play: {played.group(0) if played else "no count"}')
    asks = re.search(r'stage-asks: at rest (\d+), pointing (\d+), after leaving (\d+)', plugin_text)
    if not asks:
        fails.append('no stage-asks line in the probe output')
    else:
        rest, pointing, after = map(int, asks.groups())
        if rest or after:
            fails.append(f'a stopped plugin page asked for frames: {rest} at rest, {after} after leaving')
        if not pointing:
            fails.append('a card pointed at asked for no frames: the gate measured nothing')
    if 'player-pixels: same; moves yes' not in plugin_text:
        line = next((l for l in plugin_text.splitlines() if 'player-pixels' in l), 'no player-pixels line')
        fails.append(f'plugin page reuse: {line}')
    not_applied = re.findall(r'theme-applied (\S+): NO', plugin_text)
    if not_applied or plugin_text.count('theme-applied') != 3:
        fails.append(f'themes not in force while measured: {not_applied or plugin_text.count("theme-applied")}')
    redraw = next((l for l in text.splitlines() if 'redraw-check:' in l), '')
    for step, verdict in re.findall(r'([\w-]+): (same|DIFF[^;]*)', redraw):
        if verdict != 'same':
            fails.append(f'redraw-check {step}: {verdict} (pictures in {out / "frames"})')
    if 'sidebar kept its draw true' not in redraw:
        fails.append('terminal output drew the sidebar again: ' + (redraw or 'no redraw-check line'))
    strip = re.search(r'frames-pane: [^\n]*tab strip drew (\d+)', text)
    if not strip or int(strip.group(1)):
        fails.append(f'terminal output drew the tab strip {strip.group(1) if strip else "?"} times in 300 frames: it is a cached view')
    if not redraw:
        fails.append('no redraw-check line in the probe output')
    ring = re.search(r'focus ring: active (true|false), expired (true|false), extra frames (\d+)', redraw)
    if not ring:
        fails.append('no split-pane focus ring result in redraw-check')
    elif ring[1] != 'true' or ring[2] != 'true':
        fails.append(f'{ring[0]}: the probe must reach both sides of the fade')
    elif int(ring[3]):
        fails.append(f'{ring[0]}: an expired focus ring requested another frame')
    recognition = re.search(
        r'^recognition-check: (\d+) sidebar draws for (\d+) diagnostics updates; .*; '
        r'metrics arrived (true|false); revision alone painted (true|false); '
        r'phase painted (true|false), (\d+) px changed$', text, re.M)
    if not recognition:
        fails.append('no recognition status redraw result')
    elif not int(recognition[2]) or recognition[3] != 'true':
        fails.append(f'{recognition[0]}: the probe must receive the diagnostics')
    elif recognition[5] != 'true' or not int(recognition[6]):
        fails.append(f'{recognition[0]}: a change in recognition phase must still draw')
    elif int(recognition[1]) or recognition[4] == 'true':
        fails.append(f'{recognition[0]}: host bookkeeping drew the sidebar again')
    titles = re.search(r'^title-check: (\d+) sidebar draws, (\d+) strip draws for (\d+) title changes; '
                       r'.*; title arrived (true|false), (\d+) px changed$', text, re.M)
    if not titles:
        fails.append('no terminal title redraw result')
    elif not int(titles[3]) or titles[2] != titles[3] or titles[4] != 'true' or not int(titles[5]):
        fails.append(f'{titles[0]}: every changed title must reach the strip')
    elif int(titles[1]):
        fails.append(f'{titles[0]}: terminal titles drew the sidebar again')

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
        shares = plugin_shares(plugin_ms, plugin_text)
        BASELINE.write_text(''.join(f'{k} {v:.2f}\n' for k, v in sorted({**measured, 'pane-share': share, **shares}.items())))
        print(f'perf-gate: wrote {BASELINE.relative_to(ROOT)}')
    else:
        base = {k: float(v) for k, v in (l.split() for l in BASELINE.read_text().splitlines() if l.strip())}
        s, b = share, base['pane-share']
        print(f'perf-gate: a terminal frame costs {s:.2f} of a full redraw, baseline {b:.2f}')
        if not counted:
            print('perf-gate: no instruction counter (virtual machine): gating the share only')
            if s > b * SHARE_BUDGET:
                fails.append(f'pane-share: {s:.2f}, over {SHARE_BUDGET}x the baseline {b:.2f}')
            for k, v in plugin_shares(plugin_ms, plugin_text).items():
                print(f'perf-gate: {k} {v:.2f}, baseline {base[k]:.2f}')
                if v > base[k] * SHARE_BUDGET:
                    fails.append(f'{k}: {v:.2f}, over {SHARE_BUDGET}x the baseline {base[k]:.2f}')
        for k, b in sorted(base.items()):
            v = measured.get(k)
            if k.endswith('-share') or v is None or not counted:
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


if __name__ == '__main__':
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f'interrupted by signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    main()
