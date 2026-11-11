---
name: mooring
description: "When the user says 'note this down', 'add a todo', or 'follow up later', or when out-of-scope follow-up work surfaces, record it in Fibo Todos. Skip questions answered on the spot."
---
<!-- Written by Fibo; updated when the app starts. -->

# Record a Fibo Todo

1. Run `fibo todo list` first to check whether it is already tracked.
2. If it already exists: `fibo todo note --task <id> "new progress or context"`.
3. Otherwise: `fibo todo add "One-line title" --said "User's exact words"`. If asked for a space, pass `--space <space>` using one of the spaces named in the error.

Tell the user in one sentence what was recorded.

Completion is marked by the user in the Fibo window — you cannot mark a todo done.

If `fibo` is missing or unreachable, report the error to the user. Never bypass the CLI by writing directly to Fibo's socket or database.
