//! Telling a CLI from a process's arguments, however its system writes paths.
use super::*;

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

#[test]
fn a_cli_is_found_by_its_name_on_every_system() {
    for program in [
        "/usr/local/bin/codex",
        r"C:\Users\me\node_modules\@openai\codex\vendor\codex.exe",
        r"C:\tools\CODEX.EXE",
    ] {
        let name = program_name(program);
        assert!(name.eq_ignore_ascii_case("codex"), "{program}: {name}");
    }
    assert_eq!(
        from_argv(&argv(&[r"C:\x\vendor\codex.exe"])),
        Some(Agent::Codex)
    );
    assert_eq!(from_argv(&argv(&["/opt/bin/claude"])), Some(Agent::Claude));
    assert_eq!(from_argv(&argv(&["/bin/zsh", "-l"])), None);
    assert_eq!(from_argv(&[]), None);
}

/// As npm's own launcher on Windows starts it, with both slashes in one path, and from the
/// directory Node's installer uses, which has a space in it.
#[test]
fn a_cli_is_found_by_the_script_its_interpreter_runs() {
    let claude = argv(&[
        r"C:\Program Files\nodejs\node.exe",
        r"C:\Users\me\AppData\fnm/node_modules/@anthropic-ai/claude-code/cli.js",
    ]);
    assert_eq!(from_argv(&claude), Some(Agent::Claude));
    let backslashes = argv(&[
        r"C:\Program Files\nodejs\node.exe",
        r"C:\Users\me\node_modules\@anthropic-ai\claude-code\cli.js",
    ]);
    assert_eq!(from_argv(&backslashes), Some(Agent::Claude));
    let other = argv(&[r"C:\Program Files\nodejs\node.exe", r"C:\work\server.js"]);
    assert_eq!(from_argv(&other), None);
}

#[test]
fn flags_are_carried_past_the_script_an_interpreter_runs() {
    let args = argv(&[
        r"C:\Program Files\nodejs\node.exe",
        r"C:\x\node_modules\@anthropic-ai\claude-code\cli.js",
        "--dangerously-skip-permissions",
    ]);
    assert_eq!(
        adapter(Agent::Claude).launch_flags(&args),
        ["--dangerously-skip-permissions"]
    );
}
