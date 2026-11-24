//! The welcome page's folder (#519): where the first Agent starts, and 「选择项目文件夹…」
//! right there. The first space starts in home when the App was opened from the Finder or
//! the Dock (`default_space_dir`), and a space with no folder in the newest folder worked
//! in; for either, the welcome page offers the newest folder worked in that is a git work
//! tree instead, and starting from the page makes it the space's folder.
use super::*;
use std::path::Path;

/// How many recent folders are looked through for a git work tree.
const LOOKED: usize = 12;

/// The git work tree `dir` is in: itself or the nearest folder above it with `.git` (a
/// folder, or a file as in a worktree). Home and what is above it never count: a home
/// folder kept in git is not a project.
pub(super) fn git_root(dir: &str, home: Option<&str>) -> Option<String> {
    let home = home.map(|h| Path::new(h.trim_end_matches('/')));
    let mut at = Some(Path::new(dir.trim_end_matches('/')));
    while let Some(path) = at {
        if home.is_some_and(|h| h.starts_with(path)) || path.as_os_str().is_empty() {
            return None;
        }
        if path.join(".git").exists() {
            return Some(path.to_string_lossy().into_owned());
        }
        at = path.parent();
    }
    None
}
/// A space folder that names no project: none, home itself or `/`.
pub(super) fn no_project(cwd: &str, home: Option<&str>) -> bool {
    let cwd = cwd.trim_end_matches('/');
    cwd.is_empty() || home.is_some_and(|h| h.trim_end_matches('/') == cwd)
}
/// The newest of `offered` (newest first) in a git work tree, as that tree's folder.
pub(super) fn recent_repo(offered: &[String], home: Option<&str>) -> Option<String> {
    offered.iter().find_map(|d| git_root(d, home))
}

impl Workbench {
    /// The shown space, when its folder names no project: what the welcome page picks for.
    fn first_run_space(&self) -> Option<&Space> {
        let home = self.host.home();
        (self.workspace.spaces.iter())
            .find(|s| s.id == self.nav.space)
            .filter(|s| no_project(&s.cwd, home.as_deref()))
    }
    /// After each snapshot while the welcome page is up: the folder it offers, looked up
    /// here rather than in each frame.
    pub(super) fn first_run_suggest(&mut self) {
        let home = self.host.home();
        self.first_run.repo = match self.first_run_space() {
            Some(_) => recent_repo(
                &self.workspace.offered_dirs(home.as_deref(), LOOKED),
                home.as_deref(),
            ),
            None => None,
        };
    }
    /// Starting from the welcome page: the folder it offered becomes the shown space's, so
    /// this Agent and the ones after it start there. The window's copy is set at once, as
    /// the launch that follows reads it before the host's reply.
    pub(super) fn first_run_pin(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.first_run.repo.take() else {
            return;
        };
        let Some(space) = self.first_run_space().cloned() else {
            return;
        };
        let space = Space { cwd: repo, ..space };
        if let Some(own) = (self.workspace.spaces.iter_mut()).find(|s| s.id == space.id) {
            own.cwd = space.cwd.clone();
        }
        self.edit(Edit::UpdateSpace(space), cx);
    }
    /// 「启动一个 Agent」 with where it opens and 「选择项目文件夹…」, then the recent folders
    /// when that is open.
    pub(super) fn first_run_place(&self, cx: &Context<Self>) -> gpui::Div {
        let space = (self.workspace.spaces.iter()).find(|s| s.id == self.nav.space);
        let note = match (&self.first_run.repo, space) {
            (Some(repo), _) => t!(
                "在 {place} 打开（最近的 git 目录）",
                place = home_relative(&self.host, repo)
            ),
            (None, Some(space)) => t!("在 {place} 打开", place = self.space_label(space)),
            (None, None) => t!("在 {place} 打开", place = "~"),
        };
        let target = space.map(|s| s.id.clone());
        let recent = space.is_some() && !self.recent_dirs().is_empty();
        let choose = button("first-run-choose-dir", t!("选择项目文件夹…"))
            .border_0()
            .px(px(T::S4))
            .text_color(T::color(T::link()))
            .on_click(cx.listener(move |this, _, window, cx| match &target {
                // The recent folders first, as 「换目录」 under an empty tab lists them,
                // with 其他目录… for the Finder.
                Some(id) if recent => {
                    let open = this.layout.choosing_dir.as_deref() == Some(id.as_str());
                    this.layout.choosing_dir = (!open).then(|| id.clone());
                    cx.notify();
                }
                target => this.choose_space_dir(target.clone(), window, cx),
            }));
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(T::S4))
            .child(
                super::first_run::section(t!("启动一个 Agent"), note)
                    .child(div().flex().child(choose)),
            )
            .children(space.and_then(|s| self.dir_picker(s, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_folder_in_a_git_work_tree_is_offered_as_its_root() {
        let base = std::env::temp_dir().join(format!("fibo-frf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = |n: &str| {
            let d = base.join(n);
            std::fs::create_dir_all(&d).unwrap();
            d.to_string_lossy().into_owned()
        };
        let home_dir = dir("home");
        let (plain, repo, sub, worktree) = (
            dir("home/notes"),
            dir("home/code/app"),
            dir("home/code/app/crates/core"),
            dir("home/code/wt"),
        );
        std::fs::create_dir_all(base.join("home/code/app/.git")).unwrap();
        std::fs::write(base.join("home/code/wt/.git"), "gitdir: elsewhere\n").unwrap();
        let home = Some(home_dir.as_str());
        assert_eq!(git_root(&plain, home), None);
        assert_eq!(git_root(&sub, home).as_deref(), Some(repo.as_str()));
        assert_eq!(
            git_root(&format!("{repo}/"), home).as_deref(),
            Some(repo.as_str())
        );
        assert_eq!(
            git_root(&worktree, home).as_deref(),
            Some(worktree.as_str())
        );
        // The newest folder is no project: the next one that is wins, as its root.
        let offered = [plain.clone(), sub, worktree];
        assert_eq!(recent_repo(&offered, home).as_deref(), Some(repo.as_str()));
        assert_eq!(recent_repo(&[plain], home), None);
        assert_eq!(recent_repo(&[], home), None);
        // A home kept in git is not a project, nor is anything found by going above home.
        std::fs::create_dir_all(base.join("home/.git")).unwrap();
        assert_eq!(git_root(&dir("home/notes"), home), None);
        // The first space opened from the Finder starts in home: no project, so the page
        // offers one; a folder of its own (opened from a shell in a project) stays.
        assert!(no_project("", home));
        assert!(no_project(&format!("{home_dir}/"), home));
        assert!(!no_project(&repo, home));
        assert!(no_project("/", home), "nor is /");
        let _ = std::fs::remove_dir_all(base);
    }
}
