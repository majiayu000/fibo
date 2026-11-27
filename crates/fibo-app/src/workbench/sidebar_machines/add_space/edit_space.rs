//! Another machine's space renamed or given a folder from this window, as this machine's own
//! space menu does (重命名, 设置目录…), and a folder made there for a space: the add-space
//! panel with the space it edits, over `updatespace` and `mkdir`. The rows here say first
//! what Enter does, so a name never quietly turns into a folder's.
use super::*;

/// What the panel sends for `make`: a new space, or a change to the space `editing`.
pub(super) fn space_op(make: Make, editing: Option<String>) -> Op {
    let request = Some(fibo_core::protocol::RequestId::fresh().id);
    let (path, name) = match make {
        Make::Folder(path) | Make::NewFolder(path) => (Some(path), None),
        Make::Named(name) => (None, Some(name)),
    };
    match editing {
        Some(space) => Op::UpdateSpace {
            space,
            name,
            path,
            request,
        },
        None => Op::AddSpace {
            path,
            name,
            request,
        },
    }
}

/// `make` if its host can do it for this panel, else why not.
pub(super) fn allowed(adding: &AddingSpace, make: Make) -> Result<Make, &'static str> {
    if !adding.brief.said_hello() {
        Err(t!("还在连那台电脑，稍等再试"))
    } else if adding.editing.is_some() && !adding.brief.lists("updatespace") {
        Err(t!("那台电脑上的 Fibo 太旧，还改不了空间；更新它之后再试"))
    } else if matches!(make, Make::NewFolder(_)) && !adding.brief.lists("mkdir") {
        Err(t!("那台电脑上的 Fibo 太旧，还建不了文件夹；更新它之后再试"))
    } else {
        Ok(make)
    }
}

/// What the text typed would do, as the rows above the folders say it: the words, whether
/// Enter does it, and what it makes. None when the folders listed say it already.
fn enter_choice(adding: &AddingSpace, spaces: &[Space]) -> Option<(String, bool, Make)> {
    let names = adding.brief.lists("spacename") || adding.editing.is_some();
    match target(&adding.text, &[], names) {
        Ok(Make::Named(name)) => {
            let words = match &adding.editing {
                Some(space) if space.name == name => return None,
                Some(_) => t!("改名为「{name}」", name),
                None if spaces.iter().any(|s| s.name == name) => t!("已有空间「{name}」", name),
                None => t!("新建空间「{name}」", name),
            };
            Some((words, true, Make::Named(name)))
        }
        Ok(Make::Folder(path)) => {
            let listed = (adding.offered.iter())
                .any(|p| same_folder(p, &path) || same_folder(&shorter(p, &adding.home), &path));
            if listed {
                (adding.editing.is_some())
                    .then(|| (t!("目录设为 {path}", path), true, Make::Folder(path)))
            } else if !split_path(&path).1.is_empty() && adding.brief.lists("mkdir") {
                Some((t!("新建文件夹 {path}", path), false, Make::NewFolder(path)))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(feature = "visual-test")]
pub(super) fn visual_choice(
    adding: &AddingSpace,
    spaces: &[Space],
) -> Option<(String, bool, Make)> {
    enter_choice(adding, spaces)
}

impl Workbench {
    /// Right-click on another machine's space chip: the panel, its name in the field.
    pub(in crate::workbench) fn begin_edit_machine_space(
        &mut self,
        index: usize,
        space: Space,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_space_panel(index, Some(space), window, cx);
    }
    /// The row above the folders for what the text typed would do, if anything.
    pub(super) fn enter_row(
        &self,
        adding: &AddingSpace,
        spaces: &[Space],
        cx: &Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let (words, enter, make) = enter_choice(adding, spaces)?;
        Some(
            div()
                .id("machine-space-enter")
                .role(gpui::accesskit::Role::Button)
                .aria_label(words.clone())
                .h(px(ROW))
                .flex_shrink_0()
                .px(px(T::S6))
                .flex()
                .items_center()
                .rounded(px(T::S4))
                .when(enter, |d| d.bg(T::color(F::selected())))
                .hover(|s| s.bg(T::color(F::selected())))
                .cursor_pointer()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(F::TIME))
                        .text_color(T::color(F::title()))
                        .child(words),
                )
                .when(enter, |d| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(F::TAG))
                            .text_color(T::color(F::third()))
                            .child(t!("回车")),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.submit_machine_space(Some(make.clone()), cx);
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(make: Make, editing: Option<&str>) -> serde_json::Value {
        let mut v = serde_json::to_value(space_op(make, editing.map(str::to_owned))).unwrap();
        v.as_object_mut().unwrap().remove("request");
        v
    }
    #[test]
    fn a_new_space_is_added_and_an_edited_one_updated() {
        use serde_json::json;
        let named = || Make::Named("前端".into());
        assert_eq!(
            op(named(), None),
            json!({ "op": "addspace", "name": "前端" })
        );
        assert_eq!(
            op(named(), Some("s1")),
            json!({ "op": "updatespace", "space": "s1", "name": "前端" })
        );
        assert_eq!(
            op(Make::NewFolder("~/x".into()), Some("s1")),
            json!({ "op": "updatespace", "space": "s1", "path": "~/x" })
        );
        assert_eq!(
            op(Make::Folder("/a".into()), None),
            json!({ "op": "addspace", "path": "/a" })
        );
    }
}
