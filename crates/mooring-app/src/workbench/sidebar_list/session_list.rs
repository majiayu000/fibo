use super::*;

impl Workbench {
    /// One row standing for folded buckets; a click opens them. Under a heading it reads
    /// `◇ 等你验收 4  ‖ 停下了 3  很久没动 165`, each with its state's symbol; inside a folder
    /// it is the quieter `另有 等你验收 2 · 停下了 1`.
    fn fold_row(
        &self,
        id: String,
        parts: &[(Bucket, usize)],
        open: bool,
        prefix: &str,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let key = id.clone();
        let mono = T::mono(cx).family;
        let row = div()
            .id(gpui::SharedString::from(format!("sidebar-fold-{id}")))
            .min_h(px(T::ROW))
            .px(px(T::S8))
            .flex()
            .items_center()
            .gap(px(T::S4))
            .rounded(px(T::CONTROL_RADIUS))
            .cursor_pointer()
            .hover(|s| s.bg(T::color(T::raised())))
            .font_family(T::UI_FONT)
            .text_size(px(if prefix.is_empty() {
                T::META
            } else {
                T::CHIP_SIZE
            }))
            .text_color(T::color(if prefix.is_empty() {
                T::secondary()
            } else {
                T::muted()
            }));
        // The parts give way before the chevron does in a narrow column.
        let mut line = div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap_x(px(if prefix.is_empty() { T::S10 } else { T::S6 }));
        if !prefix.is_empty() {
            line = line.child(prefix.to_owned());
        }
        for (i, (b, n)) in parts.iter().enumerate() {
            let count = div()
                .font_family(mono.clone())
                .text_color(T::color(if prefix.is_empty() {
                    T::primary()
                } else {
                    T::muted()
                }))
                .child(n.to_string());
            let mut part = div().flex_shrink_0().flex().items_center().gap(px(T::S4));
            if prefix.is_empty() {
                if *b != Bucket::Stale {
                    part = part.child(glyph(bucket_group(*b)).font_family(mono.clone()));
                }
            } else if i > 0 {
                line = line.child("·");
            }
            line = line.child(part.child(b.label()).child(count));
        }
        row.child(line)
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(T::color(T::muted()))
                    .child(if open { "⌄" } else { "›" }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_sidebar_group(&key, open, cx)))
            .into_any_element()
    }
    /// The session rows for the current space, arranged by the chosen preset; returns the
    /// rows, how many sessions were left for 鸟瞰, and whether the closed ones were placed
    /// by the preset (a WASM one) rather than left to the summary under the list.
    pub(in crate::workbench) fn session_list(
        &self,
        sessions: Vec<Listed>,
        closed: &super::super::sidebar_wasm::Closed,
        current: Option<&str>,
        cx: &Context<Self>,
    ) -> (Vec<gpui::AnyElement>, usize, bool) {
        let preset = self.sidebar.preset();
        let detail = |l: &Listed| match preset.detail {
            Detail::Never => None,
            Detail::Always => self.row_detail(l.0, l.1),
            Detail::Attention => {
                (matches!(self.row_bucket(l), Bucket::Attention | Bucket::Running)
                    || l.1 == Status::Background
                    || current == Some(l.0.id.as_str()))
                .then(|| self.row_detail(l.0, l.1))
                .flatten()
            }
        };
        let mut out = vec![];
        // 置顶 comes first in every preset; its rows leave their usual group.
        let pinned_terminal = |l: &Listed| {
            l.0.terminal
                .as_ref()
                .filter(|t| {
                    self.sidebar.pinned.contains(t) && live_terminal(&self.workspace, l.0).is_some()
                })
                .cloned()
        };
        let (mut top, sessions): (Vec<Listed>, Vec<Listed>) = sessions
            .into_iter()
            .partition(|l| pinned_terminal(l).is_some());
        let all_pinned = !top.is_empty() && sessions.is_empty();
        if !top.is_empty() {
            top.sort_by_key(|l| {
                pinned_terminal(l).and_then(|t| self.sidebar.pinned.iter().position(|p| *p == t))
            });
            let recent = preset.group == Grouping::Recent;
            out.push(if recent {
                self.flat_fold("recent:pinned", "置顶", top.len(), cx)
            } else {
                self.heading("置顶".into(), top.len(), cx)
                    .mt(px(T::S14))
                    .into_any_element()
            });
            let open = !recent || self.sidebar.open("recent:pinned", true);
            for l in top {
                // The session you are in stays in sight while its group is folded.
                if open || current == Some(l.0.id.as_str()) {
                    let d = detail(&l);
                    out.push(self.session_row(l, current, d, End::State, cx));
                }
            }
        }
        let mut budget = preset.limit;
        let mut more = 0;
        let mut owns_closed = false;
        let mut push = |out: &mut Vec<gpui::AnyElement>, l: Listed, d: Option<String>, e: End| {
            if budget == 0 && current != Some(l.0.id.as_str()) {
                more += 1;
                return;
            }
            budget = budget.saturating_sub(1);
            out.push(self.session_row(l, current, d, e, cx));
        };
        match preset.group {
            Grouping::Flat if all_pinned => (),
            Grouping::Flat => {
                for l in sessions {
                    let d = detail(&l);
                    push(&mut out, l, d, End::State);
                }
            }
            Grouping::Status => {
                let folded: Vec<(Bucket, usize)> = Bucket::ALL
                    .into_iter()
                    .filter(|b| preset.folds(*b))
                    .map(|b| {
                        (
                            b,
                            sessions.iter().filter(|l| self.row_bucket(l) == b).count(),
                        )
                    })
                    .filter(|(_, n)| *n > 0)
                    .collect();
                let rest_open = self.sidebar.open("status:rest", false);
                for b in Bucket::ALL.into_iter().filter(|b| !preset.folds(*b)) {
                    let rows: Vec<_> = sessions
                        .iter()
                        .filter(|l| self.row_bucket(l) == b)
                        .collect();
                    if rows.is_empty() {
                        continue;
                    }
                    out.push(
                        self.heading(b.label().into(), rows.len(), cx)
                            .into_any_element(),
                    );
                    for l in rows {
                        push(&mut out, *l, detail(l), End::State);
                    }
                }
                if !folded.is_empty() {
                    out.push(
                        div()
                            .mt(px(T::S12))
                            .child(self.fold_row("status:rest".into(), &folded, rest_open, "", cx))
                            .into_any_element(),
                    );
                }
                // Opened, in the summary's order: done, stopped, then gone quiet.
                let mut rest: Vec<_> = sessions
                    .iter()
                    .filter(|l| preset.folds(self.row_bucket(l)))
                    .collect();
                rest.sort_by_key(|l| Bucket::ALL.iter().position(|b| *b == self.row_bucket(l)));
                for l in rest {
                    // The session you are in stays in sight while its group is folded.
                    if rest_open || current == Some(l.0.id.as_str()) {
                        push(&mut out, *l, detail(l), End::State);
                    }
                }
            }
            Grouping::Recent => {
                self.recent_rows(sessions, closed, current, &mut push, &mut out, cx);
                owns_closed = true;
            }
            Grouping::Wasm => {
                owns_closed =
                    self.wasm_rows(preset, sessions, closed, current, &mut push, &mut out, cx);
            }
            Grouping::Directory => {
                // What needs you is listed once, on top, with its folder in its second
                // line; the folders below only count it.
                let pinned: Vec<_> = sessions
                    .iter()
                    .filter(|l| self.row_bucket(l) == Bucket::Attention)
                    .collect();
                if !pinned.is_empty() {
                    out.push(
                        self.heading(mooring_core::status::NEEDS_YOU.into(), pinned.len(), cx)
                            .into_any_element(),
                    );
                    for l in &pinned {
                        let d = detail(l).map(|d| format!("{} · {d}", folder_name(&l.0.cwd)));
                        push(&mut out, **l, d, End::State);
                    }
                }
                // Folders in the order their first session sorts: busy ones first.
                let mut folders: Vec<(&str, Vec<Listed>)> = vec![];
                for l in &sessions {
                    match folders.iter_mut().find(|(cwd, _)| *cwd == l.0.cwd) {
                        Some((_, rows)) => rows.push(*l),
                        None => folders.push((l.0.cwd.as_str(), vec![*l])),
                    }
                }
                // Open folders (something running) first, then those holding what needs
                // you, then the quiet ones; otherwise as their first session sorts.
                folders.sort_by_key(|(_, rows)| {
                    let has = |b: Bucket| rows.iter().any(|l| self.row_bucket(l) == b);
                    (!has(Bucket::Running), !has(Bucket::Attention))
                });
                if !folders.is_empty() {
                    out.push(Self::group_title("目录".into(), None, cx).into_any_element());
                }
                // A folder's rows sit in from its name.
                let indent = |out: &mut Vec<gpui::AnyElement>, from: usize| {
                    let rows: Vec<_> = out.drain(from..).collect();
                    out.extend(
                        rows.into_iter()
                            .map(|r| div().pl(px(T::S14)).child(r).into_any_element()),
                    );
                };
                for (cwd, rows) in folders {
                    let needs = rows
                        .iter()
                        .filter(|l| self.row_bucket(l) == Bucket::Attention)
                        .count();
                    let running = rows.iter().any(|l| self.row_bucket(l) == Bucket::Running);
                    let key = format!("dir:{cwd}");
                    let open = self.sidebar.open(&key, running);
                    out.push(self.folder_row(cwd, &key, rows.len(), needs, open, running, cx));
                    if !open {
                        continue;
                    }
                    let from = out.len();
                    let mut folded: Vec<(Bucket, usize)> = vec![];
                    for l in rows
                        .iter()
                        .filter(|l| self.row_bucket(l) != Bucket::Attention)
                    {
                        let b = self.row_bucket(l);
                        if preset.folds(b) {
                            match folded.iter_mut().find(|(f, _)| *f == b) {
                                Some((_, n)) => *n += 1,
                                None => folded.push((b, 1)),
                            }
                        } else {
                            push(&mut out, *l, detail(l), End::State);
                        }
                    }
                    folded.sort_by_key(|(b, _)| Bucket::ALL.iter().position(|a| a == b));
                    if !folded.is_empty() {
                        let more_key = format!("dir-more:{cwd}");
                        let more_open = self.sidebar.open(&more_key, false);
                        out.push(self.fold_row(more_key, &folded, more_open, "另有", cx));
                        if more_open {
                            for l in rows.iter().filter(|l| {
                                self.row_bucket(l) != Bucket::Attention
                                    && preset.folds(self.row_bucket(l))
                            }) {
                                push(&mut out, *l, detail(l), End::State);
                            }
                        }
                    }
                    indent(&mut out, from);
                }
            }
        }
        (out, more, owns_closed)
    }
}
