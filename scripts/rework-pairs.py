#!/usr/bin/env python3
"""Which fixes repaired a commit from the last few days.

For each fix since a date, blames the lines it changed and names the recent commit they came
from. Deterministic input for the rework review in docs/lessons.md; which pairs are the same
kind of mistake is still a reader's call.

    python3 scripts/rework-pairs.py --since 2026-11-01 [--days 5]
"""
import argparse
import collections
import re
import subprocess

# Where a line came from says nothing here: they move or rename lines, they don't write them.
NOT_A_SOURCE = re.compile(r"(?i)initial import|refactor|^chore|split|rename|改名|^style")
FIX = re.compile(r"(?i)(fix|修)")
HUNK = re.compile(r"@@ -(\d+)(?:,(\d+))? ")
CODE = ["*.rs", "*.ts", "*.tsx", "*.py", "*.sh", "*.js", "*.json", "*.yml"]
MOST_HUNKS = 40


def git(*args):
    done = subprocess.run(["git", *args], capture_output=True, text=True, errors="replace")
    return done.stdout


def removed_lines(sha):
    """{file: [(first line, count)]} in the fix's parent."""
    hunks, path = collections.defaultdict(list), None
    for line in git("diff", "-U0", "-w", f"{sha}^", sha, "--", *CODE).splitlines():
        if line.startswith("--- "):
            path = line[6:] if line.startswith("--- a/") else None
        match = HUNK.match(line)
        if match and path and int(match.group(2) or 1):
            hunks[path].append((int(match.group(1)), int(match.group(2) or 1)))
    return hunks


def sources(sha, known):
    found = collections.Counter()
    total = 0
    for path, hunks in removed_lines(sha).items():
        args = ["blame", "-w", "-s", "--abbrev=7", f"{sha}^"]
        for first, count in hunks[:MOST_HUNKS]:
            args += ["-L", f"{first},{first + count - 1}"]
        for line in git(*args, "--", path).splitlines():
            total += 1
            source = line.split()[0].lstrip("^")[:7]
            full = next((k for k in known if k.startswith(source)), None)
            if full:
                found[full] += 1
    return found, total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--since", required=True)
    parser.add_argument("--days", type=int, default=5, help="how old a repaired commit may be")
    parser.add_argument("--rev", default="origin/main")
    opts = parser.parse_args()
    log = "--format=%h|%ct|%s"
    known = {}
    for row in git("log", opts.rev, "--no-merges", log).splitlines():
        sha, at, subject = row.split("|", 2)
        known[sha] = (int(at), subject)
    fixes = git("log", opts.rev, "--no-merges", f"--since={opts.since}", log).splitlines()
    fixes = [row.split("|", 2) for row in fixes]
    fixes = [(s, int(a), t) for s, a, t in fixes if FIX.match(t)]
    pairs = []
    for sha, at, subject in fixes:
        found, total = sources(sha, known)
        recent = [
            (source, lines)
            for source, lines in found.most_common()
            if 0 < at - known[source][0] < opts.days * 86400
            and not NOT_A_SOURCE.search(known[source][1])
            and lines >= 2
        ]
        if recent:
            source, lines = recent[0]
            pairs.append((source, sha, (at - known[source][0]) / 3600, lines, total, subject))
    same_day = sum(1 for p in pairs if p[2] < 24)
    of_fix = sum(1 for p in pairs if FIX.match(known[p[0]][1]))
    print(f"{len(fixes)} fixes, {len(pairs)} repair a recent commit")
    print(f"{same_day} within 24 hours, {of_fix} repair an earlier fix")
    for source, sha, hours, lines, total, subject in sorted(pairs):
        print(
            f"{source} {known[source][1][:50]} => {sha} {hours:5.1f}h "
            f"{lines}/{total} {subject[:60]}"
        )


if __name__ == "__main__":
    main()
