use fibo_core::terminal::agent_input_ready;
#[test]
fn onboarding_and_nonfirst_choices_do_not_receive_task_text() {
    // Real Claude 2.1.302 onboarding has no numeric options, and its preview
    // can put the selected row more than twenty lines above the bottom.
    let theme = format!(
        "Welcome to Claude Code\nChoose the text style that looks best with your terminal\n❯ ✔ Dark mode\n  Light mode\n{}",
        "preview row\n".repeat(25)
    );
    assert!(!agent_input_ready(&theme));
    assert!(!agent_input_ready(
        "Choose the text style that looks best with your terminal\n  ✔ Dark mode\n❯ Light mode"
    ));
    assert!(!agent_input_ready(
        "Sign in\n  1. With ChatGPT\n> 2. With Device Code\n  3. API key"
    ));
    assert!(!agent_input_ready("Allow this command?\n› 3. No, cancel"));
    assert!(agent_input_ready("› 写一个测试\n? for shortcuts"));
    assert!(agent_input_ready(&format!("{theme}\n❯ \n? for shortcuts")));
    assert!(agent_input_ready(
        "› 2. Cancel\n\n› Ask Codex to do anything"
    ));
}
#[test]
fn bracketed_paste_pages_without_an_input_prompt_are_not_ready() {
    assert!(!agent_input_ready(
        "Security notes:\n1. Claude can make mistakes.\n2. Only use code you trust.\nPress Enter to continue…"
    ));
    assert!(!agent_input_ready(
        "Welcome to Codex\n>_ OpenAI Codex\nLoading…"
    ));
    assert!(!agent_input_ready("project% "));
    assert!(agent_input_ready("❯ \n? for shortcuts"));
    assert!(!agent_input_ready(
        "Accessing workspace:\n❯ No, exit\n  Yes, I trust this folder\nEnter to confirm · Esc to cancel"
    ));
    assert!(!agent_input_ready(
        "Accessing workspace:\n  No, exit\n❯ Yes, I trust this folder\nEnter to confirm · Esc to cancel"
    ));
    assert!(agent_input_ready(
        "❯ No, exit\nEnter to confirm · Esc to cancel\n❯ \n? for shortcuts"
    ));
}
#[test]
fn a_missing_agent_command_is_reported_not_waited_on() {
    assert!(super::command_missing(
        "Agent-OS% codex\nzsh: command not found: codex\nAgent-OS% "
    ));
    assert!(super::command_missing(
        "$ claude\nbash: claude: command not found\n$ "
    ));
    assert!(!super::command_missing("› 写一个测试\n\n  ? for shortcuts"));
}
#[test]
fn menus_block_placement_but_the_input_box_does_not() {
    assert!(!agent_input_ready(
        "Do you trust the files in this folder?\n\n❯ 1. Yes, proceed\n  2. No, exit\n"
    ));
    assert!(!agent_input_ready(
        "│ › 1. Yes, allow Codex to work in this folder\n"
    ));
    assert!(agent_input_ready(
        "› Ask Codex to do anything\n\n  ? for shortcuts\n"
    ));
    assert!(agent_input_ready("> \n  1. plan written by the agent\n"));
}
