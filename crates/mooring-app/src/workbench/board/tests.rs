use super::*;

#[test]
fn branches_color_by_their_prefix() {
    assert_eq!(branch_hue("feat/peek"), Some(T::BRANCH_FEAT));
    assert_eq!(branch_hue("fix/timer"), branch_hue("hotfix/x"));
    assert_eq!(branch_hue("main"), None);
    assert_eq!(branch_hue("wip-thing"), None);
}

#[test]
fn a_space_keeps_its_gradient() {
    assert_eq!(mesh_index("mooring"), mesh_index("mooring"));
    let distinct: std::collections::HashSet<_> = ["a", "b", "c", "d", "e", "f", "g", "h"]
        .iter()
        .map(|s| mesh_index(s))
        .collect();
    assert!(distinct.len() > 2, "spaces spread over the palettes");
}
fn column(ids: &[&str], extra: usize) -> ColumnList {
    ColumnList {
        state: gpui::ListState::new(
            ids.len() + extra,
            gpui::ListAlignment::Top,
            px(T::CARD_ESTIMATE),
        ),
        ids: ids.iter().map(|s| s.to_string()).collect(),
        revision: 1,
    }
}
#[test]
fn session_arrival_and_removal_preserve_the_visible_session() {
    let mut list = column(&["a", "b", "c", "d"], 0);
    list.state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(T::S12),
    });
    list.sync(&["new", "a", "b", "c", "d"].map(String::from), 2);
    let top = list.state.logical_scroll_top();
    assert_eq!(list.ids[top.item_ix], "c");
    assert_eq!(top.offset_in_item, px(T::S12));
    list.sync(&["b", "c", "d"].map(String::from), 3);
    let top = list.state.logical_scroll_top();
    assert_eq!(list.ids[top.item_ix], "c");
    assert_eq!(top.offset_in_item, px(T::S12));
}
#[test]
fn content_updates_keep_the_scroll_anchor() {
    let mut list = column(&["a", "b", "c"], 0);
    list.state.scroll_to(gpui::ListOffset {
        item_ix: 1,
        offset_in_item: px(T::S8),
    });
    list.sync(&["a", "b", "c"].map(String::from), 2);
    let top = list.state.logical_scroll_top();
    assert_eq!(top.item_ix, 1);
    assert_eq!(top.offset_in_item, px(T::S8));
    assert_eq!(list.state.item_count(), 3);
}
#[test]
fn todo_add_row_survives_filtering_to_empty_and_back() {
    let mut list = column(&["a", "b"], 1);
    list.sync(&[], 2);
    assert_eq!(list.state.item_count(), 1);
    list.sync(&["c".to_string()], 3);
    assert_eq!(list.state.item_count(), 2);
    assert_eq!(list.ids, ["c"]);
}
