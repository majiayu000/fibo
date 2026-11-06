//! Which child process of an agent is the background shell its Stop hook listed. Only this
//! finds; whether what was found is a job or a server is `background_shells`.
//!
//! Claude Code runs a command as `zsh -c '… eval '<command>' …'`, and how it writes a `'`
//! inside is its own business (`'\''`, since 2.1.319 `'"'"'`), so quoting is left out of the
//! comparison. `exec` is the exception to that shape: it replaces the shell, and all that is
//! left of the command is what it ran.

/// How much of a command is compared: the hook may clip the rest.
const HEAD: usize = 60;

/// Whether a process with this argv runs `command`.
pub(crate) fn runs(argv: &str, command: &str) -> bool {
    let command = command.split("… [+").next().unwrap_or(command);
    let head: String = command.chars().take(HEAD).collect();
    let head = bare(&head);
    if head.trim().is_empty() {
        return false;
    }
    let argv = bare(argv);
    argv.contains(&head)
        || exec_targets(command)
            .iter()
            .any(|target| became(&argv, target))
}

/// Whether a process with this argv is what `exec <target>` turned into: the target itself,
/// the same arguments under a program that renamed itself (Python does), or a script under
/// the interpreter its first line names.
fn became(argv: &str, target: &str) -> bool {
    let starts = |text: &str, with: &str| {
        !with.is_empty()
            && text
                .strip_prefix(with)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    };
    let args = |text: &'_ str| text.split_once(' ').map_or("", |(_, rest)| rest).to_owned();
    starts(argv, target) || starts(&args(argv), &args(target)) || starts(&args(argv), target)
}

fn bare(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\'' | '"' | '\\'))
        .collect()
}

/// What each `exec` at a command position would leave as the process's argv, as far as the
/// text can tell: up to the first redirection, separator or expansion.
fn exec_targets(command: &str) -> Vec<String> {
    let mut out = vec![];
    for (at, _) in command.match_indices("exec ") {
        let before = command[..at].trim_end();
        if !(before.is_empty() || before.ends_with([';', '&', '|', '\n', '('])) {
            continue;
        }
        let rest = &command[at + "exec ".len()..];
        let end = rest
            .find([';', '|', '&', '<', '>', '$', '`', '\n', ')'])
            .unwrap_or(rest.len());
        let target = bare(&rest[..end]);
        let target = target.trim();
        if !target.is_empty() && !target.starts_with('-') {
            out.push(target.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Claude Code's wrapper around `eval`, as `ps -ww` showed it under 2.1.319.
    fn wrapped(quoted: &str) -> String {
        format!(
            "/bin/zsh -c source /x/snapshot.sh 2>/dev/null || true && eval '{quoted}' < /dev/null && pwd -P >| /tmp/cwd"
        )
    }
    /// Every way of writing a command that was checked against a real Claude Code, with the
    /// process it left. A new shape that is not found gets a row here and in the decision
    /// record (docs/decisions/2026-11-04-background-shell-own-client.md).
    #[test]
    fn finding_cases() {
        let new_quote = r#"'"'"'"#;
        let cases: Vec<(&str, &str, String)> = vec![
            ("plain", "python3 -m http.server 8000", wrapped("python3 -m http.server 8000")),
            ("clipped by the hook", "python3 -m http.server 8000… [+40 chars]", wrapped("python3 -m http.server 8000 --bind 127.0.0.1")),
            ("old quoting", "echo 'hi' && sleep 9", wrapped(r"echo '\''hi'\'' && sleep 9")),
            (
                "new quoting, 2.1.319",
                "R=o/r; perl -e 'alarm 2400; exec @ARGV' sh -c 'while :; do gh run list; done'",
                wrapped(&format!("R=o/r; perl -e {q}alarm 2400; exec @ARGV{q} sh -c {q}while :; do gh run list; done{q}", q = new_quote)),
            ),
            ("pipe", "/bin/sleep 151 | cat", wrapped("/bin/sleep 151 | cat")),
            (
                "heredoc",
                "python3 - <<'EOF'\nimport time; time.sleep(152)\nEOF",
                wrapped(&format!("python3 - <<{q}EOF{q}\nimport time; time.sleep(152)\nEOF", q = new_quote)),
            ),
            ("several lines", "x=1\n/bin/sleep 154", wrapped("x=1\n/bin/sleep 154")),
            ("expansions", "echo \"a $HOME\" `date` && /bin/sleep 155", wrapped("echo \"a $HOME\" `date` && /bin/sleep 155")),
            ("exec alone", "exec /bin/sleep 153", "/bin/sleep 153".into()),
            ("exec after cd", "cd /tmp && exec /bin/sleep 153", "/bin/sleep 153".into()),
            ("exec with a redirection", "exec /bin/sleep 153 > /tmp/log 2>&1", "/bin/sleep 153".into()),
            ("exec with quoted arguments", "exec python3 -c 'import time' x", "python3 -c import time x".into()),
            ("exec of a program that renames itself", "exec python3 -m http.server 41778", "/Library/Frameworks/Python.framework/Versions/3.12/Resources/Python.app/Contents/MacOS/Python -m http.server 41778".into()),
            ("exec of a script", "exec /x/job.sh a", "/bin/sh /x/job.sh a".into()),
            ("exec, before the shell has reached it", "sleep 1; exec /bin/sleep 153", wrapped("sleep 1; exec /bin/sleep 153")),
        ];
        for (name, command, argv) in &cases {
            assert!(runs(argv, command), "{name}: {argv}");
        }
    }
    #[test]
    fn another_process_is_not_taken_for_the_command() {
        let argv = wrapped("python3 -m http.server 8000");
        assert!(!runs(&argv, "cargo test"));
        assert!(!runs(&argv, "  "));
        assert!(!runs(&argv, "''"), "nothing but quotes names no command");
        assert!(!runs("/bin/sleep 1530", "exec /bin/sleep 153"));
        assert!(!runs("node /x/mcp.js", "exec node /y/server.js"));
        assert!(
            !runs("/bin/sleep 153", "echo exec /bin/sleep 153"),
            "an argument, not a command"
        );
        assert!(!runs("@ARGV sh -c x", "perl -e 'alarm 9; exec' -l"));
        assert!(
            !runs("anything", "exec \"$@\""),
            "a target the text cannot tell"
        );
    }
}
