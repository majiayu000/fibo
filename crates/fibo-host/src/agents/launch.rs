//! A first message and pictures handed to the CLI when starting or resuming it.
//! OpenCode's positional argument is a project directory, so its message is `--prompt=`.
//! Claude Code reads an `@path` in the first message as the picture
//! itself (no Read call, and no asking to read outside the folder: 2026-11-12 tried it,
//! a bare path did ask); Codex takes `--image=path`, starting or resuming. They follow the
//! words, so the conversation's title is still what was asked.
use fibo_platform::shell;
use std::io;

/// The command line that starts `agent`, and its first message exactly as that command line
/// hands it to the CLI: the words with each picture's reference for Claude Code, the words
/// alone for the rest. User text stays in the shell's separate argument, OpenCode's option
/// value included.
pub fn launch(
    shell: &str,
    agent: &str,
    command: String,
    prompt: Option<&str>,
    pictures: &[String],
    resuming: bool,
) -> io::Result<(Vec<String>, Option<String>)> {
    // OpenCode 1.19.5 only consumes --prompt on its home route, not its session route.
    // Refuse before opening anything rather than report success after losing the message.
    if agent == "opencode" && resuming && prompt.is_some() {
        return Err(io::Error::other(t!(
            "OpenCode 恢复会话时不能附带消息；请先留空恢复，再在聊天中发送"
        )));
    }
    let (mut command, message) = attach(agent, command, prompt, pictures)?;
    let argument = match (agent, &message) {
        ("opencode", Some(text)) => Some(format!("--prompt={text}")),
        (_, text) => {
            if text.as_ref().is_some_and(|p| p.starts_with('-')) {
                command.push_str(" --");
            }
            text.clone()
        }
    };
    let argv = shell::run_then_stay(shell, &command, argument.as_deref());
    Ok((argv, message))
}

/// The command and first message to start `agent` with, `pictures` attached. No pictures:
/// both as they were. An Agent with no way to take them refuses rather than drop them.
fn attach(
    agent: &str,
    command: String,
    prompt: Option<&str>,
    pictures: &[String],
) -> io::Result<(String, Option<String>)> {
    let prompt = prompt.map(str::to_owned);
    if pictures.is_empty() {
        return Ok((command, prompt));
    }
    match agent {
        "claude" => {
            let refs: Vec<String> = pictures.iter().map(|p| format!("@{p}")).collect();
            let refs = refs.join(" ");
            let prompt = match prompt {
                Some(p) => format!("{p}\n\n{refs}"),
                None => refs,
            };
            Ok((command, Some(prompt)))
        }
        "codex" => {
            let flags: Vec<String> = (pictures.iter())
                .map(|p| format!("--image={}", fibo_platform::shell::quote(p)))
                .collect();
            Ok((format!("{command} {}", flags.join(" ")), prompt))
        }
        _ => Err(io::Error::other(t!("这个 Agent 开新任务时还不能带图片"))),
    }
}

#[cfg(test)]
mod tests {
    use super::attach;

    fn launch_argv(
        shell: &str,
        agent: &str,
        command: String,
        prompt: Option<&str>,
        pictures: &[String],
        resuming: bool,
    ) -> std::io::Result<Vec<String>> {
        super::launch(shell, agent, command, prompt, pictures, resuming).map(|(argv, _)| argv)
    }

    const PIC: &str = "/r/phone-images/a.jpg";

    #[test]
    fn claude_reads_the_pictures_after_the_words() {
        let (command, prompt) =
            attach("claude", "claude".into(), Some("看图"), &[PIC.into()]).unwrap();
        assert_eq!(command, "claude");
        assert_eq!(prompt.unwrap(), "看图\n\n@/r/phone-images/a.jpg");
    }

    #[test]
    fn codex_takes_each_picture_as_a_flag_that_leaves_the_prompt_alone() {
        let pics = [PIC.to_owned(), "/r/phone-images/b.png".to_owned()];
        let (command, prompt) =
            attach("codex", "codex resume x".into(), Some("看图"), &pics).unwrap();
        let q = fibo_platform::shell::quote;
        let want = format!("codex resume x --image={} --image={}", q(PIC), q(&pics[1]));
        assert_eq!(command, want);
        assert_eq!(prompt.as_deref(), Some("看图"));
    }

    #[test]
    fn no_pictures_changes_nothing_and_an_agent_without_a_way_refuses() {
        let same = attach("grok", "grok".into(), Some("hi"), &[]).unwrap();
        assert_eq!(same, ("grok".to_owned(), Some("hi".to_owned())));
        assert!(attach("grok", "grok".into(), Some("hi"), &[PIC.into()]).is_err());
    }

    #[test]
    fn opencode_without_a_task_keeps_the_existing_start_or_resume_command() {
        for command in [
            "opencode --port 0",
            "opencode --port 0 --session old-session",
        ] {
            assert_eq!(
                launch_argv("shell", "opencode", command.into(), None, &[], true).unwrap(),
                fibo_platform::shell::run_then_stay("shell", command, None)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn opencode_tasks_are_prompt_options_not_directories() {
        let command = "printf '%s\\n'";
        for prompt in [
            "phone-walk task",
            "--looks-like-a-flag",
            "a 'quote' $(false)\nnext",
        ] {
            let argv = launch_argv(
                "/usr/bin/true",
                "opencode",
                command.into(),
                Some(prompt),
                &[],
                false,
            )
            .unwrap();
            // Run the actual shell script with a harmless argv-printing stand-in.
            let output = std::process::Command::new("/bin/sh")
                .args(&argv[3..])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                format!("--prompt={prompt}\n")
            );
        }
    }

    /// The first message kept as evidence is the one on the command line, not the words
    /// alone: Claude Code's carries its pictures, OpenCode's comes as `--prompt=` and its
    /// value is what the CLI keeps.
    #[test]
    fn the_first_message_is_what_the_command_line_hands_over() {
        let words = "--两行\n`代码` \"引号\"";
        for (agent, pictures, sent) in [
            ("codex", vec![PIC.to_owned()], words.to_owned()),
            ("opencode", vec![], words.to_owned()),
            ("claude", vec![PIC.to_owned()], format!("{words}\n\n@{PIC}")),
        ] {
            let (argv, message) =
                super::launch("sh", agent, agent.into(), Some(words), &pictures, false).unwrap();
            assert_eq!(message.as_deref(), Some(sent.as_str()), "{agent}");
            let handed = argv.last().unwrap();
            assert!(
                handed == &sent || handed == &format!("--prompt={sent}"),
                "{agent}: {argv:?}"
            );
        }
        let (_, none) = super::launch("sh", "codex", "codex".into(), None, &[], true).unwrap();
        assert_eq!(none, None);
    }

    #[test]
    fn opencode_resume_refuses_a_message_instead_of_silently_discarding_it() {
        assert!(launch_argv(
            "shell",
            "opencode",
            "opencode --session old-session".into(),
            Some("continue the task"),
            &[],
            true,
        )
        .is_err());
    }

    #[test]
    fn positional_prompts_still_guard_leading_dashes() {
        for agent in ["claude", "codex"] {
            assert_eq!(
                launch_argv("shell", agent, agent.into(), Some("--literal"), &[], false).unwrap(),
                fibo_platform::shell::run_then_stay(
                    "shell",
                    &format!("{agent} --"),
                    Some("--literal")
                )
            );
        }
    }
}
