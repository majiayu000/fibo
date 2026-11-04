# AGENTS.md

Mooring: a native macOS workbench for coding agents (Rust, GPUI, libghostty-vt). It was called Keepline; see "Compatibility" below.

## Layout

- `crates/mooring-app`: the GPUI window.
- `crates/mooring-host`: the background host (`mooring serve`) and the `mooring` command Agents use inside its terminals (`mooring --skill`, `mooring --skill plugins`).
- `crates/mooring-core`: model, protocol, status rules and plugin formats shared by both.
- `crates/mooring-remote`, `crates/mooring-relay`: the phone link and its relay. `mobile/`: the phone app (Expo).
- `plugins/`: example plugins. `eval/`: terminal rendering and protocol checks.

## Build and check

- `./bootstrap.sh` once (pinned GPUI and libghostty, needs Zig 0.16), then `./run.sh`.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `sh ci-test.sh` (retries the wall-clock `managed_session` cases once).
- New worktrees: symlink `.deps` from the main checkout; set `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`.
- Window performance: rules, how to measure and the CI gate (`scripts/perf-gate.py`) are in `docs/performance/index.html`; read it before touching pane observers, the sidebar view or per-frame work.
- Why code is the way it is: `git why <file>:<line>` (or a sha) prints the commit message, then the user's words that asked for it and the agent reply they answered. Agent commits carry `Agent-Session` / `Agent-Prompt` trailers from a global git hook (`~/.config/agent-trace`, this machine only); keep them when rewriting commits. Settled decisions belong in the commit message body, not only in the conversation.
- One owner per fact (docs/plan.md principle 7): a session's state, name, 要你处理, time, folder and state words come from core (`status::of`, `session_title`, `status::needs_you`, `session_updated`, `terminal_cwd`, `Status::label`). `crates/mooring-core/tests/guard.rs` counts raw-field reads and hand-written state words per file; rewrite `tests/guard-baseline.txt` (`MOORING_GUARD_UPDATE=1`) only for a use you mean to keep.

## Compatibility with Keepline installs

`crates/mooring-core/src/legacy.rs` keeps an install from before the rename working: `KEEPLINE_*` variables are adopted at start and set beside `MOORING_*` for terminals and plugins, `~/.config/keepline` moves to `~/.config/mooring` once, an existing `~/.local/share/keepline-workbench` stays the runtime, Claude's status-line readings are read from `~/.keepline` while `~/.mooring` does not exist, hook entries testing `$KEEPLINE_HOOK` count as ours (outdated), and the app bundle links `keepline` and `keeplined` to `mooring`. The phone link's crypto strings and the relay path `/keepline` keep their old spelling on purpose: changing them breaks paired phones and deployed relays.
