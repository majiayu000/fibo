use super::*;
use crate::sample::every_state;

fn same(a: &Workspace, b: &Workspace) -> bool {
    serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
}
/// Sends `next` against what `held` was sent as, as the host and a window do.
fn roundtrip(held: &Workspace, index: &Index, next: &Workspace) -> (Delta, Workspace, Index) {
    let (delta, after) = diff(next.clone(), Some(index));
    let wire: Delta = serde_json::from_slice(&serde_json::to_vec(&delta).unwrap()).unwrap();
    let applied = apply(wire, Some(held)).unwrap();
    (delta, applied, after)
}

#[test]
fn only_what_changed_goes_and_the_window_ends_with_the_whole() {
    let first = every_state();
    let (whole, index) = diff(first.clone(), None);
    assert_eq!(whole.base, None);
    let held = apply(whole, None).unwrap();
    assert!(same(&held, &first));

    let mut next = first.clone();
    next.revision += 1;
    let session = next.agent_sessions[1].id.clone();
    next.content.get_mut(&session).unwrap().title = "改了".into();
    next.agent_sessions[2].updated += 5;
    let (delta, applied, _) = roundtrip(&held, &index, &next);
    assert_eq!(delta.base, Some(index.stamp));
    assert_eq!(delta.content.keys().collect::<Vec<_>>(), [&session]);
    assert_eq!(delta.sessions.len(), 1);
    assert_eq!(delta.order, None);
    assert!(same(&applied, &next));
}

#[test]
fn a_session_gone_new_or_moved_is_followed() {
    let first = every_state();
    let (whole, index) = diff(first.clone(), None);
    let held = apply(whole, None).unwrap();
    let mut next = first.clone();
    let gone = next.agent_sessions.remove(0);
    next.content.remove(&gone.id);
    next.agent_sessions.swap(0, 1);
    let mut new = next.agent_sessions[0].clone();
    new.id = "new".into();
    next.agent_sessions.push(new);
    let (delta, applied, _) = roundtrip(&held, &index, &next);
    assert_eq!(delta.gone, [gone.id]);
    assert!(delta.order.is_some());
    assert!(same(&applied, &next));
}

#[test]
fn nothing_changed_sends_no_session() {
    let first = every_state();
    let (_, index) = diff(first.clone(), None);
    let (delta, again) = diff(first, Some(&index));
    assert!(delta.sessions.is_empty() && delta.content.is_empty() && delta.gone.is_empty());
    assert_eq!(again.stamp, index.stamp);
}

#[test]
fn a_delta_needs_its_base_and_one_id_twice_goes_whole() {
    let mut first = every_state();
    let (_, index) = diff(first.clone(), None);
    first.revision += 1;
    let (delta, _) = diff(first.clone(), Some(&index));
    assert!(apply(delta, None).is_err());
    let twice = first.agent_sessions[0].clone();
    first.agent_sessions.push(twice);
    let (delta, _) = diff(first.clone(), Some(&index));
    assert_eq!(delta.base, None);
    assert!(same(&apply(delta, None).unwrap(), &first));
}
