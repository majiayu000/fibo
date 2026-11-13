#!/usr/bin/env python3
"""Walk every phone screen over adb and keep the evidence: docs/phone-app/walkthrough.md.

A run lives in ~/Documents/claude-pages/mooring/<date>-phone-walk[-N]/: one screenshot and
UI dump per step, results.json, and index.html listing every checklist item, so an item
nobody walked shows up as 没验 instead of being forgotten. Screenshots can hold private
content: they stay out of the repo.

  phone-walk.py start                  new run; records the App and APK versions
  phone-walk.py setup                  test git dir + test Claude session (marker: phone-walk)
  phone-walk.py open-chat [终端]        deep link straight into the test chat, never via home cards
  phone-walk.py shot <编号> [说明]       screenshot + UI dump for a checklist item
  phone-walk.py tap <文字> [--contains]  tap the one element with this text
  phone-walk.py type <文字>              type ASCII into the focused field
  phone-walk.py key hide|back|enter|home  hide the keyboard (only if up), or press a key
  phone-walk.py texts                   print the texts on screen with their centers
  phone-walk.py push-image              put a test picture in the phone's gallery
  phone-walk.py pick-image              in the system picker, select that picture and nothing else
  phone-walk.py mark <编号> 过|不过|跳过 <说明>
  phone-walk.py report                  write index.html and print what is not walked yet
  phone-walk.py teardown                close the test terminals, delete the test dirs
"""
import datetime
import html
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CHECKLIST = REPO / "docs/phone-app/walkthrough.md"
PAGES = Path.home() / "Documents/claude-pages/mooring"
# Each terminal keeps its own run: two sessions walking at once must not write into each
# other's (2026-11-09 one did). The phone itself is one: the lock says who walks it now.
WHO = os.environ.get("MOORING_TERMINAL") or os.environ.get("KEEPLINE_TERMINAL") or "shell"
CURRENT = PAGES / f".phone-walk-current-{re.sub(r'[^A-Za-z0-9_-]', '_', WHO)}"
LOCK = PAGES / ".phone-walk-lock"
# A run untouched this long has stopped; its lock no longer holds the phone.
STALE_S = 600
ADB = os.environ.get("ADB", str(Path.home() / "Library/Android/sdk/platform-tools/adb"))
SERIAL = os.environ.get("ANDROID_SERIAL", "")
RUNTIME = Path.home() / ".local/share/keepline-workbench"
MARKER = "phone-walk"
PACKAGE = "dev.mooring.mobile"
# Buttons that change something. They are only pressed while the screen shows the marker,
# i.e. inside the test session or on its card.
GUARDED = ("允许", "拒绝", "停下", "发送", "发给它", "标为看过", "标记完成", "接着做", "开始",
           "排队", "先停下", "记下", "接着管", "已处理", "稍后提醒")
NEVER = ("移除", "以后都允许", "换机器码", "重新扫码")


def die(msg):
    sys.exit(f"phone-walk: {msg}")


def fibo():
    found = shutil.which("fibo") or shutil.which("mooring")
    if found:
        return found
    # An app updated in place keeps its folder name, so an old install is still Mooring.app,
    # and one from before the program rename has only `mooring` inside.
    for name in ("Fibo", "Mooring"):
        for program in ("fibo", "mooring"):
            app = Path(f"/Applications/{name}.app/Contents/MacOS/{program}")
            if app.exists():
                return str(app)
    return die("找不到 fibo 命令")


def adb(*args, out=False):
    serial = SERIAL or device()
    cmd = [ADB, "-s", serial, *args]
    if out:
        return subprocess.run(cmd, capture_output=True, check=True).stdout
    subprocess.run(cmd, check=True)


def device():
    lines = subprocess.run([ADB, "devices"], capture_output=True, text=True, check=True).stdout
    real = [l.split()[0] for l in lines.splitlines()[1:]
            if l.endswith("\tdevice") and not l.startswith("emulator-")]
    if len(real) != 1:
        die(f"要正好一台真手机，现在是 {real}；用 ANDROID_SERIAL 指定")
    return real[0]


def checklist():
    rows = re.findall(r"^\| `([a-z.]+)` \| ([^|]+) \| ([^|]+) \| ([^|]+) \| ([^|]+) \|$",
                      CHECKLIST.read_text(), re.M)
    return [dict(id=r[0], page=r[1].strip(), how=r[2].strip(), look=r[3].strip(), mode=r[4].strip())
            for r in rows]


def run_dir():
    if not CURRENT.exists():
        die("还没有开始，先 phone-walk.py start")
    return Path(CURRENT.read_text().strip())


def state():
    p = run_dir() / "results.json"
    return json.loads(p.read_text()) if p.exists() else {}


def save(s):
    (run_dir() / "results.json").write_text(json.dumps(s, ensure_ascii=False, indent=1))


def holder():
    """Who else walks the phone now: (terminal, run dir), or None."""
    try:
        who, run = LOCK.read_text().split("\n")[:2]
    except (OSError, ValueError):
        return None
    if who == WHO:
        return None
    results = Path(run) / "results.json"
    touched = results.stat().st_mtime if results.exists() else LOCK.stat().st_mtime
    return (who, run) if time.time() - touched < STALE_S else None


def start():
    other = holder()
    if other and "--force" not in sys.argv:
        die(f"终端 {other[0]} 正在走查这台手机（{other[1]}，{STALE_S // 60} 分钟内有动静）。"
            "两个会话同时点同一台手机会互相打乱：等它走完，或确认它已停下再加 --force")
    day = datetime.date.today().isoformat()
    d, n = PAGES / f"{day}-phone-walk", 2
    while d.exists():
        d, n = PAGES / f"{day}-phone-walk-{n}", n + 1
    d.mkdir(parents=True)
    CURRENT.write_text(str(d))
    LOCK.write_text(f"{WHO}\n{d}\n")
    head = subprocess.run(["git", "-C", str(REPO), "rev-parse", "--short", "origin/main"],
                          capture_output=True, text=True).stdout.strip()
    pkg = adb("shell", "dumpsys", "package", PACKAGE, out=True).decode()
    apk = re.search(r"lastUpdateTime=(.+)", pkg)
    save({"started": time.strftime("%F %T"), "origin_main": head, "app": app_head(),
          "apk_installed": apk.group(1).strip() if apk else "没装", "items": {}, "tests": []})
    print(d)


def app_head():
    for p in (REPO.parent / "mooring/target/bundle-here/Fibo.app/Contents/Resources/receipt.json",
              REPO.parent / "mooring/target/bundle-here/Mooring.app/Contents/Resources/receipt.json",
              REPO.parent / "mooring/target/bundle-here/receipt.json"):
        if p.exists():
            return json.loads(p.read_text()).get("git_head", "?")[:7]
    return "?"


def setup():
    s = state()
    d = Path(tempfile.mkdtemp(prefix=f"{MARKER}-"))
    (d / "notes.txt").write_text("phone-walk test file\nline two\n")
    subprocess.run(["git", "init", "-q", str(d)], check=True)
    subprocess.run(["git", "-C", str(d), "add", "."], check=True)
    subprocess.run(["git", "-C", str(d), "-c", "user.name=t", "-c", "user.email=t@t",
                    "commit", "-qm", "init"], check=True)
    task = (f"This is a Mooring {MARKER} test session. Run `echo {MARKER}-ready` and reply "
            f"with only {MARKER}-ready. Later messages come from the phone test; keep replies short.")
    r = subprocess.run([fibo(), "run", "claude", task, "--cwd", str(d), "--no-report", "--json"],
                       capture_output=True, text=True)
    if r.returncode:
        # A new temp dir always asks whether to trust it; answer for this dir only.
        stuck = re.search(r"split-\d+-\d+", r.stderr + r.stdout)
        if not stuck:
            die(f"开测试会话失败：{r.stderr or r.stdout}")
        terminal = stuck.group(0)
        s["tests"].append({"dir": str(d), "terminal": terminal})
        save(s)
        screen = subprocess.run([fibo(), "read", terminal, "--lines", "20"],
                                capture_output=True, text=True).stdout
        if "trust this folder" not in screen or str(d.name) not in screen:
            die(f"测试会话停在别的画面上：fibo read {terminal}")
        send_keys(terminal, ["down", "enter"])
        time.sleep(4)
        subprocess.run([fibo(), "prompt", terminal, task, "--wait", "--timeout", "120"], check=True)
    else:
        terminal = json.loads(r.stdout.strip().splitlines()[-1]).get("terminal")
        s["tests"].append({"dir": str(d), "terminal": terminal})
        save(s)
    print(json.dumps({"dir": str(d), "terminal": terminal}, ensure_ascii=False))


def send_keys(terminal, keys):
    lines = [{"id": 1, "op": "hello", "version": 1, "client": MARKER},
             {"id": 2, "op": "keys", "terminal": terminal, "keys": keys}]
    body = "".join(json.dumps(l) + "\n" for l in lines)
    r = subprocess.run([fibo(), "remote", "--stdio", "--role", "full"], input=body,
                       capture_output=True, text=True, timeout=20)
    if '"id":2' not in r.stdout.replace(" ", "") or "error" in r.stdout:
        die(f"按键没送到：{r.stdout[-300:]}{r.stderr[-300:]}")


def open_chat(terminal=None):
    s = state()
    terminal = terminal or (s["tests"][-1]["terminal"] if s.get("tests") else die("先 setup"))
    if terminal not in [t["terminal"] for t in s.get("tests", [])]:
        die(f"{terminal} 不是这次开的测试终端")
    machine = (RUNTIME / "machine-id").read_text().strip()
    url = f"mooring:///chat?machine={machine}&terminal={terminal}"
    adb("shell", "am", "start", "-a", "android.intent.action.VIEW", "-d", f"'{url}'", PACKAGE)


def nodes():
    xml = adb("exec-out", "uiautomator", "dump", "/dev/tty", out=True).decode("utf-8", "replace")
    out = []
    for m in re.finditer(r'<node [^>]*?text="([^"]*)"[^>]*?content-desc="([^"]*)"[^>]*?bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"', xml):
        text = html.unescape(m.group(1) or m.group(2))
        if text:
            x1, y1, x2, y2 = map(int, m.groups()[2:])
            out.append((text, (x1 + x2) // 2, (y1 + y2) // 2))
    return xml, out


def in_front():
    """Mooring is the app on screen. Another one there (a picker, the gallery) is tapped only
    where the walk says so; a tap meant for Mooring must not land in the person's own apps."""
    focus = adb("shell", "dumpsys", "window", out=True).decode()
    line = next((l for l in focus.splitlines() if "mCurrentFocus" in l), "")
    return PACKAGE in line, line.strip()


def tap(text, contains=False):
    ours, line = in_front()
    if not ours and "--any-app" not in sys.argv:
        die(f"前台不是 Mooring（{line}），不点「{text}」。系统选图之类要点别的 App 时加 --any-app")
    if any(w in text for w in NEVER):
        die(f"「{text}」在永远不按的名单里")
    xml, found = nodes()
    hits = []
    for n in found:
        # A row and its label report the same text on the same line: one element.
        if (text in n[0] if contains else n[0] == text) and not any(
                abs(n[2] - h[2]) < 90 for h in hits):
            hits.append(n)
    if len(hits) != 1:
        die(f"「{text}」找到 {len(hits)} 个：{[h[0][:30] for h in hits]}")
    if any(w in hits[0][0] for w in GUARDED) and MARKER not in xml:
        die(f"屏幕上没有 {MARKER}，不按「{hits[0][0]}」：这里可能不是测试会话")
    adb("shell", "input", "tap", str(hits[0][1]), str(hits[0][2]))
    time.sleep(1.5)


def shot(item, note=""):
    ids = [c["id"] for c in checklist()]
    if item not in ids:
        die(f"{item} 不在清单里：{ids}")
    s = state()
    shots = s["items"].setdefault(item, {}).setdefault("shots", [])
    name = f"{item}-{len(shots) + 1}"
    (run_dir() / f"{name}.png").write_bytes(adb("exec-out", "screencap", "-p", out=True))
    xml, _ = nodes()
    (run_dir() / f"{name}.xml").write_text(xml)
    shots.append({"file": f"{name}.png", "note": note, "at": time.strftime("%T")})
    save(s)
    print(run_dir() / f"{name}.png")


def mark(item, verdict, note):
    if verdict not in ("过", "不过", "跳过"):
        die("结果只能是 过 / 不过 / 跳过")
    s = state()
    entry = s["items"].setdefault(item, {})
    if verdict != "跳过" and not entry.get("shots"):
        die(f"{item} 没有截图，不能记「{verdict}」；先 shot")
    entry.update(verdict=verdict, note=note)
    save(s)


def push_image():
    try:
        from PIL import Image, ImageDraw, ImageFont
    except ImportError:
        die("要 Pillow 才能画测试图")
    img = Image.new("RGB", (800, 400), "#2b6cb0")
    try:
        font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 96)
    except OSError:
        font = ImageFont.load_default()
    ImageDraw.Draw(img).text((60, 140), "WALK 4217", fill="white", font=font)
    p = run_dir() / "test-image.png"
    img.save(p)
    target = f"/sdcard/Pictures/phone-walk-{int(time.time())}.png"
    adb("push", str(p), target)
    adb("shell", "am", "broadcast", "-a", "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
        "-d", f"file://{target}")
    s = state()
    s.setdefault("pushed", []).append(target)
    s["pushed_at"] = time.strftime("%H:%M")
    save(s)
    print(f"{target}（图上写着 WALK 4217）")


def pick_image():
    """In the system picker, select the test picture by the minute it was pushed: the first cell
    is not always it, and a wrong one is someone's own photo (2026-11-09)."""
    at = state().get("pushed_at")
    if not at:
        die("先 push-image")
    h, m = at.split(":")
    stamp = f"{int(h)}:{m}"
    _, found = nodes()
    hits = [n for n in found if n[0].startswith("照片拍摄于") and n[0].endswith(stamp) and "动态" not in n[0]]
    if len(hits) != 1:
        die(f"相册里拍摄于 {stamp} 的照片有 {len(hits)} 张，不点：可能测试图还没进相册，退出选图再开一次")
    adb("shell", "input", "tap", str(hits[0][1]), str(hits[0][2]))
    time.sleep(1.5)
    print(f"选了拍摄于 {stamp} 的那张")


def report():
    s, d = state(), run_dir()
    rows, missing, shots = [], [], []
    for c in checklist():
        e = s["items"].get(c["id"], {})
        verdict = e.get("verdict", "没验")
        if verdict == "没验":
            missing.append(c["id"])
        pics = ""
        for x in e.get("shots", []):
            pics += (f'<button class="shot" data-i="{len(shots)}"><img src="{x["file"]}" loading="lazy" alt="">'
                     f'<small>{html.escape(x["note"])}</small></button>')
            shots.append({"file": x["file"], "shot": x["note"], "id": c["id"], "page": c["page"],
                          "verdict": verdict, "note": e.get("note", "")})
        rows.append(f'<tr class="v-{verdict}"><td><code>{c["id"]}</code><br>{html.escape(c["page"])}</td>'
                    f'<td>{html.escape(c["look"])}</td><td><b>{verdict}</b><br>{html.escape(e.get("note", ""))}</td>'
                    f'<td class="pics">{pics}</td></tr>')
    count = {v: sum(1 for c in checklist() if s["items"].get(c["id"], {}).get("verdict", "没验") == v)
             for v in ("过", "不过", "跳过", "没验")}
    summary = " · ".join(f"{k} {v}" for k, v in count.items())
    head = (f'<h1>手机走查 {html.escape(s["started"])}</h1>'
            f'<p>电脑 App {s["app"]} · origin/main {s["origin_main"]} · 手机包安装于 {html.escape(s["apk_installed"])}</p>'
            f'<p><b>{summary}</b> <button id="all">从第一张开始看 {len(shots)} 张图</button></p>')
    data = json.dumps(shots, ensure_ascii=False).replace("</", "<\\/")
    (d / "index.html").write_text(REPORT_HEAD + head + f'<table>{"".join(rows)}</table>'
                                  + REPORT_TAIL.replace("__SHOTS__", data))
    print(d / "index.html")
    print(summary)
    if missing:
        print("没验：" + "、".join(missing))


# The report page: a table per item, and a viewer that steps through every screenshot in
# checklist order (← → keys, buttons or swipe; Esc closes) instead of opening each one.
REPORT_HEAD = """<!doctype html><html lang="zh"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>手机走查</title>
<style>
:root{--bg:#fff;--fg:#1a1a1a;--muted:#666;--line:#ddd;--ok:#e6f4ea;--bad:#fde8e8;--skip:#f3f3f3;--miss:#fff4d6}
@media (prefers-color-scheme:dark){:root{--bg:#181a1f;--fg:#e6e6e6;--muted:#999;--line:#333;--ok:#1d3326;--bad:#3a1f1f;--skip:#26282d;--miss:#3a321b}}
body{background:var(--bg);color:var(--fg);font:14px/1.5 -apple-system,sans-serif;margin:0;padding:16px}
table{border-collapse:collapse;width:100%}td{border-bottom:1px solid var(--line);padding:8px;vertical-align:top}
.v-过{background:var(--ok)}.v-不过{background:var(--bad)}.v-跳过{background:var(--skip)}.v-没验{background:var(--miss)}
button{font:inherit;color:inherit;cursor:pointer}
#all{margin-left:12px;padding:4px 12px;border:1px solid var(--line);border-radius:8px;background:var(--bg)}
.shot{display:inline-block;vertical-align:top;width:124px;margin:0 6px 6px 0;padding:0;border:0;background:none;text-align:left}
.shot img{height:220px;max-width:124px;object-fit:cover;border:1px solid var(--line);display:block}
.shot small{display:block;color:var(--muted)}
#lb{position:fixed;inset:0;background:#0e0f12;display:none;flex-direction:column;align-items:center;z-index:9;color:#eee}
#lb.on{display:flex}
#lb figure{flex:1;min-height:0;margin:0;display:flex;align-items:center;justify-content:center;width:100%}
#lb img{max-height:100%;max-width:calc(100% - 140px);object-fit:contain}
#lb .cap{padding:10px 16px 16px;text-align:center;max-width:900px}
#lb .cap b{font-size:16px}#lb .cap span{color:#aaa}
#lb .nav{position:absolute;top:50%;transform:translateY(-50%);width:56px;height:96px;border:0;border-radius:10px;background:rgba(255,255,255,.12);font-size:32px;color:#fff}
#lb .nav:disabled{opacity:.2;cursor:default}#prev{left:12px}#next{right:12px}
#close{position:absolute;top:10px;right:14px;border:0;background:none;color:#fff;font-size:28px}
#count{padding-top:10px;color:#aaa}
@media (max-width:600px){#lb img{max-width:100%}#lb .nav{width:40px;height:72px;font-size:24px}}
</style>
"""
REPORT_TAIL = """<div id="lb" role="dialog" aria-label="截图">
<div id="count"></div><button id="close" aria-label="关闭">×</button>
<figure><img id="lbimg" alt=""></figure>
<button class="nav" id="prev" aria-label="上一张">‹</button><button class="nav" id="next" aria-label="下一张">›</button>
<div class="cap"><b id="t1"></b><br><span id="t2"></span></div></div>
<script>
const shots = __SHOTS__;
const lb = document.getElementById('lb'), img = document.getElementById('lbimg');
let at = 0;
function show(i) {
  at = Math.max(0, Math.min(shots.length - 1, i));
  const s = shots[at];
  img.src = s.file;
  document.getElementById('count').textContent = (at + 1) + ' / ' + shots.length;
  document.getElementById('t1').textContent = s.id + ' · ' + s.page + ' · ' + s.verdict + ' — ' + s.shot;
  document.getElementById('t2').textContent = s.note;
  document.getElementById('prev').disabled = at === 0;
  document.getElementById('next').disabled = at === shots.length - 1;
  lb.classList.add('on');
}
const close = () => lb.classList.remove('on');
document.querySelectorAll('.shot').forEach(b => b.onclick = () => show(+b.dataset.i));
document.getElementById('all').onclick = () => show(0);
document.getElementById('prev').onclick = e => { e.stopPropagation(); show(at - 1); };
document.getElementById('next').onclick = e => { e.stopPropagation(); show(at + 1); };
document.getElementById('close').onclick = close;
lb.onclick = e => { if (e.target === lb || e.target.tagName === 'FIGURE') close(); };
document.addEventListener('keydown', e => {
  if (!lb.classList.contains('on')) return;
  if (e.key === 'ArrowRight' || e.key === ' ') { e.preventDefault(); show(at + 1); }
  else if (e.key === 'ArrowLeft') show(at - 1);
  else if (e.key === 'Escape') close();
});
let x0 = null;
lb.addEventListener('touchstart', e => { x0 = e.touches[0].clientX; }, { passive: true });
lb.addEventListener('touchend', e => {
  if (x0 === null) return;
  const dx = e.changedTouches[0].clientX - x0; x0 = null;
  if (Math.abs(dx) > 50) show(at + (dx < 0 ? 1 : -1));
});
</script></html>
"""


def teardown():
    s = state()
    dirs = [os.path.realpath(t["dir"]) for t in s.get("tests", [])]
    # Terminals the phone opened (new.task) are not in the state: close every one in a test dir.
    listed = json.loads(subprocess.run([fibo(), "list", "--json"], capture_output=True,
                                       text=True).stdout or "[]")
    listed = listed if isinstance(listed, list) else listed.get("terminals", [])
    closing = {t["terminal"] for t in s.get("tests", [])} | {
        t["id"] for t in listed
        if any(os.path.realpath(t.get("cwd") or "/").startswith(d) for d in dirs)}
    for terminal in closing:
        subprocess.run([fibo(), "close", terminal], capture_output=True)
    for d in dirs:
        shutil.rmtree(d, ignore_errors=True)
        slug = re.sub(r"[^A-Za-z0-9]", "-", d)
        shutil.rmtree(Path.home() / ".claude/projects" / slug, ignore_errors=True)
    # Plan mode writes the plan under ~/.claude/plans, named after the first words of the task.
    for plan in (Path.home() / ".claude/plans").glob(f"{MARKER}-*.md"):
        plan.unlink()
    for p in s.get("pushed", []):
        adb("shell", "rm", "-f", p)
        # A scan of the missing file drops it from the gallery too.
        adb("shell", "am", "broadcast", "-a", "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
            "-d", f"file://{p}")
    if LOCK.exists() and LOCK.read_text().startswith(f"{WHO}\n"):
        LOCK.unlink()
    print(f"关了 {len(closing)} 个测试终端，删了 {len(dirs)} 个测试目录和 {len(s.get('pushed', []))} 张测试图")


def main(argv):
    if not argv:
        sys.exit(__doc__)
    cmd, rest = argv[0], argv[1:]
    if cmd == "start":
        start()
    elif cmd == "setup":
        setup()
    elif cmd == "open-chat":
        open_chat(*rest[:1])
    elif cmd == "shot":
        shot(rest[0], " ".join(rest[1:]))
    elif cmd == "tap":
        tap(rest[0], "--contains" in rest)
    elif cmd == "type":
        # The phone's shell parses the line again: quote it, and `input text` wants %s for spaces.
        text = rest[0].replace(" ", "%s").replace("'", "'\\''")
        adb("shell", "input", "text", f"'{text}'")
    elif cmd == "key":
        if rest[0] == "hide":
            # Back only while the keyboard is up: with it down, Back leaves the page, and twice
            # it left the chat for other screens mid-walk (2026-11-09).
            shown = adb("shell", "dumpsys", "input_method", out=True).decode()
            if "mInputShown=true" in shown:
                adb("shell", "input", "keyevent", "4")
        else:
            adb("shell", "input", "keyevent", {"back": "4", "enter": "66", "home": "3"}[rest[0]])
    elif cmd == "texts":
        for t, x, y in nodes()[1]:
            print(f"{x:5} {y:5}  {t[:60]}")
    elif cmd == "push-image":
        push_image()
    elif cmd == "pick-image":
        pick_image()
    elif cmd == "mark":
        mark(rest[0], rest[1], " ".join(rest[2:]))
    elif cmd == "report":
        report()
    elif cmd == "teardown":
        teardown()
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
