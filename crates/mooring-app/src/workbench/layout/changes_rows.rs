//! How 看改动 draws one file's lines, and what it says about a file.
use super::*;
use crate::ui::tone::Tone;
use mooring_core::turn_diff::{Disk, FileSummary, LineKind, TurnFile};

pub(super) const ROW_HEIGHT: f32 = 19.;
/// The numbers' columns.
const GUTTER: f32 = 44.;
const SIGN: f32 = 16.;

pub(super) enum Row {
    /// Where a stretch of the file starts.
    Gap(u32),
    Line {
        kind: LineKind,
        old: u32,
        new: u32,
        text: gpui::SharedString,
    },
}

pub(super) fn rows_of(file: &TurnFile) -> Vec<Row> {
    let mut rows = Vec::new();
    for hunk in &file.hunks {
        if !file.created {
            let first = hunk.lines.first().map_or(0, |l| l.new.max(l.old));
            rows.push(Row::Gap(first));
        }
        rows.extend(hunk.lines.iter().map(|l| Row::Line {
            kind: l.kind,
            old: l.old,
            new: l.new,
            text: if l.text.is_empty() {
                " ".into()
            } else {
                l.text.replace('\t', "    ").into()
            },
        }));
    }
    rows
}

fn number(n: u32) -> String {
    if n == 0 { String::new() } else { n.to_string() }
}

pub(super) fn row_element(row: &Row, font: &gpui::Font) -> gpui::AnyElement {
    let base = div()
        .w_full()
        .flex()
        .font(font.clone())
        .text_size(px(T::SMALL))
        .line_height(px(ROW_HEIGHT));
    match row {
        Row::Gap(line) => base
            .px(px(T::S14))
            .bg(T::color(T::chip()))
            .text_color(T::color(T::muted()))
            .child(format!("第 {line} 行"))
            .into_any_element(),
        Row::Line {
            kind,
            old,
            new,
            text,
        } => {
            let (ink, wash, sign) = match kind {
                LineKind::Added => (Tone::Ok.ink(), Some(Tone::Ok.wash()), "+"),
                LineKind::Removed => (Tone::Danger.ink(), Some(Tone::Danger.wash()), "−"),
                LineKind::Same => (T::color(T::secondary()), None, ""),
            };
            let gutter = |n: u32| {
                div()
                    .w(px(GUTTER))
                    .flex_shrink_0()
                    .pr(px(T::S4))
                    .flex()
                    .justify_end()
                    .text_color(T::color(T::muted()))
                    .child(number(n))
            };
            base.when_some(wash, |d, w| d.bg(w))
                .child(gutter(*old))
                .child(gutter(*new))
                .child(
                    div()
                        .w(px(SIGN))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .text_color(ink)
                        .child(sign),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pr(px(T::S12))
                        .text_color(ink)
                        .child(text.clone()),
                )
                .into_any_element()
        }
    }
}

/// The line under the title: what about this file needs saying.
pub(super) fn warning(file: &TurnFile) -> Option<&'static str> {
    if file.one_by_one {
        return Some("这个文件中途被别的程序改过，这里按次显示每一次编辑。");
    }
    match file.disk {
        Disk::Same => None,
        Disk::Changed if file.edits > 0 => {
            Some("这是 Agent 当时的改动。文件后来又变了，和磁盘上的内容可能不一样。")
        }
        Disk::Changed => None,
        Disk::Gone if file.edits > 0 => Some("这个文件后来被删了。"),
        Disk::Gone => None,
    }
}

/// What to say instead of a diff, for a file with none to draw.
pub(super) fn empty_words(summary: &FileSummary, file: Option<&TurnFile>) -> Option<&'static str> {
    if summary.edits == 0 {
        Some("这个文件只有行数，没有改动内容")
    } else if file.is_some_and(|f| f.edits == 0) {
        Some("记录里已经找不到这个文件的改动")
    } else if file.is_some_and(|f| f.hunks.is_empty()) {
        Some("这一轮改完又改回去了，净改动为零")
    } else {
        None
    }
}

/// `…/layout/turn_bar.rs`: enough of a path to tell files apart in a narrow column.
pub(super) fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    match parts.as_slice() {
        [name, dir, _] => format!("…/{dir}/{name}"),
        _ => path.to_owned(),
    }
}

/// The files of `paths` every word of `filter` is found in, whatever the case, in order.
pub(super) fn matching<'a>(paths: impl Iterator<Item = &'a str>, filter: &str) -> Vec<usize> {
    let words: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    paths
        .enumerate()
        .filter(|(_, path)| {
            let path = path.to_lowercase();
            words.iter().all(|w| path.contains(w))
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooring_core::turn_diff::{DiffLine, Hunk};

    fn line(kind: LineKind, old: u32, new: u32, text: &str) -> DiffLine {
        DiffLine {
            kind,
            old,
            new,
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_new_file_has_no_stretch_headers_an_edit_does() {
        let hunk = Hunk {
            lines: vec![
                line(LineKind::Removed, 7, 0, "\tx"),
                line(LineKind::Added, 0, 7, ""),
            ],
        };
        let edited = TurnFile {
            hunks: vec![hunk.clone()],
            edits: 1,
            ..Default::default()
        };
        let rows = rows_of(&edited);
        assert!(matches!(rows[0], Row::Gap(7)));
        assert!(matches!(&rows[1], Row::Line { text, .. } if text.as_ref() == "    x"));
        assert!(matches!(&rows[2], Row::Line { text, .. } if text.as_ref() == " "));
        let created = TurnFile {
            created: true,
            ..edited
        };
        assert_eq!(rows_of(&created).len(), 2);
    }

    #[test]
    fn what_a_file_says_when_it_has_nothing_to_list() {
        let none = FileSummary::default();
        assert_eq!(
            empty_words(&none, None),
            Some("这个文件只有行数，没有改动内容")
        );
        let listed = FileSummary {
            edits: 2,
            ..Default::default()
        };
        assert_eq!(empty_words(&listed, None), None, "not read yet");
        let undone = TurnFile {
            edits: 2,
            ..Default::default()
        };
        assert_eq!(
            empty_words(&listed, Some(&undone)),
            Some("这一轮改完又改回去了，净改动为零")
        );
        let lost = TurnFile::default();
        assert_eq!(
            empty_words(&listed, Some(&lost)),
            Some("记录里已经找不到这个文件的改动")
        );
    }

    #[test]
    fn a_file_with_no_log_never_warns_about_the_disk() {
        let gone = TurnFile {
            disk: Disk::Gone,
            ..Default::default()
        };
        assert_eq!(warning(&gone), None);
        let edited = TurnFile {
            disk: Disk::Gone,
            edits: 1,
            ..Default::default()
        };
        assert!(warning(&edited).is_some());
    }

    #[test]
    fn short_paths_keep_their_last_two_parts() {
        assert_eq!(short_path("/a/b/c/d.rs"), "…/c/d.rs");
        assert_eq!(short_path("d.rs"), "d.rs");
        assert_eq!(short_path("c/d.rs"), "c/d.rs");
    }

    #[test]
    fn a_filter_needs_every_word_in_any_case() {
        let paths = ["/r/src/Turn_Bar.rs", "/r/src/pane.rs", "/r/docs/turn.md"];
        let find = |f: &str| matching(paths.iter().copied(), f);
        assert_eq!(find("turn"), [0, 2]);
        assert_eq!(find("TURN src"), [0]);
        assert_eq!(find("src bar"), [0]);
        assert_eq!(find("nothing"), Vec::<usize>::new());
        assert_eq!(find("  "), [0, 1, 2], "no words: everything");
    }
}
