# Customizing Fibo: Plugins and Appearance

For coding agents: follow this guide when the user asks you to add a command to Fibo, customize the sidebar layout, display pane decorations, send event notifications, change completion animations, or adjust status colors. Everything is plain files — no recompiling Fibo.

## Where files live

The config directory is `${XDG_CONFIG_HOME:-$HOME/.config}/fibo`, written `<config>` below.

- Plugins: `<config>/plugins/<id>/`, one directory per plugin.
- Status colors: configured in themes (see "Themes" in `fibo --skill`).
- Shortcuts, terminal fonts, completion animation choice, and pinned prompts belong in the app's **Settings** — do not hand-edit their state files.

## Rules

- **One idea per plugin**: create a new directory instead of stuffing unrelated features into an existing plugin.
- Never modify the user's existing plugins or `plugins/disabled.json` unless explicitly asked.
- Always run `fibo plugins` afterward — every item must show `✓`; fix any `✗` and re-run.
- Tell the user to click **Reload** in **Settings › Plugins** so changes take effect immediately (including `agent` plugins, no restart needed).
- Check existing plugins first: `fibo plugins available` lists bundled plugins (`--remote` checks the remote catalog), and `fibo plugins install <id>` installs one and reloads a running Fibo automatically.
- Plugins can only use the capabilities listed below. If something is unsupported (custom UI widgets, overriding session state, reading raw transcripts), say so plainly and never modify Fibo's app bundle.

## Publishing a catalog

A plugin catalog is `index.json` plus plugin folders hosted over `https://` or `file://` and listed in `<config>/market`.

- `fibo plugins keygen --name <publisher>` creates a signing key (`<config>/publisher.key`, owner-readable only — keep out of git). `fibo plugins index <dir>` writes `index.json` and signs `index.sig`.
- Older versions live under `<plugin>/versions/<version>/` so older Fibo builds automatically pick the newest compatible release; `fibo plugins install <id>@<version>` pins a version.

## Workflow

```sh
d="${XDG_CONFIG_HOME:-$HOME/.config}/fibo/plugins/git-tools"
mkdir -p "$d"
# Write $d/plugin.json and its declared files (see below)
fibo plugins            # Validate; exits 1 on problems
fibo plugins run git-tools <decl-id>   # Dry-run a hook, decoration, or quota command in the app's environment
```

`fibo plugins` checks syntax and schema; whenever you write a hook, decoration, or quota command, also run `fibo plugins run` (`--event waiting` picks an event; webhooks print a dry run unless `--send` is passed; `--cwd` sets the directory).

## plugin.json

```json
{
  "api": 1,
  "id": "git-tools",
  "name": "Git Tools",
  "version": "0.1.0",
  "description": "Inspect git status and diff from the context menu",
  "provides": ["actions"]
}
```

- `api` is always `1`. `id` uses lowercase ASCII letters, digits, and `-` (≤40 chars, matching the folder name). Action and sidebar preset IDs must be globally unique across enabled plugins; hook and decoration IDs only need to be unique within their plugin.
- `name` is required; `version`, `description`, `files` (relative script/asset paths), `touches` (external files modified by `setup`), `network` (boolean), and `fibo` (required plugin API level, currently up to `8`) are optional.
- `provides` lists at least one kind: `actions` `sidebar` `decorations` `hooks` `rules` `effects` `setup` `quota` `themes` `agent`. Every listed kind must have its file, and undeclared files are ignored.
- Every capability file starts with `"api": 1` (except `agent.json` when using `"api": 2`). Unknown fields inside capability files fail validation immediately so typos surface right away.

## Environment variables passed to plugin commands

Provided to `hooks`, `decorations`, `quota` commands, and `setup`/`teardown` (`actions` run inside a terminal and use `{cwd}` templates instead):

- `MOORING_PLUGIN_ID`, `MOORING_PLUGIN_DIR`: plugin ID and folder path.
- `MOORING_PLUGIN_DATA`: persistent per-plugin data folder (kept across updates, removed on uninstall).
- `MOORING_BIN`, `MOORING_RUNTIME`: path to `fibo` and the active runtime directory.
- `MOORING_OPTION_<KEY>`: each plugin option (`1` / `0` for booleans).

`PATH` comes from the user's login shell (queried once on first plugin command, 3s timeout).

## Options (`options` in `plugin.json`)

```json
{
  "api": 1, "id": "notify-me", "name": "Notify Me", "provides": ["hooks"],
  "options": [
    {"key": "only_errors", "title": "Errors only", "type": "boolean", "default": false, "description": "Notify only when a turn fails"},
    {"key": "interval", "title": "Interval (s)", "type": "number", "default": 300},
    {"key": "sound", "title": "Sound", "type": "choice", "choices": ["Glass", "Ping"], "default": "Glass"}
  ]
}
```

- `type`: `string`, `number`, `boolean`, or `choice` (with `choices`). `key` uses `[a-z0-9_]` (≤20 chars).
- Users edit options in **Settings › Plugins** or via `fibo plugins set <id> <key> <value>`.
- Read options from `"$MOORING_OPTION_<KEY>"` in scripts — never interpolate untrusted values into code.
- Secrets use `"secret": true` (`"type": "string"`, `"default": ""`, plugin level 3): stored in macOS Keychain, redacted from logs, and set via stdin: `printf %s VALUE | fibo plugins set <id> <key> -`.

## Run log

`fibo plugins log <id>` shows the last 50 runs (`setup`, `teardown`, `hooks`, and state changes of `decorations` / `quota`).

## actions: Context menu & Command Palette (`actions.json`)

```json
{
  "api": 1,
  "actions": [
    {"id": "git-diff", "title": "Show Git Diff Here", "command": "git -C {cwd} diff --stat && git -C {cwd} diff", "run": "split"},
    {"id": "resume-tab", "title": "Resume Claude Session in New Tab", "command": "claude --resume {session_id}", "run": "tab", "when": "agent", "agent": "claude"}
  ]
}
```

- Templates `{cwd}` `{session_id}` `{transcript}` `{task}` `{agent}` `{plugin_dir}` `{plugin_data}` are shell-quoted automatically — do not wrap them in quotes yourself.
- `run`: `split` (default), `tab`, or `paste` (inserts without pressing Return).
- `when`: `always` (default), `agent`, `shell`, or `task`. Optional `agent` restricts to a specific CLI (`"claude"`).

## sidebar: Sidebar presets (`sidebar.json`)

```json
{
  "api": 1,
  "presets": [
    {"id": "focus", "name": "Focus", "group": "status", "fold": ["running", "done", "stopped", "fresh", "stale"], "detail": "always", "limit": 8}
  ]
}
```

- `group`: `flat`, `status`, `directory`, `recent` (supports `machines`: `mixed`, `rail`, `sections`), or `wasm`.
- `fold`: collapsible sections (`running` `stopped` `fresh` `stale`; `attention` never folds).
- `detail`: `never` (default), `attention`, or `always`. `limit`: max rows before overflow to Overview (default 12).

## decorations: Pane header / tooltip badges (`decorations.json`)

```json
{
  "api": 1,
  "decorations": [
    {"id": "node", "command": "[ -f package.json ] && node -v", "place": "title", "refresh_s": 60},
    {"id": "last-commit", "command": "git log -1 --format='Last commit %cr: %s' 2>/dev/null", "place": "tip", "refresh_s": 30}
  ]
}
```

- Runs `sh -c` in the pane's `cwd` (`MOORING_CWD`, `MOORING_AGENT`, `MOORING_SESSION`, `MOORING_LAST_EXIT`, `MOORING_LAST_DURATION_MS`) and displays the first stdout line; non-zero exit (except 126/127) hides quietly.
- `place`: `title` or `tip`. `tone`: `muted` (default), `ok`, `warn`, `error`. `when`: `always`, `agent`, `shell`.

## hooks: Session & command notifications (`hooks.json`)

```json
{
  "api": 1,
  "hooks": [
    {"id": "notify", "on": ["waiting", "error", "done"], "command": "osascript -e 'on run argv' -e 'display notification (item 1 of argv) with title (item 2 of argv)' -e 'end run' \"$MOORING_DETAIL\" \"$MOORING_TITLE\"", "timeout_ms": 5000}
  ]
}
```

- `on`: `started` `waiting` `error` `done` (sessions), `command_done` `command_failed` (shell commands).
- **Event text comes from agent conversations and may contain arbitrary quotes or shell syntax — never interpolate env vars inside `osascript -e`, `python -c`, `node -e`, or SQL strings**: pass `"$MOORING_DETAIL"` and `"$MOORING_TITLE"` as `argv` arguments as shown above.
- Set either `command` (`sh -c`, JSON payload on stdin plus `MOORING_*` env vars) or `webhook` (POST JSON). `timeout_ms` defaults to 5000 (max 30000); `min_ms` filters fast shell commands.

## rules: Verification command patterns (`rules.json`)

```json
{"api": 1, "verify": [["just", "test"], ["./scripts/ci.sh"], ["task", "*"]]}
```

Prefix token patterns (`*` matches one word) whose exit code and test counts count as verification evidence.

## effects: Completion animations (`effects.json`)

```json
{"api": 1, "complete": {"style": "stamp", "duration_ms": 2400, "land": "sidebar"}}
```

- `style`: `seal`, `stamp`, `sweep`, `dissolve`, `fade`, face styles (`relief` `wake` `sunrise` `heart` `cat` `tea` `hop`, which also accept custom `face` frames), or terminal effects (`moor` `gravity` `swirl` `backspace` `crt` `confetti`). `duration_ms`: `600`–`4000`; `land`: `sidebar` or `none`.

## setup: Install / uninstall scripts (`setup.json`)

```json
{"api": 1, "setup": "sh setup.sh", "teardown": "sh teardown.sh", "timeout_ms": 10000}
```

Both `setup` (idempotent; run on install/update/enable) and `teardown` (restores original state from `MOORING_PLUGIN_DATA`; run on disable/uninstall or failed `setup`) are required.

## themes: Bundled color themes (`themes/*.json`)

Pure data (`provides: ["themes"]`); list any background images in `plugin.json`'s `files`.

## quota: Agent rate-limit meters (`quota.json`)

```json
{"api": 1, "sources": [{
  "agent": "claude",
  "file": "statusline.json",
  "windows": [
    {"minutes": 300, "used": "/rate_limits/five_hour/used_percentage", "resets_at": "/rate_limits/five_hour/resets_at"},
    {"minutes": 10080, "used": "/rate_limits/seven_day/used_percentage", "resets_at": "/rate_limits/seven_day/resets_at"}
  ]
}]}
```

Reads JSON from `file` (relative to `MOORING_PLUGIN_DATA` or absolute/`~/`) or `command` (every `refresh_s`, 60–86400s), extracting `used` percentage and `resets_at` Unix seconds via JSON Pointers (`/`). Optional pointers: `name` (level 8), `at`, `plan`.

## agent: Registering a new CLI (`agent.json` or `agents/*.json`)

Declares an agent CLI's executable, start/resume commands, and JSONL/SQLite transcript mapping (model after `plugins/opencode/agent.json` and check with `fibo plugins`). With `"api": 2`, also supports `screen_rules`, `interrupt`, `answers` (local `127.0.0.1` HTTP approval API — always use `password_env` when supported), and `input_hint`.

## Example

User asks: "Add a context-menu action to open this folder in VS Code":

```sh
d="${XDG_CONFIG_HOME:-$HOME/.config}/fibo/plugins/open-in-vscode"
mkdir -p "$d"
cat > "$d/plugin.json" <<'JSON'
{"api": 1, "id": "open-in-vscode", "name": "Open in VS Code", "provides": ["actions"]}
JSON
cat > "$d/actions.json" <<'JSON'
{"api": 1, "actions": [{"id": "code", "title": "Open Folder in VS Code", "command": "code {cwd}", "run": "paste"}]}
JSON
fibo plugins
```

Once every line shows `✓`, tell the user to click **Reload** in **Settings › Plugins**, then right-click a pane or press Cmd-P and search "VS Code".
