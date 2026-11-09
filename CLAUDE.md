# CLAUDE.md

Fibo (https://fibo.page): a native macOS workbench for coding agents (Rust, GPUI, libghostty-vt). It was called Mooring, and before that Keepline; see "Compatibility" below. The crates, modules, the program file `mooring` and the `MOORING_*` variables keep the old name on purpose; what users see says Fibo.

## Layout

- `crates/mooring-app`: the GPUI window.
- `crates/mooring-host`: the background host (`mooring serve`) and the `mooring` command Agents use inside its terminals (`mooring --skill`, `mooring --skill plugins`).
- `crates/mooring-core`: model, protocol, status rules and plugin formats shared by both.
- `crates/mooring-remote`, `crates/mooring-relay`: the phone link and its relay. `mobile/`: the phone app (Expo).
- `plugins/`: example plugins. Don't commit `plugins/index.json` or `index.sig` by hand: after a push, `.github/workflows/sign-catalog.yml` regenerates and signs them. `eval/`: terminal rendering and protocol checks.

## Build and check

- `./bootstrap.sh` once (pinned GPUI and libghostty, needs Zig 0.16), then `./run.sh`.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `sh ci-test.sh` (retries the wall-clock `managed_session` cases once).
- New worktrees: symlink `.deps` from the main checkout; set `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`.
- Fixing something you can see: `python3 scripts/visual-check.py shots <sha> <out>` renders the commit and its parent on the same fixture data into `compare.html`; the old build must show the bug and the new one must not, or it isn't reported as fixed. `visual-check.py guard <sha> <test> --undo <file> <old> <new>` must report that the test fails with the fix undone.
- Phone: tests, fake agents and the web build miss a whole class of bugs. After changing what the phone shows or does, and right after swapping the App or updating a plugin, walk the whole checklist in `docs/phone-app/walkthrough.md` on the real phone (the `phone-walk` skill, `scripts/phone-walk.py`), not just the part you changed. Report its 过 / 不过 / 跳过 / 没验 counts; items not walked go under 没验 (`没验：真手机` when there is no phone). Every bug only this caught is in `docs/phone-app/real-device-findings.md`; add a row for each new one.
- Window performance: rules, how to measure and the CI gate (`scripts/perf-gate.py`) are in `docs/performance/index.html`; read it before touching pane observers, the sidebar view or per-frame work.
- The commit message is the change record. A commit that changes more than a few lines ends its body with one line each (only what code and diff do not already say): `Why:` the motivation, in the user's words when they gave them; `Changed:` the approach the diff does not show at a glance; `Rejected:` what else was considered and why not (`none: <reason>` is fine); and when they apply `Impact:`, `Guarded-by:` the test that fails if this is undone (add such a test rather than more prose when one can hold it), `Supersedes: <sha>`. The commit-msg hook warns when `Why:` / `Rejected:` are missing.
- A decision that outlasts one change (spans several places, was reversed once, or the same option was rejected twice) also gets a record in `docs/decisions/` (see its README). The first time a Claude session reads or edits a file, its recent `Rejected:` / `Guarded-by:` lines and the decisions naming it are shown (a hook); however a file was edited, an agent commit is stopped once when a staged file has such lines the session has not been shown: fix the change or say why in the message, then run the same commit again; `git why <file>:<line>` (or a sha) prints the commit message, the user's words that asked for it and the agent reply they answered. Agent commits carry `Agent-Session` / `Agent-Prompt` trailers from a global hook (`~/.config/agent-trace`, this machine only); keep them when rewriting commits.
- Before changing code, read `docs/lessons.md`: the mistakes made more than once, with the commits that show them. A rule seen on three days moves here.
- Changing a state rule, an interface or a file format: try it with the old one still there (the host just restarted, a plugin updated after the host started, a file or script an older build wrote), and say what a fix that only holds for new data does to the old. Seen on four days (`docs/lessons.md`).
- One owner per fact (docs/plan.md principle 7): a session's state, name, 要你处理, time, folder and state words come from core (`status::of`, `session_title`, `status::needs_you`, `session_updated`, `terminal_cwd`, `Status::label`). `crates/mooring-core/tests/guard.rs` counts raw-field reads and hand-written state words per file; rewrite `tests/guard-baseline.txt` (`MOORING_GUARD_UPDATE=1`) only for a use you mean to keep.
- Words the window shows go through `t!("中文")` with English in `crates/mooring-core/locales/en/*.toml`; `tests/i18n.rs` checks every key has English and that unconverted Chinese only goes down. How to convert a file: docs/decisions/2026-11-08-english-ui.md.
- File size: no source file grows past 800 lines, however it was written (`crates/mooring-core/tests/file_size.rs`). Put new code in a new file. Files in `tests/file-size-baseline.txt` can only get shorter; `MOORING_GUARD_UPDATE=1` lowers their numbers and never raises one.

## Reaching main

Local `main` only follows `origin/main`: never commit or merge on it. It drifted to 18 ahead and 128 behind when sessions merged into it locally and each left pushing to the next, while PRs landed on GitHub. A `reference-transaction` hook refuses anything else (`sh scripts/install-hooks.sh` once per clone; `MOORING_ALLOW_LOCAL_MAIN=1` for a deliberate exception).

- Work in a worktree on a branch. Verify, then `git fetch origin && git rebase origin/main`, push the branch, and `gh pr create --fill && gh pr merge --auto --merge`: it merges itself once CI passes, so nothing has to wait for it.
- GitHub refuses a direct push to `main` and a merge before `Workbench Rust checks` passes, for everyone including admins (ruleset `main requires CI`, docs/decisions/2026-11-06-main-requires-ci.md). When a check is red, fix the cause rather than loosening the rule.
- Follow it in the main checkout with `git pull --ff-only`. Package the App from `origin/main`.
- Push only your own commits. Nothing waits on "someone else's unpushed commits", because there are none.

## Compatibility with Mooring installs

The rename to Fibo (docs/decisions/2026-11-09-rename-fibo.md) works like the Keepline one below: `~/.config/mooring` moves to `~/.config/fibo` once, an existing `~/.local/share/mooring-workbench` stays the runtime, the command is `fibo` and `mooring` still answers (PATH links for both; in the bundle `fibo` links to the program `mooring`), `MOORING_*` stays what code reads and hooks test, with `FIBO_*` set beside it and adopted at start. The updater accepts the old bundle id `local.mooring.workbench` and the `mooring` cask.

## Compatibility with Keepline installs

`crates/mooring-core/src/legacy.rs` keeps an install from before the rename working: `KEEPLINE_*` variables are adopted at start and set beside `MOORING_*` for terminals and plugins, `~/.config/keepline` moves to `~/.config/mooring` once, an existing `~/.local/share/keepline-workbench` stays the runtime, hook entries testing `$KEEPLINE_HOOK` count as ours (outdated), and the app bundle links `keepline` and `keeplined` to `mooring`. The phone link's crypto strings and the relay path `/keepline` keep their old spelling on purpose: changing them breaks paired phones and deployed relays.
