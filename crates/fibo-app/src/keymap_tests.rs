use super::*;

fn press(text: &str) -> Keystroke {
    Keystroke::parse(text).unwrap()
}

#[test]
fn chords_round_trip_and_shifted_symbols_fold() {
    for text in [
        "cmd-t",
        "cmd-shift-[",
        "cmd--",
        "cmd-,",
        "cmd-ctrl-alt-shift-k",
    ] {
        assert_eq!(Chord::parse(text).unwrap().text(), text);
    }
    assert_eq!(
        Chord::parse("cmd-}").unwrap(),
        Chord::parse("cmd-shift-]").unwrap()
    );
    assert_eq!(
        Chord::from_keystroke(&press("cmd-shift-}")),
        Chord::parse("cmd-shift-]")
    );
    assert_eq!(Chord::parse("cmd-shift-d").unwrap().display(), "⇧⌘D");
    assert_eq!(Chord::parse("cmd--").unwrap().display(), "⌘-");
    assert_eq!(Chord::parse("hyper-t"), None);
    assert_eq!(Chord::parse("cmd-"), None);
    assert_eq!(Chord::parse("cmd-shift"), None);
}

#[test]
fn defaults_follow_the_terminal_apps() {
    let map = Keymap::default();
    for (keys, action) in [
        ("cmd-t", Action::NewTab),
        ("cmd-w", Action::Close),
        ("cmd-d", Action::SplitRight),
        ("cmd-shift-d", Action::SplitDown),
        ("cmd-shift-[", Action::PrevTab),
        ("cmd-shift-}", Action::NextTab),
        ("cmd-[", Action::PrevPane),
        ("cmd-]", Action::NextPane),
        ("cmd-1", Action::Tab(1)),
        ("cmd-8", Action::Tab(8)),
        ("cmd-9", Action::LastTab),
        ("cmd-p", Action::Search),
        ("cmd-,", Action::Settings),
        ("cmd-j", Action::NextWaiting),
        ("cmd-b", Action::ToggleSidebar),
        ("cmd-f", Action::Find),
        ("cmd-k", Action::Clear),
        ("cmd-=", Action::ZoomIn),
        ("cmd-shift-=", Action::ZoomIn),
        ("cmd--", Action::ZoomOut),
        ("cmd-0", Action::ZoomReset),
        ("cmd-home", Action::ScrollTop),
        ("cmd-end", Action::ScrollBottom),
        ("cmd-pageup", Action::PageUp),
        ("cmd-pagedown", Action::PageDown),
    ] {
        assert_eq!(map.action_for(&press(keys)), Some(action), "{keys}");
    }
    // Copy, paste and everything without ⌘ stay out of the keymap.
    for keys in [
        "cmd-c",
        "cmd-v",
        "ctrl-t",
        "ctrl-d",
        "alt-f",
        "t",
        "cmd-shift-t",
    ] {
        assert_eq!(map.action_for(&press(keys)), None, "{keys}");
    }
    let mut seen = vec![];
    for (a, chords) in &map.bindings {
        for c in chords {
            assert!(a.fixed() || c.refusal().is_none(), "{}", c.text());
            assert!(!seen.contains(c), "{}", c.text());
            seen.push(c.clone());
        }
    }
}

#[test]
fn tab_numbers_are_fixed_and_old_files_still_load() {
    assert!(Keymap::actions().all(|a| !a.fixed()));
    assert!(Keymap::actions().any(|a| a == Action::NextPane));
    let refused = Chord::parse("cmd-3").unwrap().refusal();
    assert_eq!(refused, Some("⌘1–⌘9 固定用于切换标签"));
    assert!(Chord::parse("cmd-shift-3").unwrap().refusal().is_none());
    // An older file: its tab-number entries and a move onto ⌘1 are skipped, the
    // rest still applies.
    let old = r#"{"tab_1":["cmd-shift-1"],"last_tab":["cmd-0"],"close":["cmd-1"],"new_tab":["cmd-shift-t"]}"#;
    let map = Keymap::parse(old).unwrap();
    assert_eq!(map.action_for(&press("cmd-1")), Some(Action::Tab(1)));
    assert_eq!(map.action_for(&press("cmd-9")), Some(Action::LastTab));
    assert_eq!(map.action_for(&press("cmd-w")), Some(Action::Close));
    assert_eq!(map.action_for(&press("cmd-0")), Some(Action::ZoomReset));
    assert_eq!(map.action_for(&press("cmd-shift-t")), Some(Action::NewTab));
    assert_eq!(
        map.to_json(),
        r#"{
  "new_tab": [
    "cmd-shift-t"
  ]
}"#
    );
    let mut map = Keymap::default();
    map.assign(Action::NextPane, Chord::parse("cmd-shift-p").unwrap());
    assert_eq!(Keymap::parse(&map.to_json()).unwrap(), map);
}

#[test]
fn assigning_a_taken_chord_moves_it_and_reset_takes_it_back() {
    let mut map = Keymap::default();
    let chord = Chord::parse("cmd-d").unwrap();
    assert_eq!(map.holder(Action::NewTab, &chord), Some(Action::SplitRight));
    map.assign(Action::NewTab, chord.clone());
    assert_eq!(map.action_for(&press("cmd-d")), Some(Action::NewTab));
    assert_eq!(map.action_for(&press("cmd-t")), None);
    assert!(map.chords(Action::SplitRight).is_empty());
    map.reset(Action::SplitRight);
    assert_eq!(map.action_for(&press("cmd-d")), Some(Action::SplitRight));
    assert!(map.chords(Action::NewTab).is_empty());
    map.reset(Action::NewTab);
    assert_eq!(map, Keymap::default());
}

#[test]
fn only_changes_are_written_and_read_back() {
    assert_eq!(Keymap::default().to_json(), "{}");
    let mut map = Keymap::default();
    map.assign(Action::NewTab, Chord::parse("cmd-shift-t").unwrap());
    map.assign(Action::SplitDown, Chord::parse("cmd-d").unwrap());
    let json = map.to_json();
    let read = Keymap::parse(&json).unwrap();
    assert_eq!(read, map);
    assert!(json.contains("new_tab") && json.contains("split_right"));
    assert!(!json.contains("tab_1"));
    // A file that names one action alone gets that chord moved off its default owner.
    let read = Keymap::parse(r#"{"new_tab":["cmd-d"]}"#).unwrap();
    assert_eq!(read.action_for(&press("cmd-d")), Some(Action::NewTab));
    assert!(read.chords(Action::SplitRight).is_empty());
}

#[test]
fn bad_files_are_refused_whole() {
    for text in [
        r#"{"new_tabb":["cmd-t"]}"#,
        r#"{"new_tab":"cmd-t"}"#,
        r#"{"new_tab":["ctrl-t"]}"#,
        r#"{"new_tab":["cmd-c"]}"#,
        r#"{"new_tab":["cmd-x"],"close":["cmd-x"]}"#,
        "[]",
    ] {
        assert!(Keymap::parse(text).is_err(), "{text}");
    }
    assert_eq!(Keymap::parse("{}").unwrap(), Keymap::default());
}

#[test]
fn without_a_command_key_ctrl_shift_is_command_and_ctrl_stays_the_terminal_s() {
    let chord = |text: &str| Chord::from_keystroke_where(&press(text), false).unwrap();
    assert_eq!(chord("ctrl-shift-t"), Chord::parse("cmd-t").unwrap());
    assert_eq!(
        chord("ctrl-shift-alt-d"),
        Chord::parse("cmd-shift-d").unwrap()
    );
    assert_eq!(
        chord("ctrl-alt-left"),
        Chord::parse("cmd-alt-left").unwrap()
    );
    assert_eq!(chord("ctrl-c"), Chord::parse("ctrl-c").unwrap());
    assert_eq!(chord("alt-b"), Chord::parse("alt-b").unwrap());
    assert_eq!(chord("shift-enter"), Chord::parse("shift-enter").unwrap());
    // With a command key nothing is borrowed.
    assert_eq!(
        Chord::from_keystroke_where(&press("ctrl-shift-t"), true).unwrap(),
        Chord::parse("ctrl-shift-t").unwrap()
    );
}

#[test]
fn what_is_shown_without_a_command_key_is_what_to_press() {
    let shown = |text: &str| Chord::parse(text).unwrap().display_in_words();
    assert_eq!(shown("cmd-t"), "Ctrl+Shift+T");
    assert_eq!(shown("cmd-shift-d"), "Ctrl+Shift+Alt+D");
    assert_eq!(shown("cmd-alt-left"), "Ctrl+Alt+←");
    assert_eq!(shown("ctrl-c"), "Ctrl+C");
    // Pressing what it says gives the chord back, for every default.
    for action in Keymap::actions() {
        for chord in action.defaults() {
            let chord = Chord::parse(chord).unwrap();
            let (ctrl, shift, alt) = match (chord.cmd, chord.shift, chord.alt) {
                (true, true, _) => (true, true, true),
                (true, false, true) => (true, false, true),
                (true, false, false) => (true, true, false),
                (false, shift, alt) => (chord.ctrl, shift, alt),
            };
            let mut k = press(&chord.key);
            k.modifiers.control = ctrl;
            k.modifiers.shift = shift;
            k.modifiers.alt = alt;
            assert_eq!(
                Chord::from_keystroke_where(&k, false).as_ref(),
                Some(&chord)
            );
        }
    }
}
