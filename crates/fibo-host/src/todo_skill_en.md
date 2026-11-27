---
name: fibo
description: "When the user says 'note this down', 'add a todo', or 'follow up later', or when out-of-scope follow-up work surfaces, record it in Fibo Todos. Skip questions answered on the spot."
---
<!-- Written by Fibo; updated when the app starts. -->

# Record a Fibo Todo

1. Run `fibo todo list` first to check whether it is already tracked.
2. If it already exists: `fibo todo note --task <id> "new progress or context"`.
3. Otherwise: `fibo todo add "One-line title" --said "User's exact words" --done-when "what counts as done"`. If asked for a space, pass `--space <space>` using one of the spaces named in the error. If you start on it right away, add `--work`.

Do not record: questions answered on the spot, decisions not to act ("keep them all for now"), or a bare link with nothing to do.

Tell the user in one sentence what was recorded.

When it is done: `fibo todo note --done --task <id> "evidence: PR, commit, checks run"`. The todo then reads Awaiting your check, and the user confirms or sends it back in the window or on the phone — you cannot mark a todo as completed.

If `fibo` is missing or unreachable, report the error to the user. Never bypass the CLI by writing directly to Fibo's socket or database.
