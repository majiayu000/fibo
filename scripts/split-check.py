#!/usr/bin/env python3
"""Checks a file split changed nothing but where code lives (docs/file-split/README.md).

  split-check.py audit BASE END          every refactor(split): commit in BASE..END moves code only
  split-check.py measure SHA OUT         run every check on SHA in its own worktree, into OUT
  split-check.py measure SHA OUT --repeat  scenes and perf-gate again on a built SHA
  split-check.py report BASE BASE2 END   compare measures; writes END/report.md, compare.html
  split-check.py self-test               the audit catches what it claims to

Measuring a commit opens real windows (isolated HOME and runtime, no real Agent).
Exit status 1 means something needs a person; the output says what.

BASE2 is the baseline measured a second time (--repeat). A check, a test or a scene that
regressed fails the report. Pictures never do: a clean move still differed outside the noise of
two baseline runs in 20 of 93, so they are sorted for a person to look at, largest first. A
scene failing on the baseline proves nothing. Run on an awake machine with little else going.
"""
import argparse, collections, hashlib, json, os, pathlib, re, shutil, struct, subprocess, sys, tempfile, time, zlib

BASE = pathlib.Path(__file__).resolve().parents[1]
CACHE = pathlib.Path.home() / 'Library/Caches/MooringVisual'
XCODE = '/Applications/Xcode.app/Contents/Developer'
ENV = {**os.environ, 'LIBGHOSTTY_VT_SYS_OPTIMIZE': 'ReleaseFast', **({'DEVELOPER_DIR': XCODE} if os.path.isdir(XCODE) else {})}
APP_MANIFEST = 'crates/mooring-app/Cargo.toml'
SPLIT = 'refactor(split):'
# Files a split commit may touch besides Rust sources.
BOOKKEEPING = {'crates/mooring-core/tests/file-size-baseline.txt', 'crates/mooring-core/tests/guard-baseline.txt', 'docs/file-split/README.md'}


def git(*args, cwd=BASE):
    return subprocess.run(['git', *args], cwd=cwd, check=True, text=True, capture_output=True, errors='replace').stdout


# --- What a Rust file says, whatever file it says it in ---

def strip_comments(s):
    """Comments gone; returns the code and a mask of it with string contents blanked."""
    code, mask, i, n = [], [], 0, len(s)
    def keep(text, blank=False):
        code.append(text)
        mask.append(re.sub(r'[^\n]', ' ', text) if blank else text)
    while i < n:
        if s.startswith('//', i):
            j = s.find('\n', i)
            i = n if j < 0 else j
        elif s.startswith('/*', i):
            depth, i = 1, i + 2
            while i < n and depth:
                if s.startswith('/*', i): depth, i = depth + 1, i + 2
                elif s.startswith('*/', i): depth, i = depth - 1, i + 2
                else: i += 1
        elif (m := re.match(r'b?r(#*)"', s[i:i + 40])) and (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == '_')):
            end = s.find('"' + m.group(1), i + m.end())
            end = n if end < 0 else end + 1 + len(m.group(1))
            keep(s[i:end], True); i = end
        elif s[i] == '"':
            j = i + 1
            while j < n and s[j] != '"':
                j += 2 if s[j] == '\\' else 1
            keep(s[i:j + 1], True); i = j + 1
        elif s[i] == "'" and (m := re.match(r"'(\\.[^']*|[^\\'])'", s[i:i + 14])):
            keep(s[i:i + m.end()], True); i += m.end()
        else:
            keep(s[i]); i += 1
    return ''.join(code), ''.join(mask)


def squash(text):
    """Tokens only: spacing and the trailing commas rustfmt adds when a line reflows."""
    return re.sub(r',(?=[)}\]])', '', re.sub(r'\s+', '', text))


VISIBILITY = re.compile(r'\bpub(\s*\([^)]*\))?\s+')
FN = re.compile(r'\bfn\s+([A-Za-z_][A-Za-z0-9_]*)')
HEAD = re.compile(r'(?:[ \t]*#!?\[[^\n]*\]\s*\n)*[ \t]*(?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:const|async|unsafe|extern(?:\s+"[^"]*")?)\s+)*$')


def parse(source):
    """Outermost functions as (name, fingerprint), and the other code as squashed lines."""
    code, mask = strip_comments(source)
    fns, rest, cursor, at = [], [], 0, 0
    while m := FN.search(mask, at):
        i, parens = m.end(), 0
        while i < len(mask) and not (parens == 0 and mask[i] in '{;'):
            parens += (mask[i] in '([') - (mask[i] in ')]')
            i += 1
        if i >= len(mask) or mask[i] == ';':  # a declaration, no body to move
            at = i + 1
            continue
        depth, j = 0, i
        while j < len(mask):
            depth += (mask[j] == '{') - (mask[j] == '}')
            j += 1
            if depth == 0:
                break
        begin = m.start()
        for start in range(max(cursor, m.start() - 600), m.start()):
            hit = HEAD.fullmatch(code, start, m.start())
            if hit and (start == 0 or code[start - 1] == '\n'):
                begin = start
                break
        rest.append(code[cursor:begin])
        body = VISIBILITY.sub('', code[begin:j], count=1)
        fns.append((m.group(1), hashlib.sha1(squash(body).encode()).hexdigest()[:12]))
        cursor = at = j
    rest.append(code[cursor:])
    lines = [squash(VISIBILITY.sub('', l)) for l in ''.join(rest).split('\n')]
    return fns, [l for l in lines if l]


STRUCTURE = re.compile(r'^(use.*;|(\w+,)*\w*(::\w+)*[,{]?|\}+[;,]?|mod\w+;|mod\w+\{|impl.*\{|#\[path=.*\]|#\[cfg\(test\)\]|#\[cfg\(feature=.*\)\]|#!?\[allow.*\])$')


def compare(before, after):
    """Compare two {path: source} trees. A moved fn or line cancels out wherever it landed."""
    fns_a, fns_b, rest_a, rest_b = collections.Counter(), collections.Counter(), collections.Counter(), collections.Counter()
    for tree, fns, rest in ((before, fns_a, rest_a), (after, fns_b, rest_b)):
        for source in tree.values():
            f, r = parse(source)
            fns.update(f); rest.update(r)
    changed = sorted({n for n, _ in (fns_a - fns_b) + (fns_b - fns_a)})
    lost = [l for l in (rest_a - rest_b).elements() if not STRUCTURE.match(l)]
    gained = [l for l in (rest_b - rest_a).elements() if not STRUCTURE.match(l)]
    # A declaration rustfmt reflowed has the same text across different lines.
    if ''.join(sorted(''.join(lost))) == ''.join(sorted(''.join(gained))):
        lost, gained = [], []
    return changed, lost, gained


def audit(args):
    commits = git('log', '--reverse', '--no-merges', '--format=%H\t%s', f'{args.base}..{args.end}').splitlines()
    problems, done = [], 0
    big = {l.split('\t')[0] for l in git('show', f'{args.base}:crates/mooring-core/tests/file-size-baseline.txt').splitlines()}
    for line in commits:
        sha, subject = line.split('\t', 1)
        files = git('diff', '--no-renames', '--name-only', f'{sha}^', sha).splitlines()
        if not subject.startswith(SPLIT):
            touched = sorted(big & set(files))
            if touched:
                print(f'NOTE {sha[:9]} is not a split commit and edits {", ".join(touched)}: {subject}')
            continue
        done += 1
        def tree(rev):
            out = {}
            for f in files:
                if f.endswith('.rs'):
                    try: out[f] = git('show', f'{rev}:{f}')
                    except subprocess.CalledProcessError: pass
            return out
        changed, lost, gained = compare(tree(f'{sha}^'), tree(sha))
        other = [f for f in files if not f.endswith('.rs') and f not in BOOKKEEPING]
        new = [f for f in files if f.startswith(('crates/mooring-app/src/terminal/pane', 'crates/mooring-app/src/desktop')) and f.endswith('.rs')]
        if changed or lost or gained or other:
            problems.append(sha)
            print(f'READ {sha[:9]} {subject}')
            if changed: print('  functions whose text changed: ' + ', '.join(changed))
            for l in lost[:30]: print('  gone: ' + l[:120])
            for l in gained[:30]: print('  new:  ' + l[:120])
            if other: print('  other files: ' + ', '.join(other))
        else:
            print(f'ok   {sha[:9]} {subject}')
        if new:
            # The list is in the theme's tests, wherever a split of its own put them.
            listed = git('grep', '-h', '"terminal/', args.end, '--', 'crates/mooring-app/src/ui') + git('grep', '-h', '"desktop.rs"', args.end, '--', 'crates/mooring-app/src/ui')
            for f in new:
                short = f.removeprefix('crates/mooring-app/src/')
                if git('cat-file', '-t', f'{args.end}:{f}').strip() == 'blob' and f'"{short}"' not in listed and not short.endswith('_tests.rs'):
                    problems.append(sha)
                    print(f'TRAP {short}: not in the color guard list in ui/theme.rs')
    # MOORING_GUARD_UPDATE turns a guard green whatever moved, so its totals are checked here.
    def totals(rev):
        counts = collections.Counter()
        for row in git('show', f'{rev}:crates/mooring-core/tests/guard-baseline.txt').splitlines():
            kind, _, n = row.split('\t')
            counts[kind] += int(n)
        return counts
    grown = totals(args.end) - totals(args.base)
    for kind, n in sorted(grown.items()):
        problems.append(kind)
        print(f'GUARD {kind}: {n} more than at the base; a split only moves these')
    print(f'{done} split commits, {len(problems)} to read by hand')
    return 1 if problems or not done else 0


# --- Running everything on one commit ---

SCENES = {name: '1' for name in '''COMPLETE REASON SPLIT_CLI ROUTE RECONNECT SCROLLBACK SCROLL_KEYS SELECTION_PIXEL SELECTION
    DIAGNOSTIC SPLIT PROGRAM_STATUS TAB_CWD SHORTCUT TAB_OVERFLOW TAB FOLLOW_INPUT LINK NEW_SPACE SPACE FOLDERLESS_SPACE
    LAYOUT PEEK WHY RESTART OVERVIEW PHONES PINNED_ASK PANE_DRAG RESUME'''.split()}
SCENES.update(WHEEL='seq 1 2000', KEYMAP='1', SUBAGENT='0')
PAGES = '''Main MainSpace StatusSessions HereClickOut SidebarTodos BoardDrawer BoardAccept BoardRefuse HereTyped RestartHost
    CloseIdle CloseWorking Markdown SelectHint HereMore TodoNew TodoNewBare TodoNewChecks TodoDetail TodoDetailAttach
    TodoDetailFull TodoDetailDone TodoStart Cleanup QuickTask ScreenWait StartPick IconMenu IconMascot SidebarDirectory
    SidebarWasm StyleStandard StyleTerminal PaneMenu ActionRun SearchEmpty SearchGroup Search SearchNone Closed OverviewEnd
    Overview Menubar BoardDrag BoardRefuseDrag'''.split()


def step(out, name, cmd, cwd, env=ENV, timeout=2400):
    log = out / f'{name}.log'
    start = time.time()
    with open(log, 'w') as f:
        try:
            code = subprocess.run(cmd, cwd=cwd, env=env, stdout=f, stderr=subprocess.STDOUT, timeout=timeout).returncode
        except subprocess.TimeoutExpired:
            code = 'timeout'
    print(f'{name}: {code} ({time.time() - start:.0f}s)', flush=True)
    return code


def scene(bin_dir, name, value, out):
    root = pathlib.Path(tempfile.mkdtemp(prefix='sc-', dir='/tmp'))
    try:
        for d in ('home', 'cfg/mooring', 'claude'):
            (root / d).mkdir(parents=True)
        (root / 'rt').mkdir(mode=0o700)
        out.mkdir(parents=True, exist_ok=True)
        env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin', 'HOME': str(root / 'home'), 'XDG_CONFIG_HOME': str(root / 'cfg'), 'CLAUDE_CONFIG_DIR': str(root / 'claude'),
               'CODEX_HOME': str(root / 'home/.codex'), 'LANG': 'en_US.UTF-8', 'SHELL': '/bin/zsh', 'GPUI_PAUSE_IDLE_DISPLAY_LINK': '1', f'MOORING_{name}_EVIDENCE': value}
        try:
            p = subprocess.run([str(bin_dir / 'mooring-app'), '--runtime', str(root / 'rt'), '--visual-evidence', str(out)], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors='replace', timeout=240)
            code, log = p.returncode, p.stdout
        except subprocess.TimeoutExpired as e:
            code, log = 'timeout', (e.stdout or b'').decode(errors='replace')
        (out / 'app.log').write_text(log)
        return 'pass' if code == 0 and 'panicked' not in log else f'fail {code}'
    finally:
        # The host outlives the app by design, and a failed scene may not have stopped it.
        pattern = f'serve --runtime {root}/rt'
        subprocess.run(['pkill', '-f', pattern], check=False)
        time.sleep(1)
        subprocess.run(['pkill', '-9', '-f', pattern], check=False)
        shutil.rmtree(root, ignore_errors=True)


def wait_for_quiet(most=300):
    """Scenes wait in wall-clock time; under another build's load they time out for nothing."""
    limit, waited = os.cpu_count() or 8, 0
    while os.getloadavg()[0] > limit and waited < most:
        time.sleep(15)
        waited += 15
    return round(os.getloadavg()[0], 1)


def scenes(work, out, only=None):
    results = {}
    bin_dir = work / 'target/debug'
    for name, value in SCENES.items():
        if only and name not in only:
            continue
        for attempt in (1, 2):
            load = wait_for_quiet()
            shutil.rmtree(out / 'scenes' / name, ignore_errors=True)
            results[name] = scene(bin_dir, name, value, out / 'scenes' / name)
            print(f'scene {name}: {results[name]} (try {attempt}, load {load})', flush=True)
            if results[name] == 'pass':
                break
    if only and 'pages' not in only:
        return results
    fixture = work / 'scripts/visual-fixture.py'
    env = {**ENV, 'MOORING_VISUAL_BIN': str(bin_dir), 'MOORING_FIXTURE_BOARD': '1'}
    for i in range(0, len(PAGES), 6):  # one run has 120 seconds
        chunk = ','.join(PAGES[i:i + 6])
        for attempt in (1, 2):
            wait_for_quiet()
            step(out, f'fixture-prepare-{i}', [sys.executable, fixture, 'prepare'], work, env, 300)
            ok = step(out, f'pages-{i}', [sys.executable, fixture, 'capture', out / 'scenes/pages'], work, {**env, 'MOORING_PAGE_LIST': chunk}, 180) == 0
            step(out, f'fixture-stop-{i}', [sys.executable, fixture, 'stop'], work, env, 60)
            results['pages ' + chunk] = 'pass' if ok else 'fail'
            if ok:
                break
    if only:
        return results
    for s in ('board', 'sidebar', 'typing'):
        tmp = f'/private/tmp/ms-{s}-{os.getpid()}'
        code = step(out, f'remote-{s}', ['bash', work / f'scripts/remote-{s}-evidence.sh'], work, {**ENV, 'MOORING_EVIDENCE_TMP': tmp}, 300)
        text = (out / f'remote-{s}.log').read_text(errors='replace')
        results[f'remote-{s}'] = 'pass' if code == 0 and 'PASS' in text else 'fail'
        if pathlib.Path(tmp, 'shots').exists():
            shutil.copytree(f'{tmp}/shots', out / f'scenes/remote-{s}', dirs_exist_ok=True)
        shutil.rmtree(tmp, ignore_errors=True)
    return results


def test_names(log):
    return [l[:-6] for l in pathlib.Path(log).read_text(errors='replace').splitlines() if l.endswith(': test')]


def terminals_left():
    """Free pseudo-terminals. At the limit every scene fails with 'Device not configured',
    which reads like a regression and is not one."""
    limit = int(subprocess.run(['sysctl', '-n', 'kern.tty.ptmx_max'], capture_output=True, text=True).stdout)
    return limit - len(list(pathlib.Path('/dev').glob('ttys*')))


def prepare_dependencies(work, sha):
    """This commit's own patched GPUI: a patch applied for one commit can't be taken back for
    another, and a baseline built on the end's patches would hide what they change."""
    wanted = git('rev-parse', f'{sha}:patches', f'{sha}:bootstrap.sh')
    deps, stamp = work / '.deps', work / '.deps-for'
    if stamp.exists() and stamp.read_text() == wanted and deps.is_dir():
        return
    if deps.is_symlink():
        deps.unlink()
    shutil.rmtree(deps, ignore_errors=True)
    subprocess.run(['sh', 'bootstrap.sh'], cwd=work, env=ENV, check=True, stdout=subprocess.DEVNULL)
    stamp.write_text(wanted)


def measure(args):
    left = terminals_left()
    if left < 100 and not args.force:
        sys.exit(f'only {left} pseudo-terminals free: close some sessions first, or --force')
    out = pathlib.Path(args.out).resolve(); out.mkdir(parents=True, exist_ok=True)
    sha = git('rev-parse', '--verify', f'{args.sha}^{{commit}}').strip()
    # One path for every commit: the fixture's sessions live under it, and pages show it.
    work = CACHE / 'split-work'
    if not (work / '.git').exists():
        git('worktree', 'prune')
        git('worktree', 'add', '--detach', str(work), sha)
    git('reset', '-q', '--hard', cwd=work)
    git('checkout', '-q', '--detach', sha, cwd=work)
    prepare_dependencies(work, sha)
    if not os.path.isdir(XCODE):
        # No metal compiler here: GPUI compiles its shaders at start instead. Both sides of a
        # comparison build the same way; perf-gate's numbers don't match the committed baseline.
        manifest = work / APP_MANIFEST
        text = manifest.read_text()
        assert 'features = ["font-kit"]' in text, APP_MANIFEST
        manifest.write_text(text.replace('features = ["font-kit"]', 'features = ["font-kit", "runtime_shaders"]'))
    # Scenes need a window being drawn: keep the display awake for this run.
    awake = subprocess.Popen(['caffeinate', '-dimsu', '-w', str(os.getpid())])
    r = {'sha': sha}
    if args.repeat:
        r['perf-gate'] = step(out, 'perf-gate', [sys.executable, 'scripts/perf-gate.py', 'target/release', str(out / 'perf-gate')], work)
    elif not args.only:
        visual = ['--features', 'visual-test']
        for name, cmd, cwd in (
            ('fmt', ['cargo', 'fmt', '--check'], work),
            ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'], work),
            ('check-visual', ['cargo', 'check', '-p', 'mooring-app', '--all-targets', *visual], work),
            ('ci-test', ['sh', 'ci-test.sh'], work),
            ('app-visual', ['cargo', 'test', '-p', 'mooring-app', *visual, '--no-fail-fast'], work),
            ('mobile-install', ['bun', 'install', '--frozen-lockfile'], work / 'mobile'),
            ('mobile-test', ['bun', 'test'], work / 'mobile'),
            ('mobile-types', ['bun', 'run', 'typecheck'], work / 'mobile'),
            ('protocol', [sys.executable, '-I', 'check.py'], work / 'eval/protocol-contract'),
            ('build-release', ['cargo', 'build', '--release', '-p', 'mooring-host', '-p', 'mooring-app', '--features', 'mooring-app/visual-test'], work),
            ('perf-gate', [sys.executable, 'scripts/perf-gate.py', 'target/release', str(out / 'perf-gate')], work),
            ('theme-scenes', [sys.executable, 'scripts/theme-scenes.py', 'target/release', str(out / 'theme-scenes')], work),
        ):
            r[name] = step(out, name, cmd, cwd)
        for name, cmd in (('default', ['cargo', 'test', '--workspace', '--', '--list']), ('visual-test', ['cargo', 'test', '-p', 'mooring-app', *visual, '--', '--list'])):
            step(out, f'list-{name}', cmd, work)
        names = [f'default\t{n}' for n in test_names(out / 'list-default.log')] + [f'visual-test\t{n}' for n in test_names(out / 'list-visual-test.log')]
        (out / 'tests.txt').write_text('\n'.join(sorted(names)) + '\n')
    step(out, 'build-debug', ['cargo', 'build', '-p', 'mooring-host', '-p', 'mooring-app', '--features', 'mooring-app/visual-test'], work)
    r['scenes'] = scenes(work, out, set(args.only.split(',')) if args.only else None)
    (out / 'measure.json').write_text(json.dumps(r, indent=1, ensure_ascii=False))
    awake.terminate()
    return 0


# --- Comparing measures ---

def png_rows(path):
    data = pathlib.Path(path).read_bytes()
    i, idat, width, height, kind = 8, [], 0, 0, 0
    while i < len(data):
        n = struct.unpack('>I', data[i:i + 4])[0]
        tag = data[i + 4:i + 8]
        if tag == b'IHDR':
            width, height, depth, kind = struct.unpack('>IIBB', data[i + 8:i + 18])
            assert depth == 8 and kind in (2, 6), f'{path}: unexpected PNG format'
        elif tag == b'IDAT':
            idat.append(data[i + 8:i + 8 + n])
        i += n + 12
    raw = zlib.decompress(b''.join(idat))
    bpp = 3 if kind == 2 else 4
    stride, prev, rows = width * bpp, bytearray(width * bpp), []
    for y in range(height):
        at = y * (stride + 1)
        f, row = raw[at], bytearray(raw[at + 1:at + 1 + stride])
        if f == 1:
            for x in range(bpp, stride): row[x] = (row[x] + row[x - bpp]) & 255
        elif f == 2:
            row = bytearray((a + b) & 255 for a, b in zip(row, prev))
        elif f == 3:
            for x in range(stride): row[x] = (row[x] + ((row[x - bpp] if x >= bpp else 0) + prev[x]) // 2) & 255
        elif f == 4:
            for x in range(stride):
                a, b, c = row[x - bpp] if x >= bpp else 0, prev[x], prev[x - bpp] if x >= bpp else 0
                p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                row[x] = (row[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(row)); prev = row
    return width, height, bpp, rows


def changed_rows(a, b):
    """{row: (first, last)} of the pixels that differ, or None when the sizes differ."""
    wa, ha, pa, ra = png_rows(a)
    wb, hb, pb, rb = png_rows(b)
    if (wa, ha, pa) != (wb, hb, pb):
        return None
    out = {}
    for y, (p, q) in enumerate(zip(ra, rb)):
        if p != q:
            xs = [x for x in range(wa) if p[x * pa:x * pa + pa] != q[x * pa:x * pa + pa]]
            out[y] = (xs[0], xs[-1], xs)
    return out


def outside_noise(diff, noise, margin=24):
    """Differing pixels outside what moved between the baseline's own two runs. A clock's
    new digit lights pixels near the old one, so the noise is widened by a margin."""
    bands = {}
    for y, (first, last, _) in noise.items():
        for near in range(y - margin // 2, y + margin // 2 + 1):
            low, high = bands.get(near, (first, last))
            bands[near] = (min(low, first), max(high, last))
    count = 0
    for y, (_, _, xs) in diff.items():
        low, high = bands.get(y, (1, 0))
        count += sum(not (low - margin <= x <= high + margin) for x in xs)
    return count


def same_name(path):
    """A picture's file by its name without the measured time some scenes put in it."""
    if path.exists():
        return path
    plain = re.sub(r'-\d+ms(?=\.png$)', '', path.name)
    found = [f for f in path.parent.glob('*.png') if re.sub(r'-\d+ms(?=\.png$)', '', f.name) == plain]
    return found[0] if found else path


def picture(base, again, end, shape_limit=0.02):
    """same, noise, moved N or unstable, for one picture in all three runs."""
    if pathlib.Path(base).read_bytes() == pathlib.Path(end).read_bytes():
        return 'same'
    diff = changed_rows(base, end)
    if diff is None:
        return 'moved size'
    if not pathlib.Path(again).exists():
        return f'moved {sum(len(v[2]) for v in diff.values())}'
    noise = changed_rows(base, again)
    if noise is None:
        return 'unstable'
    wide = sum(v[1] - v[0] + 1 for v in noise.values())
    width, height, _, _ = png_rows(base)
    if wide > shape_limit * width * height:
        return 'unstable'
    n = outside_noise(diff, noise)
    return f'moved {n}' if n else 'noise'


def perf_numbers(log):
    if not pathlib.Path(log).exists():
        return {}
    found = re.findall(r'^perf-gate: (\S+) ([0-9.]+) M instructions', pathlib.Path(log).read_text(errors='replace'), re.M)
    return {k: float(v) for k, v in found}


def report(args):
    a, a2, b = (pathlib.Path(p).resolve() for p in (args.base, args.again, args.end))
    ma, ma2, mb = (json.loads((d / 'measure.json').read_text()) for d in (a, a2, b))
    lines, bad = [f'# 拆分验收：{ma["sha"][:9]} 对比 {mb["sha"][:9]}', ''], 0
    lines += ['## 检查', '', '| 检查 | 基线 | 终点 | |', '|---|---|---|---|']
    for k in mb:
        if k in ('sha', 'scenes', 'perf-gate'):
            continue
        new = mb[k] != 0 and ma.get(k) == 0
        bad += new
        lines.append(f'| {k} | {ma.get(k)} | {mb[k]} | {"回归" if new else ""} |')
    pa, pa2, pe = perf_numbers(a / 'perf-gate.log'), perf_numbers(a2 / 'perf-gate.log'), perf_numbers(b / 'perf-gate.log')
    lines += ['', '## 性能（百万指令，门禁自身的预算见 perf-gate.py）', '', '| 项 | 基线两遍 | 终点 | |', '|---|---|---|---|']
    for k, v in sorted(pe.items()):
        top = max(pa.get(k, 0), pa2.get(k, 0))
        # Beyond 5% over the worst of the baseline's own two runs.
        worse = top > 0 and v > top * 1.05
        bad += worse
        lines.append(f'| {k} | {pa.get(k)} / {pa2.get(k)} | {v} | {"变慢" if worse else ""} |')
    lines += ['', '## 场景（失败已重跑一次）', '', '| 场景 | 基线 | 基线重复 | 终点 | |', '|---|---|---|---|---|']
    usable = []
    for k, v in mb['scenes'].items():
        w, w2 = ma['scenes'].get(k), ma2['scenes'].get(k)
        firm = w == 'pass' and w2 == 'pass'
        new = v != 'pass' and firm
        bad += new
        if firm and v == 'pass':
            usable.append(k)
        note = '回归' if new else '' if firm else '基线不稳，不作数' if 'pass' in (w, w2) else '基线就失败，不作数'
        lines.append(f'| {k} | {w} | {w2} | {v} | {note} |')
    for k in set(ma['scenes']) - set(mb['scenes']):
        if not args.partial:
            lines.append(f'| {k} | {ma["scenes"][k]} | {ma2["scenes"].get(k)} | 缺失 | |')
            bad += ma['scenes'][k] == 'pass'
    if (b / 'tests.txt').exists():
        def key(line):
            label, name = line.split('\t', 1)
            return label, name.rsplit('::', 1)[-1]
        ca = collections.Counter(key(l) for l in (a / 'tests.txt').read_text().splitlines())
        cb = collections.Counter(key(l) for l in (b / 'tests.txt').read_text().splitlines())
        missing = sorted((ca - cb).elements())
        bad += len(missing)
        lines += ['', '## 测试名单', '', f'基线 {sum(ca.values())}，终点 {sum(cb.values())}，少了 {len(missing)}，多了 {sum((cb - ca).values())}。', ''] + [f'- 缺 `{x[0]}` {x[1]}' for x in missing]
    tally, moved, unstable = collections.Counter(), [], []
    dirs = {('pages' if k.startswith('pages ') else k) for k in usable}
    # A page chunk that failed on any run leaves stale or missing pictures: only whole chunks count.
    for rel in sorted(f.relative_to(a / 'scenes') for d in dirs for f in (a / 'scenes' / d).rglob('*.png')):
        e = same_name(b / 'scenes' / rel)
        if not e.exists():
            verdict = 'moved missing'
        else:
            verdict = picture(a / 'scenes' / rel, same_name(a2 / 'scenes' / rel), e)
        tally[verdict.split()[0]] += 1
        pair = f'<div class=p><img src="{os.path.relpath(a / "scenes" / rel, b)}"><img src="{os.path.relpath(e, b)}"></div>'
        if verdict.startswith('moved'):
            size = verdict.split()[1]
            moved.append((int(size) if size.isdigit() else 10**9, f'<h2>{rel} · {verdict}</h2>{pair}'))
        elif verdict == 'unstable':
            unstable.append(f'<h2>{rel}</h2>{pair}')
    moved = [html for _, html in sorted(moved, key=lambda m: -m[0])]
    lines += ['', '## 画面（不判通过，要人看）', '', f'逐像素相同 {tally["same"]} 张；差异都在噪声区 {tally["noise"]} 张；噪声区外有差异 {tally["moved"]} 张；基线两遍就差很多 {tally["unstable"]} 张。要看 {tally["moved"] + tally["unstable"]} 张，见 compare.html。']
    style = 'body{margin:16px;background:#111;color:#ddd;font:14px -apple-system,sans-serif}.p{display:grid;grid-template-columns:1fr 1fr;gap:12px}img{width:100%}'
    (b / 'compare.html').write_text(f'<!doctype html><html lang="zh"><meta charset="utf-8"><title>拆分前后画面</title><style>{style}</style><h1>噪声区外有差异（左基线，右终点）</h1>' + ''.join(moved) + '<h1>基线自身不稳定，要人看</h1>' + ''.join(unstable) + '</html>')
    (b / 'report.md').write_text('\n'.join(lines) + '\n')
    print('\n'.join(lines))
    print(f'\n{bad} regressions; {len(moved) + len(unstable)} pictures for a person; {b / "compare.html"}')
    return 1 if bad else 0


# --- The audit must catch what it claims to ---

def self_test(_):
    src = '''use a::b;
pub(super) const LIMIT: u8 = 3;
/// Doc.
#[cfg(feature = "visual-test")]
pub fn one(a: u8, b: u8) -> u8 {
    let c = a - b;
    let d = c + 1;
    if d < LIMIT { d } else { 0 }
}
#[test]
fn two() { assert_eq!(one(3, 1), 3); }
'''
    moved = {'a.rs': 'mod x;\nuse x::*;\npub(super) const LIMIT: u8 = 3;\n', 'x.rs': '''use super::*;
// A comment is not behavior.
#[cfg(feature = "visual-test")]
pub(crate) fn one(
    a: u8,
    b: u8,
) -> u8 {
    let c = a - b;
    let d = c + 1;
    if d < LIMIT { d } else { 0 }
}
#[test]
fn two() { assert_eq!(one(3, 1), 3); }
'''}
    def verdict(tree):
        return compare({'a.rs': src}, tree)
    assert verdict(moved) == ([], [], []), verdict(moved)
    cases = {
        'a condition': ('d < LIMIT', 'd <= LIMIT'),
        'two lines swapped': ('    let c = a - b;\n    let d = c + 1;\n', '    let d = c + 1;\n    let c = a - b;\n'),
        'operands swapped': ('a - b', 'b - a'),
        'an attribute dropped': ('#[cfg(feature = "visual-test")]\n', ''),
        'a test deleted': ('#[test]\nfn two() { assert_eq!(one(3, 1), 3); }\n', ''),
        'a string': ('assert_eq!(one(3, 1), 3)', 'assert_eq!(one(3, 1), 4)'),
        'a constant': ('LIMIT: u8 = 3', 'LIMIT: u8 = 4'),
    }
    for name, (old, new) in cases.items():
        tree = {k: v.replace(old, new) for k, v in moved.items()}
        assert tree != moved, name
        changed, lost, gained = verdict(tree)
        assert changed or lost or gained, f'missed: {name}'
    tree = {**moved, 'y.rs': 'fn two() { assert_eq!(one(3, 1), 3); }\n'}
    assert verdict(tree)[0] == ['two'], 'missed: a copy left behind'
    near = {10: (100, 110, [100, 110])}
    assert outside_noise({10: (120, 120, [120])}, near) == 0, 'a clock digit beside the noise'
    assert outside_noise({10: (500, 500, [500])}, near) == 1, 'a change away from the noise'
    assert outside_noise({200: (105, 105, [105])}, near) == 1, 'a change in another row'
    print(f'self-test ok: a clean move passes, {len(cases) + 1} changes are caught')
    return 0


parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
commands = parser.add_subparsers(required=True)
s = commands.add_parser('audit'); s.add_argument('base'); s.add_argument('end'); s.set_defaults(go=audit)
s = commands.add_parser('measure'); s.add_argument('sha'); s.add_argument('out'); s.add_argument('--repeat', action='store_true'); s.add_argument('--force', action='store_true'); s.add_argument('--only', help='scene names or pages, comma separated'); s.set_defaults(go=measure)
s = commands.add_parser('report'); s.add_argument('base'); s.add_argument('again'); s.add_argument('end'); s.add_argument('--partial', action='store_true'); s.set_defaults(go=report)
s = commands.add_parser('self-test'); s.set_defaults(go=self_test)
args = parser.parse_args()
sys.exit(args.go(args))
