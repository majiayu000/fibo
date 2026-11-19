use super::*;
use crate::sample::every_state;

fn same(a: &Workspace, b: &Workspace) -> bool {
    serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
}
fn whole(w: &Workspace) -> (Kept, Index) {
    let (delta, index) = diff(w.clone(), None, true).unwrap();
    (apply(delta, None).unwrap(), index)
}
/// Sends `next` against what `held` was sent as, as the host and a window do.
fn roundtrip(held: &Kept, index: &Index, next: &Workspace, fields: bool) -> (Delta, Kept, Index) {
    let (delta, after) = diff(next.clone(), Some(index), fields).unwrap();
    let wire: Delta = serde_json::from_slice(&serde_json::to_vec(&delta).unwrap()).unwrap();
    let applied = apply(wire, Some(held)).unwrap();
    (delta, applied, after)
}

#[test]
fn only_what_changed_goes_and_the_window_ends_with_the_whole() {
    let first = every_state();
    let (whole, index) = diff(first.clone(), None, true).unwrap();
    assert_eq!(whole.base, None);
    let held = apply(whole, None).unwrap();
    assert!(same(&held.workspace, &first));

    let mut next = first.clone();
    next.revision += 1;
    let session = next.agent_sessions[1].id.clone();
    next.content
        .get_mut(&session)
        .map(std::sync::Arc::make_mut)
        .unwrap()
        .title = "改了".into();
    next.agent_sessions[2].updated += 5;
    let (delta, applied, _) = roundtrip(&held, &index, &next, false);
    assert_eq!(delta.base, Some(index.stamp));
    assert_eq!(delta.content.keys().collect::<Vec<_>>(), [&session]);
    assert_eq!(delta.sessions.len(), 1);
    assert_eq!(delta.order, None);
    assert!(
        !delta.fields && delta.rest.contains_key("tabs"),
        "Brief: rest whole"
    );
    assert!(same(&applied.workspace, &next));
}

/// On agentos the rest was 352 KB on every refresh, terminals most of it, and between
/// refreshes none of it changed.
#[test]
fn of_the_rest_only_the_fields_that_changed_go() {
    let first = every_state();
    let (held, index) = whole(&first);
    let mut next = first.clone();
    next.revision += 1;
    let (delta, applied, _) = roundtrip(&held, &index, &next, true);
    assert_eq!(delta.rest.keys().collect::<Vec<_>>(), ["revision"]);
    assert!(same(&applied.workspace, &next));
}

/// A field left out of a `BriefAll` reply is read as unchanged, so none may be left out
/// for being empty.
#[test]
fn every_field_of_a_workspace_is_written_even_empty() {
    let fields = |w: &Workspace| {
        let v = serde_json::to_value(w).unwrap();
        v.as_object().unwrap().keys().cloned().collect::<Vec<_>>()
    };
    assert_eq!(fields(&Workspace::default()), fields(&every_state()));
}

#[test]
fn a_session_gone_new_or_moved_is_followed() {
    let first = every_state();
    let (held, index) = whole(&first);
    let mut next = first.clone();
    let gone = next.agent_sessions.remove(0);
    next.content.remove(&gone.id);
    next.agent_sessions.swap(0, 1);
    let mut new = next.agent_sessions[0].clone();
    new.id = "new".into();
    next.agent_sessions.push(new);
    let (delta, applied, _) = roundtrip(&held, &index, &next, true);
    assert_eq!(delta.gone, [gone.id]);
    assert!(delta.order.is_some());
    assert!(same(&applied.workspace, &next));
}

#[test]
fn nothing_changed_sends_no_session() {
    let first = every_state();
    let (_, index) = diff(first.clone(), None, true).unwrap();
    let (delta, again) = diff(first, Some(&index), true).unwrap();
    assert!(delta.sessions.is_empty() && delta.content.is_empty() && delta.gone.is_empty());
    assert!(delta.rest.is_empty());
    assert_eq!(again.stamp, index.stamp);
}

#[test]
fn a_delta_needs_its_base_and_one_id_twice_goes_whole() {
    let mut first = every_state();
    let (_, index) = diff(first.clone(), None, true).unwrap();
    first.revision += 1;
    let (delta, _) = diff(first.clone(), Some(&index), true).unwrap();
    assert!(apply(delta, None).is_err());
    let twice = first.agent_sessions[0].clone();
    first.agent_sessions.push(twice);
    let (delta, _) = diff(first.clone(), Some(&index), true).unwrap();
    assert_eq!(delta.base, None);
    assert!(same(&apply(delta, None).unwrap().workspace, &first));
}

/// A window before `BriefAll` reads the rest of a `Brief` reply as a workspace.
#[test]
fn a_brief_reply_still_reads_as_the_old_delta() {
    #[derive(Deserialize)]
    struct Old {
        rest: Workspace,
    }
    let first = every_state();
    let (_, index) = diff(first.clone(), None, false).unwrap();
    let (delta, _) = diff(first.clone(), Some(&index), false).unwrap();
    let wire = serde_json::to_vec(&delta).unwrap();
    let old: Old = serde_json::from_slice(&wire).unwrap();
    assert_eq!(old.rest.tabs.len(), first.tabs.len());
    assert!(!String::from_utf8(wire).unwrap().contains("\"fields\""));
}

#[test]
fn remembered_hashes_give_the_same_delta_as_fresh_ones() {
    let first = every_state();
    let mut hashes = Hashes::default();
    let (_, index) = diff_with(first.clone(), None, true, &mut hashes).unwrap();
    let mut next = first.clone();
    let session = next.agent_sessions[1].id.clone();
    next.content
        .get_mut(&session)
        .map(std::sync::Arc::make_mut)
        .unwrap()
        .title = "改了".into();
    let gone = next.agent_sessions[0].id.clone();
    next.content.remove(&gone);
    let (kept, kept_index) = diff_with(next.clone(), Some(&index), true, &mut hashes).unwrap();
    let (fresh, fresh_index) = diff(next.clone(), Some(&index), true).unwrap();
    assert_eq!(kept_index.stamp, fresh_index.stamp);
    assert_eq!(
        serde_json::to_value(&kept).unwrap(),
        serde_json::to_value(&fresh).unwrap()
    );
    assert_eq!(
        kept.content.keys().collect::<Vec<_>>(),
        [&session],
        "only what changed"
    );
    assert!(
        !hashes.content.contains_key(&gone),
        "forgets content that left"
    );
}

#[test]
fn remembered_session_hashes_give_the_same_delta_as_fresh_ones() {
    let first = every_state();
    let mut hashes = Hashes::default();
    let (_, index) = diff_with(first.clone(), None, true, &mut hashes).unwrap();
    let mut next = first.clone();
    next.agent_sessions[1].updated += 1;
    let changed = next.agent_sessions[1].id.clone();
    let gone = next.agent_sessions.remove(0).id;
    let (kept, kept_index) = diff_with(next.clone(), Some(&index), true, &mut hashes).unwrap();
    let (fresh, fresh_index) = diff(next.clone(), Some(&index), true).unwrap();
    assert_eq!(kept_index.stamp, fresh_index.stamp);
    assert_eq!(
        serde_json::to_value(&kept).unwrap(),
        serde_json::to_value(&fresh).unwrap()
    );
    let sent: Vec<_> = kept.sessions.iter().map(|s| &s.id).collect();
    assert_eq!(sent, [&changed], "only the session that changed");
    assert!(
        !hashes.sessions.contains_key(&gone),
        "forgets sessions that left"
    );
    assert_eq!(
        hashes.sessions.len(),
        next.agent_sessions.len(),
        "remembers every session sent, so the next reply writes none out again"
    );
    // Unchanged again: the same stamp, nothing sent.
    let (again, _) = diff_with(next.clone(), Some(&kept_index), true, &mut hashes).unwrap();
    assert!(again.sessions.is_empty());
    assert_eq!(again.stamp, kept_index.stamp);
}

/// A refresh copied every session's content twice in the window and once in the host
/// (11-19: 62 M instructions a refresh on a 1621-session install). Content no one changed
/// stays one allocation from the host's snapshot through the window's applied workspace.
#[test]
fn content_no_one_changed_is_shared_not_copied() {
    let first = every_state();
    let mut hashes = Hashes::default();
    let (delta, index) = diff_with(first.clone(), None, true, &mut hashes).unwrap();
    let held = apply(delta, None).unwrap();
    let mut next = held.workspace.clone();
    let changed = next.agent_sessions[1].id.clone();
    std::sync::Arc::make_mut(next.content.get_mut(&changed).unwrap()).title = "改了".into();
    let (delta, _) = diff_with(next.clone(), Some(&index), true, &mut hashes).unwrap();
    assert_eq!(delta.content.keys().collect::<Vec<_>>(), [&changed]);
    let after = apply(delta, Some(&held)).unwrap();
    for (id, c) in &after.workspace.content {
        let before = &held.workspace.content[id];
        assert_eq!(
            std::sync::Arc::ptr_eq(c, before),
            *id != changed,
            "{id}: only the changed content is a new allocation"
        );
    }
}
