use super::*;

fn device(id: &str, role: Role, kicked: u64) -> Device {
    serde_json::from_value(json!({
        "id": id, "name": id, "key": "", "role": role, "paired": 0, "kicked": kicked,
    }))
    .unwrap()
}

#[test]
fn each_reason_a_session_ends_has_its_own_close_code() {
    let since = 1_000;
    let now = [device("a", Role::Full, 0), device("b", Role::View, 0)];
    assert_eq!(dropped(&now, "a", Role::Full, since), None);

    // Unpaired: the phone must pair again.
    assert_eq!(
        dropped(&now, "gone", Role::Full, since),
        Some(Dropped::Unpaired)
    );
    assert_eq!(Dropped::Unpaired.close().0, close::UNPAIRED);

    // "Disconnect" after the session opened ends it; one from before does not.
    let kicked = [device("a", Role::Full, since + 1)];
    assert_eq!(
        dropped(&kicked, "a", Role::Full, since),
        Some(Dropped::Kicked)
    );
    assert_eq!(Dropped::Kicked.close().0, close::KICKED);
    let earlier = [device("a", Role::Full, since - 1)];
    assert_eq!(dropped(&earlier, "a", Role::Full, since), None);

    // A new role: reconnect to get it.
    assert_eq!(
        dropped(&now, "b", Role::Full, since),
        Some(Dropped::RoleChanged)
    );
    assert_eq!(Dropped::RoleChanged.close().0, close::ROLE_CHANGED);

    // Three different codes; only unpairing is the "refused" one.
    let codes = [Dropped::Unpaired, Dropped::Kicked, Dropped::RoleChanged].map(|d| d.close().0);
    assert_eq!(codes, [4401, 4410, 4411]);
}
