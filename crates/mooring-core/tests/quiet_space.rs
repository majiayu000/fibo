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

/// What happens in a space while it is put aside. Each runs the same on both copies.
type Event = fn(&mut Workspace, &str);

fn turn(w: &mut Workspace, id: &str, turn: mooring_core::content::Turn) {
    if let Some(c) = w.content.get_mut(id) {
        c.turn = turn;
        c.waiting = if turn == mooring_core::content::Turn::Waiting {
            "继续吗？".into()
        } else {
            String::new()
        };
    }
}

/// Taking a space back gives exactly what never putting it aside gives, whatever each
/// conversation was doing and whatever happened meanwhile, a week later: 先不管 is only a
/// mask over the state, never a change to it.
#[test]
fn coming_back_gives_the_state_it_would_have_had_anyway() {
    use mooring_core::content::Turn;
    let events: [(&str, Event); 9] = [
        ("nothing", |_, _| ()),
        ("finishes", |w, id| turn(w, id, Turn::Completed)),
        ("fails", |w, id| turn(w, id, Turn::Error)),
        ("asks", |w, id| turn(w, id, Turn::Waiting)),
        ("runs", |w, id| turn(w, id, Turn::Running)),
        ("interrupted", |w, id| turn(w, id, Turn::Interrupted)),
        ("terminal ends", |w, id| {
            let terminal = (w.agent_sessions.iter())
                .find(|s| s.id == id)
                .and_then(|s| s.terminal.clone());
            for t in w
                .terminals
                .iter_mut()
                .filter(|t| Some(&t.id) == terminal.as_ref())
            {
                t.state = ProcessState::Exited;
            }
        }),
        ("marked done", |w, id| {
            let _ = w.edit(Edit::SetOutcome {
                id: id.into(),
                outcome: Outcome::Completed,
                seen: None,
            });
        }),
        ("put off until later, which comes", |w, id| {
            let until = now() + 3600;
            let _ = w.edit(Edit::SetOutcome {
                id: id.into(),
                outcome: Outcome::Deferred {
                    reason: "晚点".into(),
                    until,
                },
                seen: None,
            });
            w.expire_outcomes(until);
        }),
    ];
    let start = two_spaces();
    let ids: Vec<String> = start.agent_sessions.iter().map(|s| s.id.clone()).collect();
    assert!(ids.len() > 5, "every sample state");
    // Right away, past 3 days (stale_days), and a week on.
    for later in [0, 4 * 86_400, 8 * 86_400] {
        let at = now() + later;
        for (name, event) in events {
            for id in &ids {
                let mut never = start.clone();
                let mut aside = start.clone();
                set(&mut aside, "space", true).unwrap();
                event(&mut never, id);
                event(&mut aside, id);
                let session = |w: &Workspace| {
                    w.agent_sessions
                        .iter()
                        .find(|s| &s.id == id)
                        .unwrap()
                        .clone()
                };
                let case = format!("{id} {name} +{later}s");
                // While aside the state still moves, and nothing asks.
                let (a, n) = (session(&aside), session(&never));
                assert_eq!(
                    status::of(&aside, &a, at),
                    status::of(&never, &n, at),
                    "{case}"
                );
                assert!(!aside.needs_you(&a, at), "{case}");
                assert_eq!(status_group::column(&aside, &a, at), None, "{case}");
                assert_eq!(
                    status::asks_for_you(&aside, &a, at),
                    never.needs_you(&n, at),
                    "the line counts what would ask: {case}"
                );
                set(&mut aside, "space", false).unwrap();
                let a = session(&aside);
                assert_eq!(
                    status::of(&aside, &a, at),
                    status::of(&never, &n, at),
                    "{case}"
                );
                assert_eq!(aside.needs_you(&a, at), never.needs_you(&n, at), "{case}");
                assert_eq!(
                    status_group::column(&aside, &a, at),
                    status_group::column(&never, &n, at),
                    "{case}"
                );
                assert_eq!(a.outcome, n.outcome, "{case}");
                assert_eq!(aside.content.get(id), never.content.get(id), "{case}");
            }
        }
    }
}

/// What a week away shows: a live conversation still waits for your check, and one whose
/// terminal ended has gone stale, as either would without being put aside.
#[test]
fn a_week_aside_keeps_a_live_turn_waiting_and_lets_an_ended_one_go_stale() {
    let mut w = two_spaces();
    set(&mut w, "space", true).unwrap();
    let at = now() + 8 * 86_400;
    let of = |w: &Workspace, id: &str| {
        status::of(w, w.agent_sessions.iter().find(|s| s.id == id).unwrap(), at)
    };
    assert_eq!(of(&w, "review"), status::Status::Review);
    assert_eq!(of(&w, "exited"), status::Status::Stale);
}
