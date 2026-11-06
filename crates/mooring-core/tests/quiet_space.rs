//! 先不管这个空间 (docs/quiet-space/spec.md): nothing in a space put aside is lost or
//! stopped, and none of it asks for you; taking it back returns all of it.
use mooring_core::content::now;
use mooring_core::model::*;
use mooring_core::sample::every_state;
use mooring_core::status::{self, NEEDS_YOU};
use mooring_core::status_group;

/// The sample's every state in "space", beside a second space to stay attended to.
fn two_spaces() -> Workspace {
    let mut w = every_state();
    w.spaces.push(Space {
        id: "other".into(),
        name: "另一个".into(),
        cwd: "/other".into(),
    });
    w
}
fn set(w: &mut Workspace, id: &str, quiet: bool) -> Result<(), String> {
    w.edit(Edit::SetSpaceQuiet {
        id: id.into(),
        quiet,
    })
}
fn needs(w: &Workspace) -> usize {
    let at = now();
    w.agent_sessions
        .iter()
        .filter(|s| w.needs_you(s, at))
        .count()
}

#[test]
fn a_space_put_aside_asks_for_nothing_and_comes_back_whole() {
    let mut w = two_spaces();
    let at = now();
    let before = needs(&w);
    assert!(before > 0, "the sample has sessions asking for you");
    let columns = |w: &Workspace| -> usize {
        (w.agent_sessions.iter())
            .filter(|s| status_group::column(w, s, at).is_some())
            .count()
    };
    let on_board = columns(&w);
    assert!(on_board > 0);
    let todos = status::unfinished_todos(&w, at).len();
    assert!(todos > 0);
    let (sessions, terminals, tabs, tasks) = (
        w.agent_sessions.clone(),
        w.terminals.clone(),
        w.tabs.clone(),
        w.tasks.clone(),
    );
    let states: Vec<_> = (w.agent_sessions.iter())
        .map(|s| status::of(&w, s, at))
        .collect();

    set(&mut w, "space", true).unwrap();
    assert_eq!(needs(&w), 0, "{NEEDS_YOU} counts none of them");
    assert_eq!(columns(&w), 0, "the board shows none of them");
    assert!(status::unfinished_todos(&w, at).is_empty());
    assert!(status::open_todos(&w, at).is_empty());
    // What waits there still knows it waits: its gray line counts this.
    assert!(
        (w.agent_sessions.iter()).any(|s| status::asks_for_you(&w, s, at)),
        "a question put aside is still a question"
    );
    // A state word is not changed by putting it aside: what runs, runs.
    let quiet_states: Vec<_> = (w.agent_sessions.iter())
        .map(|s| status::of(&w, s, at))
        .collect();
    assert_eq!(states, quiet_states);

    set(&mut w, "space", false).unwrap();
    assert_eq!(needs(&w), before);
    assert_eq!(columns(&w), on_board);
    assert_eq!(status::unfinished_todos(&w, at).len(), todos);
    assert_eq!(w.agent_sessions, sessions);
    assert_eq!(w.terminals, terminals);
    assert_eq!(w.tabs, tabs);
    assert_eq!(w.tasks, tasks);
}

#[test]
fn it_follows_the_space_a_session_is_listed_under() {
    let mut w = two_spaces();
    // Listed under the todo's space even if its terminal is elsewhere: one owner of "where".
    let moved = w.agent_sessions[0].clone();
    assert_eq!(w.session_space(&moved), Some("space"));
    set(&mut w, "other", true).unwrap();
    assert!(!w.session_quiet(&moved));
    set(&mut w, "other", false).unwrap();
    set(&mut w, "space", true).unwrap();
    assert!(w.session_quiet(&moved));
}

#[test]
fn one_space_always_stays_attended_to() {
    let mut w = two_spaces();
    assert_eq!(w.attended_spaces(), 2);
    set(&mut w, "other", true).unwrap();
    assert!(
        set(&mut w, "space", true).is_err(),
        "the last attended space stays"
    );
    // Setting what is already so changes nothing and is no error.
    set(&mut w, "other", true).unwrap();
    assert!(set(&mut w, "nope", true).is_err());
    // Removing the last attended space would leave only quiet ones.
    let refused = w.clone().edit(Edit::RemoveSpace { id: "space".into() });
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn removing_a_space_put_aside_forgets_it() {
    let mut w = two_spaces();
    set(&mut w, "other", true).unwrap();
    w.edit(Edit::RemoveSpace { id: "other".into() }).unwrap();
    assert!(w.quiet_spaces.is_empty());
}

#[test]
fn old_snapshots_read_as_nothing_put_aside() {
    let mut value = serde_json::to_value(two_spaces()).unwrap();
    value.as_object_mut().unwrap().remove("quiet_spaces");
    let w: Workspace = serde_json::from_value(value).unwrap();
    assert!(w.quiet_spaces.is_empty());
}
