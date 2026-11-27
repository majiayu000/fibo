//! 「换目录」 in another computer's workbench here. The Finder shows this machine's folders,
//! so a path there is typed instead: its host lists the folders in what is typed
//! (`ListDirs`), a press makes one the space's folder, and one not there yet can be made
//! there first (`MakeDir`). Enter never makes a folder: a typo would leave one behind.
use super::*;
use crate::ui::field::{Field, FieldEvent};
use fibo_core::protocol::{Reply, Request, rpc};
use std::time::Duration;

/// Typing settles for this long before its host is asked again.
const SETTLE: Duration = Duration::from_millis(150);
const SHOWN: usize = 6;

/// The field under 「换目录」 while a path there is typed.
pub(super) struct FolderField {
    space: String,
    field: Entity<Field>,
    _watch: [gpui::Subscription; 2],
    text: String,
    /// The folders in the typed path's folder, by full path, as its host listed them.
    listed: Vec<String>,
    error: Option<String>,
    /// Which question the next listing must answer: a later one makes it stale.
    asked: u64,
}

/// A path as typed splits into the folder to list and what to look for in it.
fn split(text: &str) -> (&str, &str) {
    match text.rfind('/') {
        Some(0) => ("/", &text[1..]),
        Some(at) => (&text[..at], &text[at + 1..]),
        None => (text, ""),
    }
}
fn leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
/// The listed folders whose name holds what follows the last `/`.
fn matching<'a>(listed: &'a [String], text: &str) -> Vec<&'a String> {
    let want = split(text).1.to_lowercase();
    (listed.iter())
        .filter(|p| leaf(p).to_lowercase().contains(&want))
        .take(SHOWN)
        .collect()
}
/// The folder to offer to make: what is typed, when it names one not listed there.
fn to_make(listed: &[String], text: &str) -> Option<String> {
    let name = split(text).1;
    let fresh = !name.is_empty() && !listed.iter().any(|p| leaf(p) == name);
    (fresh && (text.starts_with('/') || text.starts_with('~'))).then(|| text.to_owned())
}
/// A host built before `ListDirs` and `MakeDir` can't read the request at all.
fn refusal(error: &str) -> String {
    if error.starts_with("unsupported or invalid request") {
        t!("那台电脑上的 Fibo 太旧，还选不了那边的目录；更新它之后再试").into()
    } else {
        error.to_owned()
    }
}

impl Workbench {
    /// 「输入那台电脑上的路径…」: the field, starting at home there.
    fn begin_folder_field(&mut self, space: String, window: &mut Window, cx: &mut Context<Self>) {
        let text = "~/".to_owned();
        let field = cx.new(|cx| {
            Field::new(t!("那台电脑上的路径"), &text, cx)
                .inline()
                .placeholder("~/code/project")
        });
        let events = cx.subscribe(&field, |this, _, event, cx| match event {
            FieldEvent::Submit => this.submit_folder_field(cx),
            FieldEvent::Cancel => {
                this.layout.folder_field = None;
                cx.notify();
            }
            FieldEvent::ClickOut => {}
        });
        // Fields notify for a blinking cursor too: only a changed text asks the host again.
        let changes = cx.observe(&field, |this, field, cx| {
            let text = field.read(cx).text.trim().to_owned();
            let Some(typing) = &mut this.layout.folder_field else {
                return;
            };
            if typing.text != text {
                typing.text = text;
                typing.error = None;
                this.list_folders(true, cx);
                cx.notify();
            }
        });
        window.focus(&field.focus_handle(cx), cx);
        self.layout.folder_field = Some(FolderField {
            space,
            field,
            _watch: [events, changes],
            text,
            listed: Vec::new(),
            error: None,
            asked: 0,
        });
        self.list_folders(false, cx);
        cx.notify();
    }
    fn list_folders(&mut self, settle: bool, cx: &mut Context<Self>) {
        let socket = self.store.socket().to_owned();
        let Some(typing) = &mut self.layout.folder_field else {
            return;
        };
        typing.asked += 1;
        let asked = typing.asked;
        let path = split(&typing.text).0.to_owned();
        let timer = cx.background_executor().clone();
        let sent = cx.background_spawn(async move {
            if settle {
                timer.timer(SETTLE).await;
            }
            rpc(&socket, &Request::ListDirs { path })
        });
        cx.spawn(async move |this, cx| {
            let reply = sent.await;
            let _ = this.update(cx, |this, cx| {
                let Some(typing) = &mut this.layout.folder_field else {
                    return;
                };
                if typing.asked != asked {
                    return;
                }
                // A folder that is not there lists nothing; only a host that can't list says so.
                match reply {
                    Ok(Reply::Dirs(dirs)) => typing.listed = dirs,
                    Ok(_) => typing.listed.clear(),
                    Err(e) => {
                        typing.listed.clear();
                        let why = refusal(&e.to_string());
                        if why != e.to_string() {
                            typing.error = Some(why);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    /// Enter: the first folder offered.
    fn submit_folder_field(&mut self, cx: &mut Context<Self>) {
        let Some(typing) = &mut self.layout.folder_field else {
            return;
        };
        match matching(&typing.listed, &typing.text).first() {
            Some(path) => {
                let path = (*path).clone();
                self.folder_chosen(&path, cx);
            }
            None => {
                typing.error = Some(t!("那里没有这个目录").into());
                cx.notify();
            }
        }
    }
    /// The space takes `cwd` there as its folder, as a press on a recent one does.
    fn folder_chosen(&mut self, cwd: &str, cx: &mut Context<Self>) {
        let Some(typing) = self.layout.folder_field.take() else {
            return;
        };
        self.layout.choosing_dir = None;
        let Some(space) = self.workspace.spaces.iter().find(|s| s.id == typing.space) else {
            return;
        };
        let pinned = Space {
            cwd: cwd.to_owned(),
            ..space.clone()
        };
        self.space_dir_chosen(&pinned.id, &pinned.cwd);
        self.edit(Edit::UpdateSpace(pinned), cx);
        self.nav.pending_focus = true;
        cx.notify();
    }
    /// Makes `path` there, then makes it the space's folder.
    fn make_folder(&mut self, path: String, cx: &mut Context<Self>) {
        let socket = self.store.socket().to_owned();
        let made = cx.background_spawn(async move { rpc(&socket, &Request::MakeDir { path }) });
        cx.spawn(async move |this, cx| {
            let made = made.await;
            let _ = this.update(cx, |this, cx| match made {
                Ok(Reply::Text(path)) => this.folder_chosen(&path, cx),
                other => {
                    let why = match other {
                        Err(e) => refusal(&e.to_string()),
                        Ok(r) => format!("unexpected reply: {r:?}"),
                    };
                    if let Some(typing) = &mut this.layout.folder_field {
                        typing.error = Some(why);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Under 「换目录」 on another computer: the button that starts typing a path there, or
    /// the field with the folders it finds and the one it would make.
    pub(super) fn remote_folder_rows(
        &self,
        space: &Space,
        cx: &Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let typing = (self.layout.folder_field.as_ref()).filter(|t| t.space == space.id);
        let Some(typing) = typing else {
            let id = space.id.clone();
            return vec![
                button(
                    "empty-tab-dir-type",
                    t!("输入 {host} 上的路径…", host = self.host.name()),
                )
                .h(px(T::ROW * 0.8))
                .border_0()
                .px(px(T::S8))
                .text_size(px(T::SMALL))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_folder_field(id.clone(), window, cx)
                }))
                .into_any_element(),
            ];
        };
        let row = |id: String, words: String| {
            button(id, words)
                .h(px(T::ROW * 0.8))
                .border_0()
                .px(px(T::S8))
                .text_size(px(T::SMALL))
        };
        let mut rows = vec![
            div()
                .w(px(T::HERE_WIDTH))
                .h(px(T::ROW))
                .px(px(T::S8 - T::BORDER_WIDTH))
                .rounded(px(T::CONTROL_RADIUS))
                .border(px(T::BORDER_WIDTH))
                .border_color(T::color(T::focus()))
                .text_size(px(T::SMALL))
                .child(typing.field.clone())
                .into_any_element(),
        ];
        for (n, path) in matching(&typing.listed, &typing.text)
            .into_iter()
            .enumerate()
        {
            let chosen = path.clone();
            rows.push(
                row(
                    format!("empty-tab-dir-found-{n}"),
                    home_relative(&self.host, path),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.folder_chosen(&chosen, cx)))
                .into_any_element(),
            );
        }
        if let Some(path) = to_make(&typing.listed, &typing.text) {
            rows.push(
                row(
                    "empty-tab-dir-make".into(),
                    t!("新建文件夹 {path}", path = path.clone()),
                )
                .text_color(T::color(T::link()))
                .on_click(cx.listener(move |this, _, _, cx| this.make_folder(path.clone(), cx)))
                .into_any_element(),
            );
        }
        if let Some(why) = &typing.error {
            rows.push(
                label(why.clone())
                    .text_size(px(T::SMALL))
                    .text_color(Tone::Danger.colors().1)
                    .into_any_element(),
            );
        }
        rows
    }
}

#[cfg(feature = "visual-test")]
impl Workbench {
    /// Starts typing a path there for `space`, as its button does, and types `text`.
    pub fn visual_type_remote_folder(
        &mut self,
        space: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layout.folder_field.is_none() {
            self.layout.choosing_dir = Some(space.to_owned());
            self.begin_folder_field(space.to_owned(), window, cx);
        }
        if let Some(typing) = &self.layout.folder_field {
            typing.field.update(cx, |f, cx| {
                f.set_text(text);
                cx.notify();
            });
        }
    }
    /// What the field offers: the folders found, the one it would make, and what it says.
    pub fn visual_remote_folder(&self) -> Option<(Vec<String>, Option<String>, Option<String>)> {
        let t = self.layout.folder_field.as_ref()?;
        let found = matching(&t.listed, &t.text).into_iter().cloned().collect();
        Some((found, to_make(&t.listed, &t.text), t.error.clone()))
    }
    /// The folder space `id` has.
    pub fn visual_space_folder(&self, id: &str) -> Option<String> {
        (self.workspace.spaces.iter().find(|s| s.id == id)).map(|s| s.cwd.clone())
    }
    /// Enter, or a press on 「新建文件夹」.
    pub fn visual_choose_remote_folder(&mut self, make: bool, cx: &mut Context<Self>) {
        let made = (self.layout.folder_field.as_ref()).and_then(|t| to_make(&t.listed, &t.text));
        match made.filter(|_| make) {
            Some(path) => self.make_folder(path, cx),
            None => self.submit_folder_field(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_typed_lists_its_folder_and_offers_to_make_a_new_one_there() {
        let listed = ["/h/code/site".to_owned(), "/h/code/Web".to_owned()];
        assert_eq!(split("~/code/we"), ("~/code", "we"));
        assert_eq!(split("~/code/"), ("~/code", ""));
        assert_eq!(split("/opt"), ("/", "opt"));
        assert_eq!(matching(&listed, "~/code/we"), [&listed[1]]);
        assert_eq!(matching(&listed, "~/code/").len(), 2);
        assert_eq!(
            to_make(&listed, "~/code/new"),
            Some("~/code/new".to_owned())
        );
        assert_eq!(to_make(&listed, "~/code/site"), None, "already there");
        assert_eq!(to_make(&listed, "~/code/"), None, "no name yet");
        assert_eq!(to_make(&listed, "code/new"), None, "not a path");
    }
    #[test]
    fn an_old_host_is_named_as_too_old() {
        assert!(
            refusal("unsupported or invalid request: unknown variant `ListDirs`").contains("太旧")
        );
        assert_eq!(refusal("那里没有"), "那里没有");
    }
}
