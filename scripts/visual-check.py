#!/usr/bin/env python3
"""Evidence for a fix, gathered the same way each time, so nobody reports "fixed" from a
look at the new code alone.

  python3 scripts/visual-check.py shots SHA OUT [--pages Overview@1420,Overview@1225]
      Builds SHA^ and SHA, renders both against the same fixture data
      (visual-fixture.py with MOORING_FIXTURE_BOARD=1), writes OUT/compare.html.
      A look decides: the old build must show the bug, the new one must not.

  python3 scripts/visual-check.py guard SHA TEST --undo FILE OLD NEW [--package mooring-app]
      At SHA: TEST must pass, then with OLD replaced by NEW in FILE it must fail. A compile
      error is not a failure of the test and is reported as such.

Builds happen in one worktree under ~/Library/Caches/MooringVisual/check, so the main
checkout and its target stay untouched and a later run rebuilds only what changed. The first
run compiles everything. Nothing here pushes, commits or touches a real workspace.
"""
import argparse, html, os, pathlib, re, shutil, subprocess, sys

BASE = pathlib.Path(__file__).resolve().parents[1]
WORK = pathlib.Path.home() / 'Library/Caches/MooringVisual/check'
ENV = {**os.environ, 'DEVELOPER_DIR': '/Applications/Xcode.app/Contents/Developer', 'LIBGHOSTTY_VT_SYS_OPTIMIZE': 'ReleaseFast'}


def run(*cmd, cwd=WORK, env=ENV, check=True, capture=False):
    return subprocess.run([str(c) for c in cmd], cwd=cwd, env=env, check=check, text=True,
                          stdout=subprocess.PIPE if capture else None, stderr=subprocess.STDOUT if capture else None)


def checkout(sha):
    """The worktree at `sha`, with any earlier undo gone."""
    resolved = run('git', 'rev-parse', '--verify', f'{sha}^{{commit}}', cwd=BASE, capture=True).stdout.strip()
    if not (WORK / '.git').exists():
        run('git', 'worktree', 'prune', cwd=BASE)
        run('git', 'worktree', 'add', '--detach', WORK, resolved, cwd=BASE)
    run('git', 'reset', '-q', '--hard')
    run('git', 'checkout', '-q', '--detach', resolved)
    deps = WORK / '.deps'
    if not deps.exists():
        deps.symlink_to((BASE / '.deps').resolve())
    return resolved


def capture(sha, out, pages):
    resolved = checkout(sha)
    run('cargo', 'build', '-q', '-p', 'mooring-host', '-p', 'mooring-app', '--features', 'mooring-app/visual-test')
    env = {**ENV, 'MOORING_VISUAL_BIN': str(WORK / 'target/debug'), 'MOORING_FIXTURE_BOARD': '1', 'MOORING_PAGE_LIST': pages}
    # The fixture is this checkout's, so an old commit gets the same data as the new one.
    fixture = BASE / 'scripts/visual-fixture.py'
    run(sys.executable, fixture, 'prepare', cwd=BASE, env=env)
    try:
        run(sys.executable, fixture, 'capture', out, cwd=BASE, env=env)
    finally:
        run(sys.executable, fixture, 'stop', cwd=BASE, env=env)
    return resolved


def shots(args):
    out = pathlib.Path(args.out).resolve()
    shutil.rmtree(out, ignore_errors=True)
    before = capture(f'{args.sha}^', out / 'before', args.pages)
    after = capture(args.sha, out / 'after', args.pages)
    rows = ''
    for shot in sorted((out / 'after').glob('*-native.png')):
        name = html.escape(shot.name)
        rows += f'<h2>{name}</h2><div class="pair"><figure><figcaption>修复前 {before[:7]}</figcaption><img src="before/{name}"></figure><figure><figcaption>修复后 {after[:7]}</figcaption><img src="after/{name}"></figure></div>'
    (out / 'compare.html').write_text(f'''<!doctype html><html lang="zh"><meta charset="utf-8"><title>修复前后对照</title>
<style>body{{margin:16px;background:#111;color:#ddd;font:14px -apple-system,sans-serif}}.pair{{display:grid;grid-template-columns:1fr 1fr;gap:12px}}figure{{margin:0}}img{{width:100%}}</style>
<h1>修复前后对照</h1>{rows}</html>''')
    print(out / 'compare.html')


def guard(args):
    checkout(args.sha)
    command = ['cargo', 'test', '-p', args.package, args.test]
    first = run(*command, check=False, capture=True)
    if first.returncode != 0 or not re.search(r'test result: ok\. [1-9]\d* passed', first.stdout):
        sys.exit(f'{args.test}: does not pass or did not run at {args.sha}\n{first.stdout[-2000:]}')
    target = WORK / args.undo[0]
    text = target.read_text()
    if args.undo[1] not in text:
        sys.exit(f'{args.undo[0]}: the text to undo is not there')
    target.write_text(text.replace(args.undo[1], args.undo[2]))
    try:
        second = run(*command, check=False, capture=True)
    finally:
        run('git', 'checkout', '--', args.undo[0])
    if 'could not compile' in second.stdout:
        sys.exit(f'undo does not compile, so it proves nothing\n{second.stdout[-2000:]}')
    if second.returncode == 0:
        sys.exit(f'GUARD IS EMPTY: {args.test} still passes with the fix undone')
    print(f'guard holds: {args.test} passes with the fix and fails without it')


parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
commands = parser.add_subparsers(required=True)
s = commands.add_parser('shots'); s.add_argument('sha'); s.add_argument('out'); s.add_argument('--pages', default='Overview@1420,Overview@1225'); s.set_defaults(go=shots)
g = commands.add_parser('guard'); g.add_argument('sha'); g.add_argument('test'); g.add_argument('--undo', nargs=3, required=True, metavar=('FILE', 'OLD', 'NEW')); g.add_argument('--package', default='mooring-app'); g.set_defaults(go=guard)
args = parser.parse_args()
args.go(args)
