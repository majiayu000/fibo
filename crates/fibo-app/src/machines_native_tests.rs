use super::*;

/// Once a machine's rows come from its own workspace here, its phone list only says it
/// is online: two sources writing the same rows would flicker between two moments.
#[test]
fn a_list_leaves_rows_fed_from_the_machines_own_workspace() {
    let space = |id: &str| Space {
        id: id.into(),
        name: id.into(),
        cwd: String::new(),
    };
    let view = Mutex::new(View {
        spec: Spec::new("far".into()),
        link: Link::Connecting,
        machine: None,
        spaces: vec![space("native")],
        terminals: Vec::new(),
        native: true,
    });
    let list = |id: &str| List {
        machine: Machine {
            agent_names: Default::default(),
            id: "m".into(),
            name: "far".into(),
        },
        spaces: vec![space(id)],
        quiet: Vec::new(),
        terminals: Vec::new(),
        todos: Vec::new(),
    };
    assert!(
        apply(&view, None, list("phone")),
        "coming online is a change"
    );
    let v = view.lock().unwrap().clone();
    assert_eq!(v.link, Link::Online);
    assert_eq!(
        v.spaces,
        vec![space("native")],
        "the phone list wrote over the rows"
    );
    assert!(
        !apply(&view, None, list("phone 2")),
        "nothing drawn changed"
    );
    // Not fed yet: the phone list is all there is.
    view.lock().unwrap().native = false;
    assert!(apply(&view, None, list("phone")));
    assert_eq!(view.lock().unwrap().spaces, vec![space("phone")]);
}
