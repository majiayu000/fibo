# CLAUDE.md

Native macOS workbench for coding agents (Rust, GPUI, libghostty-vt).

- `crates/keepline-app`: the GPUI window.
- `crates/keeplined`: the background host and the `keepline` command Agents use inside its terminals.
- `crates/keepline-core`: model, protocol and status rules shared by both.
- `plugins/`: example plugins. `mobile/`: the phone chat app (Expo).

Build: `./bootstrap.sh` once, then `./run.sh`. Checks: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `sh ci-test.sh`.
New worktrees: symlink `.deps` from the main checkout and set `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`.
