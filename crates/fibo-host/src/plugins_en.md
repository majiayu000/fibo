# Customizing Fibo: Plugins and Appearance

For coding agents: follow this when the user asks you to add a command to Fibo, change how the sidebar is arranged, show something on a pane, send a notification when an event happens, change the completion animation, or change status colors. Everything is plain files; Fibo never needs recompiling.

## Where files live

The config directory is `${XDG_CONFIG_HOME:-$HOME/.config}/fibo`, written `<config>` below.

- Plugins: `<config>/plugins/<id>/`, one directory per plugin.
- Status colors: part of the theme (see "Themes" in `fibo --skill`).
- Shortcuts, terminal font, completion animation, pinned prompts and the like are changed in the app's **Settings**; don't hand-edit their files.

## Rules

- **One idea per plugin.** Make a new directory; don't fold new things into an existing plugin.
- Don't change the user's existing plugins or `plugins/disabled.json` (that is the switch in Settings) unless the user explicitly asks.
- Always run `fibo plugins` when done. It is only right when every line shows `✓`; for a `✗`, fix what it says and run it again.
- Then tell the user to click **Reload** in **Settings › Plugins**. It takes effect right away, `agent` plugins included, with no restart. Settings › Plugins also shows whether each plugin took effect and can turn it off or remove it.
- Don't write what already exists: `fibo plugins available` lists the plugins bundled with the app (add `--remote` to search the remote catalog too), and `fibo plugins install <id>` installs one. Installing, removing or switching plugins from the command line reaches a running Fibo immediately; no Reload needed.
- Plugins can only do what the kinds below allow. If the user wants something they can't do (drawing custom UI, changing session state, reading the conversation text), say so plainly. Never modify Fibo's installed files.

## Publishing your own catalog

A catalog is an `index.json` plus each plugin's files, served from any `https://` address or `file://` path. Users add it to `<config>/market`.

- `fibo plugins keygen --name <your name>` creates a private key (`<config>/publisher.key` by default, readable only by you; keep it out of git). After that, `fibo plugins index <dir>` writes `index.json` and signs it into `index.sig`.
- The first install shows users the publisher's name and the start of the key. If the key later changes or the signature disappears, they are asked again; a catalog whose signature doesn't match is not used at all. Unsigned catalogs work too, but every plugin from one asks on its first install.
- Older versions go in `<plugin>/versions/<version>/`, each a complete plugin directory; `index` lists them as well. When the app is too old for the newest version, it automatically gets the newest one it can run. `fibo plugins install <id>@<version>` picks a version.

## Workflow

```sh
d="${XDG_CONFIG_HOME:-$HOME/.config}/fibo/plugins/git-tools"
mkdir -p "$d"
# Write $d/plugin.json and the files it provides (see below)
fibo plugins            # Check; exits 1 on problems
fibo plugins run git-tools <declaration id>   # hooks, decorations, quota commands: one real run in the app's environment
```

`fibo plugins` only says the files make sense. If you wrote a hook, a decoration or a quota command, also run it once with `fibo plugins run`; it only counts as working once you see its output. For hooks, `--event waiting` picks the event; webhooks only print what they would send unless you add `--send`; `--cwd` sets the directory.

## plugin.json

```json
{
  "api": 1,
  "id": "git-tools",
  "name": "Git Tools",
  "version": "0.1.0",
  "description": "Git status and diff from the context menu",
  "provides": ["actions"]
}
```

- `api` is always `1`. `id` uses only lowercase letters, digits and `-`, at most 40 characters, ideally the same as the directory name; when two plugins share an id, the second one doesn't load. Hook and decoration ids only need to be unique within their plugin. Action and sidebar ids must be unique across all plugins (switches and choices are stored by them); a plugin that repeats one won't install.
- `name` is required; `version` and `description` are optional. Fields in `plugin.json` that this build doesn't know are skipped with a warning.
- `files` (optional): other files the plugin ships, such as scripts its commands run, relative to the plugin directory. Files not listed here are left behind when installing from the plugin directory.
- `touches` (optional): files of other programs that `setup` changes, e.g. `["~/.claude/settings.json"]`. `network` (optional): whether its commands go online. Both are shown to the user before installing. Fibo doesn't enforce them, so be truthful.
- When installing a plugin that runs commands, the command line lists these first and asks y/N in an interactive terminal; non-interactive callers (agents, scripts) must pass `--yes`.
- `mooring` (optional): the plugin level it needs. The key keeps its old name; a `"fibo"` key is not recognized and is skipped with a warning. The current level is 9: `setup`, `quota`, `files` and `options` came in level 2; `themes`, an action's `agent` and an option's `secret` in level 3; `screen_rules` and `interrupt` in `agent.json` in level 4; its `answers` in level 5. Without it, the level is worked out from the kinds and fields `plugin.json` uses; if you use a field of a kind's file added after level 2 (such as an action's `agent`), write the level yourself. A Fibo whose level is too low refuses the plugin and asks for an update.
- Kinds and fields in `plugin.json` that only newer versions know are skipped by older ones, with a warning on the settings page; everything else loads. The plugin only fails to load when none of its kinds are known. An unknown event in a hook's `on` likewise skips just that event.
- `provides` lists at least one of: `actions` `sidebar` `decorations` `hooks` `rules` `effects` `setup` `quota` `themes` `agent`. Each kind listed must have its file; a file whose kind isn't listed is not loaded.
- Every kind file also starts with `"api": 1` (except `agent.json`, see the end). An unknown field in a kind file makes that file not take effect, so a misspelled field name shows up right away.

## Environment variables every plugin command gets

Hooks, decorations, quota `command`s and `setup`/`teardown` all get these (actions run in a terminal and use templates such as `{cwd}` instead). Use the `MOORING_*` names for these: this version also sets the same values as `FIBO_*`, but earlier Fibo versions set only the old names, so a plugin using the new ones would get empty values there:

- `MOORING_PLUGIN_ID`, `MOORING_PLUGIN_DIR`: which plugin, and its directory (to find scripts shipped in `files`).
- `MOORING_PLUGIN_DATA`: the plugin's own data directory, kept across updates and deleted on removal.
- `MOORING_BIN`, `MOORING_RUNTIME`: the `fibo` command and the host it talks to; `"$MOORING_BIN" list` works in a plugin the same as in a Fibo terminal.
- `MOORING_OPTION_<KEY>`: each of the plugin's options (see below). Switches are `1` / `0`.

`PATH` is the login shell's, so `node`, `gh` and the like are found. The app and the host each open a login shell once, on their first plugin command, to ask for it (your `.zshrc` loads one extra time, about a second, without blocking the window). If no answer comes within 3 seconds, only the system directories are on it.

## Options (`options` in `plugin.json`)

```json
{
  "api": 1, "id": "notify-me", "name": "Notify Me", "provides": ["hooks"],
  "options": [
    {"key": "only_errors", "title": "Only on errors", "type": "boolean", "default": false, "description": "When on, only notify when something fails"},
    {"key": "interval", "title": "Interval (s)", "type": "number", "default": 300},
    {"key": "sound", "title": "Sound", "type": "choice", "choices": ["Glass", "Ping"], "default": "Glass"}
  ]
}
```

- `type`: `string`, `number`, `boolean` or `choice` (needs `choices`). `key` uses only lowercase letters, digits and `_`, at most 20 characters.
- Users change them under each plugin in **Settings › Plugins**, or with `fibo plugins set <id> <key> <value>`. Values are stored in `options.json` in the data directory and used from the next run on.
- Values reach commands only as environment variables, never spliced into the command text. Read `"$MOORING_OPTION_SOUND"` in the script; never let a value become part of a command.
- For API keys and the like, use `"secret": true` (`type` `string`, `default` `""`, plugin level 3). The value goes into the macOS Keychain, not a file; Settings and `fibo plugins set` only show whether it is set, the run log masks it wherever it appears, and it is deleted on removal. On the command line, give the value on stdin: `printf %s VALUE | fibo plugins set <id> <key> -`. Don't have the user put secrets in plugin files or command text.
- Any parameter a user might want different should be an option, not hard-coded in the script.

## Run log

`fibo plugins log <id>` lists the last 50 runs. `setup`, `teardown` and hooks are logged every time; decorations and quota commands only when the outcome changes (the first failure, the recovery). The latest failure also shows in that plugin's description in **Settings › Plugins**.

## actions: Commands in the context menu and ⌘P (`actions.json`)

```json
{
  "api": 1,
  "actions": [
    {"id": "git-diff", "title": "Show changes here", "command": "git -C {cwd} diff --stat && git -C {cwd} diff", "run": "split"},
    {"id": "resume-tab", "title": "Continue this Claude session in a new tab", "command": "claude --resume {session_id}", "run": "tab", "when": "agent", "agent": "claude"}
  ]
}
```

- `command` can use `{cwd}` `{session_id}` `{transcript}` `{task}` `{agent}`, plus `{plugin_dir}` (the plugin directory, to run scripts shipped in `files`) and `{plugin_data}` (its data directory). They are quoted automatically when filled in; don't add quotes yourself.
- `run`: `split` (default, runs in a split beside the pane), `tab` (a new tab), or `paste` (typed into the current terminal without pressing Return, so the user reads it before sending).
- `when`: `always` (default), `agent` (the pane has an agent session), `shell` (just a shell), or `task` (the tab has a todo attached).
- `agent` (optional): only show it in panes of this agent; write the executable or its name (`"agent": "claude"`). Always set it when the command only makes sense for one CLI, or a Codex session id ends up passed to `claude --resume`.

## sidebar: How the sidebar is arranged (`sidebar.json`)

```json
{
  "api": 1,
  "presets": [
    {"id": "focus", "name": "Focus", "group": "status", "fold": ["running", "done", "stopped", "fresh", "stale"], "detail": "always", "limit": 8}
  ]
}
```

- `group`: `flat` (one list for the current space, what needs you first), `status` (all spaces, a section per state), `directory` (all spaces, what needs you pinned on top, the rest by folder), or `recent` (what needs you on top, then every other session newest first). `status` and `directory` have space tabs on top; click one to see only that space. `wasm` only gets the current space's sessions.
- `fold`: which sections collapse into one line until clicked. Choose from `running` `stopped` `fresh` `stale`. `attention` (needs you, which includes waiting for your review) never folds. `done` from older plugins still reads, but that section no longer exists.
- `detail`: when a session row shows a second line: `never` (default), `attention` (rows that need you and the selected one), or `always`.
- `limit`: the most rows the sidebar lists; the rest go to the Board. Default 12.
- `machines`: where other computers' sessions go; only valid with `"group": "recent"`. `mixed` (one list mixed with this computer's, like the built-in "Sessions first"), `rail` (a vertical rail to pick one computer, like the built-in "Machine rail"), or `sections` (a collapsible section per computer, like the built-in "By machine"). Without it, they come after this computer's list. Fibo draws the rail and the grey rows of disconnected computers; plugins can't change them.
- New presets appear under **Choose view** at the bottom of the sidebar and in the command palette, next to the built-in ones rather than replacing them. `rail` and `sections` only appear once other computers are set up.
- There is also `"group": "wasm"` with `"wasm": "<file>.wasm"`, which arranges freely in sandboxed WebAssembly. It has to be compiled from Rust; don't use it unless asked.

## decorations: A short command output on a pane (`decorations.json`)

```json
{
  "api": 1,
  "decorations": [
    {"id": "node", "command": "[ -f package.json ] && node -v", "place": "title", "refresh_s": 60},
    {"id": "last-commit", "command": "git log -1 --format='Last commit %cr: %s' 2>/dev/null", "place": "tip", "refresh_s": 30}
  ]
}
```

- `command` runs with `sh -c` in the pane's directory, and the first line of its output is shown. A non-zero exit or no output shows nothing, so let it fail quietly in directories where it doesn't apply. Exit codes 126 and 127 (the command couldn't run) don't count as quiet: they go to the run log.
- It can read `FIBO_CWD` `FIBO_AGENT` `FIBO_SESSION`, and the shell's previous command as `FIBO_LAST_EXIT` `FIBO_LAST_DURATION_MS` (empty until there is one). The old `MOORING_*` names are still set too, so older plugins need no change.
- `place`: `title` (default, the pane's title bar) or `tip` (its own line in the card shown when hovering the title).
- `tone` (title bar only): `muted` (default), `ok`, `warn` or `error`, following the status colors.
- `when`: `always` (default), `agent` or `shell`. `refresh_s` is how often it reruns, default 10; `timeout_ms` defaults to 1000. Keep the command fast and don't go online here.

## hooks: Run something when a session changes or a command finishes (`hooks.json`)

```json
{
  "api": 1,
  "hooks": [
    {"id": "notify", "on": ["waiting", "error", "done"], "command": "osascript -e 'on run argv' -e 'display notification (item 1 of argv) with title (item 2 of argv)' -e 'end run' \"$FIBO_DETAIL\" \"$FIBO_TITLE\"", "timeout_ms": 5000}
  ]
}
```

- `on`: `started` `waiting` `error` `done` (sessions), `command_done` `command_failed` (a command typed in a shell finished).
- **Event text comes from the agent's conversation and can contain anything. Never splice it into source code of another language.** The `osascript` above takes it through `on run argv`; don't write `-e "display notification \"$FIBO_DETAIL\""`, which lets a bit of conversation text run commands. The same goes for `python -c`, `node -e` and `sqlite3`.
- Write either `command` (`sh -c`) or `webhook` (POSTs a JSON body). The event arrives as JSON on stdin and as environment variables: `FIBO_EVENT` `FIBO_SESSION` `FIBO_AGENT` `FIBO_TITLE` `FIBO_CWD` `FIBO_SPACE` `FIBO_DETAIL` `FIBO_TERMINAL`, plus `FIBO_EXIT_CODE` `FIBO_DURATION_MS` for command events. The old `MOORING_*` names are still set too, so older plugins need no change.
- `timeout_ms` defaults to 5000, at most 30000. `min_ms` only applies to command events: commands faster than it aren't reported. Each plugin runs at most 2 hooks at once, taking turns with other plugins; when events pile up, a plugin queues at most 16 and drops the oldest beyond that, logging each drop to the run log.
- Hooks can only observe, not change a session. They don't get the conversation text, nor the command line itself. The host runs hooks even when the Fibo window isn't open.

## rules: Which commands count as verification (`rules.json`)

```json
{"api": 1, "verify": [["just", "test"], ["./scripts/ci.sh"], ["task", "*"]]}
```

Each entry is the first few words of a command, compared by the last path segment (`./node_modules/.bin/vitest` counts as `vitest`); `*` matches any one word. When an agent runs one of these, its exit code and test counts count as verification evidence. Rules can only add; the built-in ones can't be removed.

## effects: The animation when marking done (`effects.json`)

```json
{"api": 1, "complete": {"style": "stamp", "duration_ms": 2400, "land": "sidebar"}}
```

- `style`: `seal` (the seal, default), `stamp`, `sweep` (a light sweep closes it), `dissolve` (dissolves into the sidebar), `fade` (just fades out); the faces that turn from `-_-` to a smile: `relief`, `wake`, `sunrise`, `heart`, `cat`, `tea`, `hop`; and the ones that play with the terminal's text and pane: `moor`, `gravity` (text falls), `swirl`, `backspace`, `crt` (an old monitor switching off), `confetti`.
- `duration_ms` is 600–4000. `land`: `sidebar` (lands on that session's sidebar row) or `none`.
- The face animations (`relief` `wake` `sunrise` `heart` `cat` `tea` `hop`) can bring their own frames; the scene stays, only the face changes:

  ```json
  {"api": 1, "complete": {"style": "tea", "duration_ms": 3000, "land": "sidebar", "face": {
    "open": "c[", "close": "]",
    "frames": [
      {"left": "=", "mouth": "_", "right": "=", "ms": 900},
      {"left": "^", "mouth": "—", "right": "^", "ms": 300, "scramble": true},
      {"left": "-", "mouth": "—", "right": "-", "ms": 90},
      {"left": "^", "mouth": "—", "right": "^", "ms": 300}
    ]}}}
  ```

  Each frame shows as `open + left + space + mouth + space + right + close`. Every frame must have the same number of characters (or the face jitters), with at most 24 frames of at most 24 characters each. `ms` is how long a frame stays (20–3000); the last frame stays until the face leaves, and the frames before it add up to at most 80% of `duration_ms`. When the last frame's look first appears, the scene turns happy (warmer, a little jump). `scramble` makes the frame flash briefly as garbled text, then settle from left to right. **Settings › Plugins › Done animations › Draw your own** writes exactly this file, saved to `plugins/my-face-<n>/`.
- Installing or turning on an animation plugin switches to its animation; switch back in **Settings › Plugins › Done animations**. Removing or turning off the one in use falls back to the seal. When "Reduce motion" is on in System Settings, every choice just fades out.

## setup: Change another program on install, change it back on removal (`setup.json`)

```json
{"api": 1, "setup": "sh setup.sh", "teardown": "sh teardown.sh", "timeout_ms": 10000}
```

- `setup` runs on install, update and turning on; `teardown` on turning off and removal. Both run with `sh -c` in the plugin directory. `MOORING_PLUGIN_DIR` is the plugin directory and `MOORING_PLUGIN_DATA` its own data directory (`<config>/plugin-data/<id>`, kept across updates and reinstalls).
- Write both: whatever `setup` changed, `teardown` must be able to put back exactly. Save the original settings in `MOORING_PLUGIN_DATA`.
- `setup` runs again when already installed (on update), so it must be safe to repeat. `teardown` must also cope with a half-done setup: it runs right away when `setup` fails.
- On failure (non-zero exit or timeout; `timeout_ms` 100–30000, default 10000), what it printed shows on the settings page. A timeout stops its child processes too.
- A plugin whose `setup` failed isn't installed: a first install stays but isn't turned on, and an update goes back to the old version.
- When `teardown` fails, the plugin isn't removed or turned off unless the user picks **Remove anyway** / **Turn off anyway** (`--force` on the command line).
- When a new version no longer has `setup`, the old version's `teardown` runs before updating, and the update doesn't happen if it fails.
- After a successful removal, `MOORING_PLUGIN_DATA` is deleted entirely; whatever `teardown` needs (the original settings) can only be read before then.
- Example: `plugins/claude-quota` (hooks into Claude Code's status line, and the original status line keeps showing).

## themes: Themes (`themes/*.json`)

Data only; nothing runs. Each file is one theme, in the format described under "Themes" in `fibo --skill`. Once installed it appears in **Settings › Appearance**; turning the plugin off or removing it makes it disappear, and if it was in use, the default theme comes back. When the user clicks **Edit current theme**, their own copy is saved; the plugin is never written to. Relative background image paths are resolved from the plugin directory, and the images must be listed in `plugin.json`'s `files`. Example: `plugins/theme-tide`.

## quota: An agent's usage limits (`quota.json`)

```json
{"api": 1, "sources": [{
  "agent": "claude",
  "command": "\"$MOORING_BIN\" claude-quota usage",
  "refresh_s": 60,
  "at": "/at",
  "error": "/error",
  "windows": [
    {"minutes": 300, "used": "/session/used", "resets_at": "/session/resets_at"},
    {"minutes": 10080, "used": "/week/used", "resets_at": "/week/resets_at"}
  ]
}]}
```

- `agent`: the CLI's executable or name. It shows in the status bar's usage, competing with the built-in reading on which is newer.
- Pick one source. `file` is a JSON file (relative to `MOORING_PLUGIN_DATA`, or an absolute or `~/` path), reread when it changes. `command` is a command that prints JSON, run in the plugin directory every `refresh_s` seconds while the app is open (60–86400, default 300), for at most 20 seconds by default; `timeout_ms` (100–300000, plugin level 8) changes that. The app and the host each run it on their own, so a command that really queries something must take its own lock. When the command exits non-zero, the last reading is kept and the failure goes to the run log; when it exits 0 but no window can be read (say, a switch is off), the agent is no longer shown.
- Each entry in `windows` has `minutes` (300 shows as 5h, 10080 as week); `used` and `resets_at` are JSON Pointers (starting with `/`) to the percentage used and the reset time (Unix seconds). A window whose `used` can't be read isn't shown; with no windows and no `error`, the agent isn't shown at all.
- `name` (optional, also a JSON Pointer): the window's own name, for windows that cover only part of the account (a single model's weekly limit, shown in the popover as e.g. `Fable week`). Unnamed windows are for the whole account. Named windows stay out of the status bar until they reach 80%, and aren't sent to the phone. When one agent has several sources, the newest is shown, and named windows only another source has stay beside it for an hour. Needs plugin level 8: when you use it, write `"mooring": 8` in `plugin.json` and put the previous version in `versions/`, because an older app that meets an unknown field makes the whole file not take effect.
- `at` and `plan` (optional, also JSON Pointers): when the reading was taken (Unix seconds; without it, the file's modification time or the command's run time) and the plan name.
- `error` (optional, plugin level 9): a JSON Pointer to why the refresh failed. On failure, still print JSON and exit 0. If there is an earlier snapshot, keep its original `at` and windows; the window marks it stale and shows the error. With no usable snapshot, print `{"at":0,"error":"reason"}`, which shows as waiting for a reading instead of carrying over the previous account's numbers. Don't overwrite the reading time with the time of this failure.
- If it has to go online, do that in `command` and declare `network: true` in `plugin.json`. Claude usage now uses native HTTP, a shared 120-second cache, a process lock and a credential fingerprint, and no longer reads account limits from the session status line; see `docs/decisions/2026-11-16-claude-quota-http.md`.

## agent: Teaching Fibo a new CLI (`agent.json` or `agents/*.json`)

The most involved kind: it declares the CLI's executable, its start and resume commands, and how to read turn state from its session records (JSONL or SQLite). Only write one when the user wants to add a new CLI. Start from an existing example (`plugins/opencode/agent.json` in Fibo's source) and check it with `fibo plugins` the same way.

With `"api": 2`, `agent.json` can declare a few more things. When you use them, write the matching `"mooring"` level in `plugin.json` and put the previous version in `versions/`, so an older app automatically installs that version instead of installing one that doesn't take effect:

- `screen_rules`: how to recognize from the screen that it is waiting for you, in the same format as `<config>/screen-rules/<id>.json`. When the prompt has no numbers, write `"menu": {"side": "none"}`, let `anchor` match the row of buttons, and use `"ask": "above_anchor"` to take the line above it as the question. Without it, the screen is read the way Claude Code looks. Broken rules fall back to the defaults and are logged in the host log.
- `interrupt`: `{"key": "esc", "presses": 2}`, which key interrupts a turn and how many times; `key` is `esc` or `ctrl-c`. Without it, Ctrl-C is sent; some CLIs quit outright on Ctrl-C, so try it first.
- `answers`: when the CLI has a local HTTP API, approvals are looked up and answered through it, so the desktop and phone get buttons: `pending` (GET, returns the list awaiting answers), `id`, `subject` (the first pointer, in order, that has text), `kind`, `always`, `reply` (POST, contains `{id}`), `allow`, `allow_always`, `deny` (POST bodies). It only connects to a port on 127.0.0.1 that this process itself listens on. The CLI has to be started with the flags that open the port, written into `start` and `resume`; without the port, Fibo falls back to watching the screen. **A local port with no password can be called by any web page open in your browser**, so use a password whenever the CLI supports one: `password_env` names the variable, `username` gives the user name, and `start` sets it to a fresh value each time.
- `input_hint`: `["ask anything"]`, the text on screen while the input box is empty, case-insensitive. CLIs whose input box has no `❯`, `›` or `>` arrow need it, or `fibo run` and a todo's Start can't find the input box to hand over the task. When set, only it is used. Needs plugin level 7.

## Status colors

They live in the theme file's `status`, `tone` and `seen`; see "Themes" in `fibo --skill`. An old `<config>/colors.json` is merged into the theme on first start and left as `colors.json.migrated`; don't write it again.

## Example

The user says "add 'Open this folder in VS Code' to the context menu":

```sh
d="${XDG_CONFIG_HOME:-$HOME/.config}/fibo/plugins/open-in-vscode"
mkdir -p "$d"
cat > "$d/plugin.json" <<'JSON'
{"api": 1, "id": "open-in-vscode", "name": "Open in VS Code", "provides": ["actions"]}
JSON
cat > "$d/actions.json" <<'JSON'
{"api": 1, "actions": [{"id": "code", "title": "Open this folder in VS Code", "command": "code {cwd}", "run": "paste"}]}
JSON
fibo plugins
```

Once every line shows `✓`, tell the user to click **Reload** in **Settings › Plugins**, then right-click a pane or press ⌘P and search "VS Code".
