//! Pictures picked on a phone's new task page, handed to the Agent the way its CLI takes
//! them when it starts: Claude Code reads an `@path` in the first message as the picture
//! itself (no Read call, and no asking to read outside the folder: 2026-11-12 tried it,
//! a bare path did ask); Codex takes `--image=path`, starting or resuming. They follow the
//! words, so the conversation's title is still what was asked.
use std::io;

/// The command and first message to start `agent` with, `pictures` attached. No pictures:
/// both as they were. An Agent with no way to take them refuses rather than drop them.
pub(super) fn attach(
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
                .map(|p| format!("--image={}", mooring_platform::shell::quote(p)))
                .collect();
            Ok((format!("{command} {}", flags.join(" ")), prompt))
        }
        _ => Err(io::Error::other("这个 Agent 开新任务时还不能带图片")),
    }
}

#[cfg(test)]
mod tests {
    use super::attach;

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
        let q = mooring_platform::shell::quote;
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
}
