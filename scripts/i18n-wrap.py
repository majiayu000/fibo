#!/usr/bin/env python3
"""Wrap a Rust file's Chinese UI strings in t!(…) and list the keys still missing English.

    python3 scripts/i18n-wrap.py <file.rs>... [--dry-run]

What it does, per file (the same literals crates/fibo-core/tests/i18n.rs counts):
- "中文"                    -> t!("中文")
- format!("共 {n} 个")       -> t!("共 {n} 个", n)
- format!("{} 个", a.len()) -> t!("{len} 个", len = a.len())
- format!("{:.1}", x)       -> the value pre-formatted: t!("{x}", x = format!("{:.1}", x))
Anything it cannot rewrite safely (a literal in a `const`, a `match` pattern, a format! it
cannot parse) is left as it is and listed as SKIP. Then it prints every t! key in the file
that has no English yet, as TOML lines with an empty value to fill in, and appends them to
nothing: copy them into a crates/fibo-core/locales/en/<area>.toml yourself.

Read the diff afterwards. A string the code compares or stores (matched against an Agent's
screen, written to a file) is data, not UI: put it back, or mark it not_ui!("…").
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
LOGS = {"eprintln!", "println!", "eprint!", "print!", "panic!", "unreachable!", "todo!",
        "expect", "debug_assert!", "assert!", "assert_eq!"}


def cjk(s):
    return any("㐀" <= c <= "䶿" or "一" <= c <= "鿿" or "　" <= c <= "〿"
               or "＀" <= c <= "￯" for c in s)


def ident(c):
    return c.isalnum() or c == "_"


def test_cut(text):
    """Offset of a `#[cfg(test)] mod x {` that runs to the end of the file."""
    for m in re.finditer(r"#\[cfg\(test\)\]\s*mod\s+\w+\s*\{", text):
        return m.start()
    return len(text)


def literals(text):
    """(start, end, value, raw) of every string literal, comments and chars skipped."""
    out = []
    i, n = 0, test_cut(text)
    s = text
    while i < n:
        c = s[i]
        if s.startswith("//", i):
            j = s.find("\n", i)
            i = n if j < 0 else j
        elif s.startswith("/*", i):
            depth = 0
            while i < n:
                if s.startswith("/*", i):
                    depth += 1
                    i += 2
                elif s.startswith("*/", i):
                    depth -= 1
                    i += 2
                    if depth == 0:
                        break
                else:
                    i += 1
        elif c == "'":
            if s[i + 1:i + 2] == "\\":
                j = s.find("'", i + 2)
                i = j + 1
            elif s[i + 2:i + 3] == "'":
                i += 3
            else:
                i += 1
        elif c == '"' or (c in "rbc" and (i == 0 or not ident(s[i - 1]))):
            j = i
            while j < n and s[j] in "rbc" and j - i < 2:
                j += 1
            raw = "r" in s[i:j]
            hashes = 0
            while raw and s[j:j + 1] == "#":
                hashes += 1
                j += 1
            if s[j:j + 1] != '"':
                while i < n and ident(s[i]):
                    i += 1
                continue
            start = i
            j += 1
            value = []
            while j < len(s):
                ch = s[j]
                if ch == '"' and s[j + 1:j + 1 + hashes] == "#" * hashes:
                    j += 1 + hashes
                    break
                if ch == "\\" and not raw:
                    nx = s[j + 1]
                    if nx == "n":
                        value.append("\n")
                    elif nx == "t":
                        value.append("\t")
                    elif nx == "u":
                        end = s.index("}", j)
                        value.append(chr(int(s[j + 3:end], 16)))
                        j = end - 1
                    elif nx == "\n":
                        j += 1
                        while s[j + 1].isspace():
                            j += 1
                        j += 1
                        continue
                    else:
                        value.append(nx)
                    j += 2
                    continue
                value.append(ch)
                j += 1
            out.append((start, j, "".join(value), raw))
            i = j
        else:
            i += 1
    return out


def call_before(s, start):
    k = start
    while k > 0 and s[k - 1].isspace():
        k -= 1
    if k == 0 or s[k - 1] != "(":
        return "", None
    paren = k - 1
    k = paren
    while k > 0 and s[k - 1].isspace():
        k -= 1
    end = k
    if k > 0 and s[k - 1] == "!":
        k -= 1
    while k > 0 and ident(s[k - 1]):
        k -= 1
    return s[k:end], k


def split_args(s, i):
    """From just after a format string, the comma-separated args up to the matching ')'.
    Returns (args, index of ')')."""
    args, depth, cur, j = [], 0, [], i
    lits = {a: b for a, b, _, _ in LITS}
    while j < len(s):
        if j in lits:
            cur.append(s[j:lits[j]])
            j = lits[j]
            continue
        c = s[j]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                if "".join(cur).strip():
                    args.append("".join(cur).strip())
                return args, j
            depth -= 1
        if c == "," and depth == 0:
            args.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
        j += 1
    return None, None


def name_for(expr, taken, i):
    e = expr.strip().lstrip("&*").strip()
    e = re.sub(r"\(\s*\)$", "", e)
    m = re.search(r"([A-Za-z_][A-Za-z0-9_]*)$", e)
    name = m.group(1) if m and not m.group(1)[0].isupper() else f"v{i}"
    if name in ("self", "s", "t", "x", "_") or len(name) == 1 and name in taken:
        name = name if name not in taken else f"v{i}"
    base, k = name, 2
    while name in taken:
        name = f"{base}{k}"
        k += 1
    taken.add(name)
    return name


def convert_format(src, value, args):
    """The t! call replacing format!(src, args), or None."""
    holes = list(re.finditer(r"\{\{|\}\}|\{([^{}]*)\}", value))
    taken = set()
    named = {}  # name -> expr or None for captured ident
    positional = [a for a in args if not re.match(r"^[A-Za-z_]\w*\s*=[^=]", a)]
    kw = {}
    for a in args:
        m = re.match(r"^([A-Za-z_]\w*)\s*=([^=].*)$", a, re.S)
        if m:
            kw[m.group(1)] = m.group(2).strip()
    pos_i = 0
    new = []
    last = 0
    for h in holes:
        new.append(value[last:h.start()])
        last = h.end()
        tok = h.group(0)
        if tok in ("{{", "}}"):
            new.append(tok)
            continue
        inner = h.group(1)
        name, _, spec = inner.partition(":")
        if name == "":
            if pos_i >= len(positional):
                return None
            expr = positional[pos_i]
            pos_i += 1
            nm = name_for(expr, taken, pos_i)
            named[nm] = format_spec(expr, spec)
        elif name.isdigit():
            return None
        else:
            expr = kw.get(name, name)
            nm = name
            if nm not in taken:
                taken.add(nm)
            if spec:
                named[nm] = format_spec(expr, spec)
            elif name in kw:
                named[nm] = expr
            else:
                named.setdefault(nm, None)
        new.append("{" + nm + "}")
    new.append(value[last:])
    if pos_i != len(positional):
        return None
    key = "".join(new)
    parts = [json.dumps(key, ensure_ascii=False)]
    for nm, expr in named.items():
        parts.append(nm if expr is None or expr == nm else f"{nm} = {expr}")
    if len(parts) == 1:
        return key, "t!(" + parts[0] + ").to_string()"
    return key, "t!(" + ", ".join(parts) + ")"


def format_spec(expr, spec):
    if not spec:
        return expr
    return 'format!("{:' + spec + '}", ' + expr + ")"


def english(key):
    """Whether any locales/en table has this key (read as the tables write it)."""
    return key in TABLE


def load_table():
    table = {}
    d = os.path.join(ROOT, "crates/fibo-core/locales/en")
    for f in sorted(os.listdir(d)) if os.path.isdir(d) else []:
        if not f.endswith(".toml"):
            continue
        for line in open(os.path.join(d, f), encoding="utf-8"):
            line = line.strip()
            if not line.startswith('"'):
                continue
            m = re.match(r'^("(?:[^"\\]|\\.)*")\s*=', line)
            if m:
                table[json.loads(m.group(1))] = f
    return table


def lit_value_as_key(value):
    return json.dumps(value, ensure_ascii=False)


TABLE = {}
LITS = []


def process(path, dry):
    global LITS
    text = open(path, encoding="utf-8").read()
    LITS = literals(text)
    edits = []  # (start, end, replacement)
    skips = []
    keys = []
    line_of = lambda off: text.count("\n", 0, off) + 1
    for start, end, value, raw in LITS:
        if not cjk(value):
            continue
        call, call_start = call_before(text, start)
        if call in ("t!", "tk!", "not_ui!") or call in LOGS:
            if call in ("t!", "tk!"):
                keys.append(value)
            continue
        line = text[text.rfind("\n", 0, start) + 1:text.find("\n", start)]
        if re.match(r"^\s*(pub(\([^)]*\))?\s+)?(const|static)\s", line):
            skips.append((line_of(start), value, "const/static"))
            continue
        after = text[end:text.find("\n", end) if text.find("\n", end) >= 0 else len(text)]
        if re.match(r"^\s*(\|[^=]*)?=>", after):
            skips.append((line_of(start), value, "match pattern"))
            continue
        if call == "format!":
            # The literal must be format!'s first argument.
            args, close = split_args(text, end)
            if args is None:
                skips.append((line_of(start), value, "format! not parsed"))
                continue
            if args and args[0] == "":
                args = args[1:]
            got = convert_format(text[start:end], value, args)
            if got is None:
                skips.append((line_of(start), value, "format! with indexed or extra args"))
                continue
            key, repl = got
            edits.append((call_start, close + 1, repl))
            keys.append(key)
            continue
        if raw:
            skips.append((line_of(start), value, "raw string"))
            continue
        edits.append((start, end, "t!(" + text[start:end] + ")"))
        keys.append(value)
    # Apply from the end; drop edits nested in an earlier (outer) edit.
    edits.sort()
    kept = []
    for e in edits:
        if kept and e[0] < kept[-1][1]:
            # nested in previous: the outer format! edit already rewrote its text range
            if e[1] <= kept[-1][1]:
                continue
        kept.append(e)
    out = text
    for s, e, r in reversed(kept):
        out = out[:s] + r + out[e:]
    if not dry and out != text:
        open(path, "w", encoding="utf-8").write(out)
    rel = os.path.relpath(path, ROOT)
    print(f"== {rel}: {len(kept)} wrapped, {len(skips)} skipped")
    for line, value, why in skips:
        print(f"SKIP {rel}:{line}: {why}: {value!r}")
    missing = []
    for k in keys:
        if k not in TABLE and k not in missing:
            missing.append(k)
    for k in missing:
        print(f"{lit_value_as_key(k)} = \"\"")


def main():
    global TABLE
    dry = "--dry-run" in sys.argv
    TABLE = load_table()
    for p in [a for a in sys.argv[1:] if not a.startswith("--")]:
        process(os.path.abspath(p), dry)
    if not dry:
        subprocess.run(["rustfmt", "--edition", "2024", *[a for a in sys.argv[1:] if not a.startswith("--")]])


if __name__ == "__main__":
    main()
