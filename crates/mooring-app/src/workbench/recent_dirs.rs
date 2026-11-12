//! A new space's empty tab offers the folders you worked in lately: one press makes it the
//! space's folder, so a fresh space does not have to start in home or wait for a 设置目录….
use super::*;
use std::{cell::RefCell, rc::Rc};

/// How many folders the empty tab lists.
const SHOWN: usize = 5;

/// The list as of a workspace revision; the empty tab draws each frame, the list only
/// changes with the workspace.
#[derive(Default)]
pub(super) struct RecentDirs(RefCell<Option<(Key, Rc<Vec<String>>)>>);
/// The workspace revision and how many sessions it holds.
type Key = (u64, usize);

/// The folders worth offering, from core's order: existing ones, home left out (a space
/// with no folder starts there already).
pub(super) fn offered(w: &Workspace, home: Option<&str>) -> Vec<String> {
    (w.recent_dirs().into_iter())
        .filter(|d| Some(*d) != home.map(|h| h.trim_end_matches('/')) && *d != "/")
        .filter(|d| Path::new(d).is_dir())
        .take(SHOWN)
        .map(str::to_owned)
        .collect()
}

impl Workbench {
    fn recent_dirs(&self) -> Rc<Vec<String>> {
        let key = (self.workspace.revision, self.workspace.agent_sessions.len());
        let mut cache = self.recent_dirs.0.borrow_mut();
        if let Some((at, dirs)) = cache.as_ref()
            && *at == key
        {
            return Rc::clone(dirs);
        }
        let home = mooring_platform::dirs::home_str();
        let dirs = Rc::new(offered(&self.workspace, home.as_deref()));
        *cache = Some((key, Rc::clone(&dirs)));
        dirs
    }
    /// Under a space with no folder and nothing remembered: 「从最近的目录开始」 and the
    /// folders, then 其他目录… for the system picker.
    pub(super) fn recent_dirs_view(
        &self,
        space: &Space,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.workspace.space_start_dir(&space.id).is_empty() {
            return None;
        }
        let dirs = self.recent_dirs();
        let mut rows = vec![];
        for (n, dir) in dirs.iter().enumerate() {
            let pinned = Space {
                cwd: dir.clone(),
                ..space.clone()
            };
            rows.push(
                button(format!("empty-tab-dir-{n}"), home_relative(dir))
                    .w_full()
                    .justify_start()
                    .border_0()
                    .text_size(px(T::SMALL))
                    .font(T::mono(cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit(Edit::UpdateSpace(pinned.clone()), cx);
                    }))
                    .into_any_element(),
            );
        }
        let other = space.id.clone();
        rows.push(
            button("empty-tab-dir-other", t!("其他目录…"))
                .w_full()
                .justify_start()
                .border_0()
                .text_size(px(T::SMALL))
                .text_color(T::color(T::link()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.choose_space_dir(Some(other.clone()), window, cx);
                }))
                .into_any_element(),
        );
        Some(
            div()
                .w(px(T::HERE_WIDTH))
                .flex()
                .flex_col()
                .gap(px(T::S2))
                .child(label(t!("选一个目录作为这个空间的目录")))
                .children(rows)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_folders_first_each_once_home_and_gone_left_out() {
        let base = std::env::temp_dir().join(format!("kl-rd-{}", std::process::id()));
        let dir = |n: &str| {
            let d = base.join(n);
            std::fs::create_dir_all(&d).unwrap();
            d.to_string_lossy().into_owned()
        };
        let (old, new, pinned, last, home) =
            (dir("old"), dir("new"), dir("pin"), dir("last"), dir("home"));
        let mut w = Workspace::default();
        let session = |id: &str, updated: u64, cwd: &str| AgentSession {
            id: id.into(),
            runtime: Agent::Codex,
            cli_session_id: id.into(),
            terminal: None,
            current: false,
            task: None,
            outcome: Outcome::Completed,
            updated,
            cwd: cwd.into(),
            transcript: String::new(),
            binding: None,
            name: String::new(),
        };
        w.agent_sessions = vec![
            session("a", 1, &old),
            session("b", 5, &format!("{new}/")),
            session("c", 3, &old),
            session("d", 9, &home),
            session("e", 8, "/nonexistent/kl-gone"),
        ];
        w.spaces.push(Space {
            id: "p".into(),
            name: "p".into(),
            cwd: pinned.clone(),
        });
        w.space_last_cwd.insert("q".into(), last.clone());
        assert_eq!(offered(&w, Some(&home)), [new, old, pinned, last]);
        let _ = std::fs::remove_dir_all(base);
    }
}
