# A CLI that draws what Claude Code 2.1.319 and Codex 0.167 drew for /model and /permissions, and
# writes each key it acts on to a file. For tests/remote_modes.rs.
import os, sys, termios, tty

which, log = sys.argv[1], sys.argv[2]
flags = sys.argv[3:]
has = lambda f: f in flags
draft = next((f.split("=", 1)[1] for f in flags if f.startswith("--draft=")), "")

CLAUDE_MODELS = [("Default (recommended)", "Opus 5.5 · Best for everyday, complex tasks"), ("Opus 5.5", "For complex work and everyday tasks"), ("Sonnet 5.5", "Most efficient for simpler tasks"), ("More models", "Show 8 more")]
CODEX_MODELS = [("GPT-6.1-Sol", "Latest workhorse model"), ("GPT-6-Astra", "Frontier intelligence"), ("GPT-6-Luna", "Fast and affordable"), ("GPT-5.6-Luna", "Older fast model")]
PERMS = [("Ask for approval", "Read and edit workspace files"), ("Approve for me", "Only ask for actions detected as potentially unsafe"), ("Full Access", "Use with caution"), ("Read Only", "Read workspace files")]
EFFORT = [("Low", "Fast"), ("Medium (default)", "Balances"), ("High", "Deep")]

cycle = ["manual mode on", "accept edits on", "plan mode on"]
if has("--allow-dangerously-skip-permissions") or has("--bypass-unseen"):
    cycle.append("bypass permissions on")
if not has("--no-auto"):
    cycle.append("auto mode on")
mode = cycle.index("plan mode on") if has("--bypass-unseen") else 0

model = "Opus 5.5" if which == "claude" else "GPT-6.1-Sol"
effort = "default"
perm = 0
said = []
box = draft
menu = None  # (title, rows, pointer, hint, what)
window = 2


def note(k):
    with open(log, "a") as f:
        f.write(k + "\n")


def row(i, rows, pointer, current):
    label, detail = rows[i]
    mark = {"claude": " ✔", "codex": " (current)"}[which] if current else ""
    point = {"claude": "❯", "codex": "›"}[which] if i == pointer else " "
    indent = "  " if which == "claude" else ""
    return f"{indent}{point} {i + 1}. {label + mark:<28}{detail}"


def draw():
    out = ["\033[2J\033[H"]
    add = lambda s="": out.append(s + "\r\n")
    add(" Fake CLI")
    add()
    for s in said:
        add(s)
    if has("--busy"):
        add("✻ Working (3s · esc to interrupt)")
    if has("--dialog"):
        add(" Bash command")
        add("   rm -rf x")
        add(" Do you want to proceed?")
        add(" ❯ 1. Yes")
        add("   2. No")
        add(" Esc to cancel · Tab to amend")
    elif menu:
        title, rows, pointer, hint, what = menu
        add("  " + title)
        add("  Your next response will be slower" if what.startswith("switch") else "")
        top = 0
        shown = list(range(len(rows)))
        scrolls = which == "codex" and what == "model"
        if scrolls:
            top = max(0, pointer - window)
            shown = shown[top : top + window + 1]
        for i in shown:
            current = (what == "model" and rows[i][0] == model) or (what == "perm" and i == perm)
            add(row(i, rows, pointer, current))
        if scrolls and shown[-1] + 1 < len(rows):
            add("↓")
        add()
        if hint:
            add("  " + hint)
    else:
        add("────────")
        if which == "claude":
            add("❯ " + box if box else '❯ Try "how do I log an error?"')
            add("────────")
            add("  " + cycle[mode])
        else:
            add("› " + box if box else "› Ask Codex to do anything")
            add()
            add(f"  {model} {effort} · /tmp/cli")
            add("  ← for agents · ? for shortcuts")
    os.write(1, "".join(out).encode())


def open_menu(what):
    global menu
    if which == "claude":
        rows = CLAUDE_MODELS
        pointer = [r[0] for r in rows].index(model)
        hint = "Enter to set as default · s to use this session only · Esc to cancel"
        menu = ("Select model", rows, pointer, hint, "model")
    elif what == "model":
        pointer = [r[0] for r in CODEX_MODELS].index(model)
        menu = ("Select Model and Effort", CODEX_MODELS, pointer, "enter select · esc back", "model")
    else:
        menu = ("Update Model Permissions", PERMS, perm, "enter select · esc back", "perm")


def enter():
    global box, menu, perm, model, effort
    if not menu:
        if box == "/model":
            box = ""
            open_menu("model")
        elif box == "/permissions" and which == "codex":
            box = ""
            open_menu("perm")
        elif box:
            note("SENT " + box)
            box = ""
        return
    title, rows, pointer, hint, what = menu
    if what.startswith("switch"):
        note("SWITCH-YES")
        model = what.split(":", 1)[1]
        said.append(f"  ⎿  Set model to {model} for this session only")
        menu = None
    elif what == "perm":
        choice = rows[pointer][0]
        if choice == "Full Access":
            if has("--other-confirm"):
                menu = ("Enable full access?", [("Yes, and also something else", ""), ("Cancel", "")], 0, "enter select · esc back", "confirm")
            else:
                menu = ("Enable full access?", [("Yes, continue anyway", "Apply full access for this session"), ("Cancel", "Go back")], 0, "enter select · esc back", "confirm")
            return
        if not has("--stuck"):
            perm = pointer
        menu = None
    elif what == "confirm":
        if rows[pointer][0] == "Yes, continue anyway":
            note("FULL-YES")
            perm = 2
        else:
            note("OTHER-YES")
            perm = 2
        menu = None
    elif what == "model" and which == "codex":
        menu = ("Select Reasoning Level for " + rows[pointer][0], EFFORT, 1, "enter default · s session · esc back", "effort:" + rows[pointer][0])
    elif what.startswith("effort"):
        note("PICK-DEFAULT")
        menu = None
    else:
        note("PICK-DEFAULT")
        menu = None


def session_pick():
    global menu, model, effort
    title, rows, pointer, hint, what = menu
    if what == "model" and which == "claude":
        note("PICK-SESSION")
        if has("--cached"):
            menu = ("Switch model?", [("Yes, switch to " + rows[pointer][0], ""), ("No, go back", "")], 0, "", "switch:" + rows[pointer][0])
            return
        model = rows[pointer][0]
        said.append(f"  ⎿  Set model to {model} for this session only")
        menu = None
    elif what.startswith("effort"):
        note("PICK-SESSION")
        model = what.split(":", 1)[1]
        effort = rows[pointer][0].split(" ")[0].lower()
        menu = None


def key(k):
    global menu, box, mode
    if k == b"\x1b[Z":
        note("SHIFT-TAB")
        mode = (mode + 1) % len(cycle)
    elif k in (b"\x1b[A", b"\x1bOA") and menu:
        t, r, p, h, w = menu
        menu = (t, r, (p - 1) % len(r), h, w)
        note("UP")
    elif k in (b"\x1b[B", b"\x1bOB") and menu:
        t, r, p, h, w = menu
        menu = (t, r, (p + 1) % len(r), h, w)
        note("DOWN")
    elif k == b"\x1b":
        note("ESC")
        if menu and menu[4].startswith("effort"):
            open_menu("model")
        else:
            menu = None
    elif k == b"\r":
        note("ENTER")
        enter()
    elif k == b"\x7f":
        box = box[:-1]
    elif menu:
        if k == b"s":
            session_pick()
    else:
        box += k.decode(errors="ignore").replace("\x1b[200~", "").replace("\x1b[201~", "")


fd = 0
old = termios.tcgetattr(fd)
tty.setraw(fd)
os.write(1, b"\x1b[?2004h")
draw()
try:
    while True:
        chunk = os.read(fd, 1024)
        if not chunk:
            break
        i = 0
        while i < len(chunk):
            if chunk.startswith(b"\x1b[200~", i):
                end = chunk.find(b"\x1b[201~", i)
                end = len(chunk) if end < 0 else end
                box += chunk[i + 6 : end].decode(errors="ignore")
                i = end + 6
            elif chunk[i] == 0x1b and i + 2 < len(chunk) and chunk[i + 1] in b"[O":
                key(chunk[i : i + 3])
                i += 3
            else:
                key(chunk[i : i + 1])
                i += 1
        draw()
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, old)
