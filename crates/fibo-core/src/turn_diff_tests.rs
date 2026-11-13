use super::*;

fn file(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("line {i}")).collect()
}
fn join(lines: &[String]) -> String {
    lines.join("\n")
}
/// The edit an Agent's tool would log for this change, with context, as the tools do.
fn edit(before: &[String], after: &[String]) -> Edit {
    let ops = similar::capture_diff_slices(Algorithm::Myers, before, after);
    let mut hunks = Vec::new();
    for group in group_diff_ops(ops, CONTEXT) {
        let first = &group[0];
        let mut lines = Vec::new();
        for op in &group {
            if matches!(op, DiffOp::Equal { .. }) {
                lines.extend(op.old_range().map(|o| format!(" {}", before[o])));
            } else {
                lines.extend(op.old_range().map(|o| format!("-{}", before[o])));
                lines.extend(op.new_range().map(|n| format!("+{}", after[n])));
            }
        }
        let (old, new) = (first.old_range(), first.new_range());
        let has_old = lines.iter().any(|l| !l.starts_with('+'));
        let has_new = lines.iter().any(|l| !l.starts_with('-'));
        hunks.push(RawHunk {
            old_start: old.start + usize::from(has_old),
            new_start: new.start + usize::from(has_new),
            lines,
        });
    }
    Edit::Patched(hunks)
}
fn compose(edits: Vec<Edit>, now: &[String]) -> TurnFile {
    let mut c = Composer::default();
    edits.into_iter().for_each(|e| c.apply(e));
    c.finish("f", Some(&join(now)))
}
fn replace(lines: &mut [String], at: usize, with: &str) {
    lines[at - 1] = with.to_owned();
}
/// `+x` and `-x` of the file, in order.
fn changes(f: &TurnFile) -> Vec<String> {
    let mut out = Vec::new();
    for line in f.hunks.iter().flat_map(|h| &h.lines) {
        match line.kind {
            LineKind::Added => out.push(format!("+{} {}", line.new, line.text)),
            LineKind::Removed => out.push(format!("-{} {}", line.old, line.text)),
            LineKind::Same => {}
        }
    }
    out
}
/// `before` with the file's hunks applied; panics where a hunk does not fit.
fn applied(f: &TurnFile, before: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut read = 1usize;
    for line in f.hunks.iter().flat_map(|h| &h.lines) {
        if line.old > 0 {
            while read < line.old as usize {
                out.push(before[read - 1].clone());
                read += 1;
            }
            assert_eq!(before[read - 1], line.text, "hunk does not fit at {read}");
            read += 1;
        }
        if line.kind == LineKind::Added {
            // No context says where: its number in the new file does.
            while out.len() + 1 < line.new as usize {
                out.push(before[read - 1].clone());
                read += 1;
            }
        }
        if line.kind != LineKind::Removed {
            out.push(line.text.clone());
        }
    }
    out.extend_from_slice(&before[read - 1..]);
    out
}

#[test]
fn one_edit_shows_its_change_with_context() {
    let v0 = file(20);
    let mut v1 = v0.clone();
    replace(&mut v1, 10, "ten");
    let f = compose(vec![edit(&v0, &v1)], &v1);
    assert_eq!(changes(&f), ["-10 line 10", "+10 ten"]);
    assert_eq!((f.added, f.removed, f.edits), (1, 1, 1));
    assert_eq!(f.hunks[0].lines.len(), 8, "three lines each side");
    assert_eq!(f.disk, Disk::Same);
}

#[test]
fn a_line_changed_twice_shows_once() {
    let v0 = file(20);
    let (mut v1, mut v2) = (v0.clone(), v0.clone());
    replace(&mut v1, 10, "first try");
    replace(&mut v2, 10, "second try");
    let f = compose(vec![edit(&v0, &v1), edit(&v1, &v2)], &v2);
    assert_eq!(changes(&f), ["-10 line 10", "+10 second try"]);
    assert_eq!((f.added, f.removed, f.edits), (1, 1, 2));
    assert!(!f.one_by_one);
}

#[test]
fn a_change_undone_leaves_nothing() {
    let v0 = file(20);
    let mut v1 = v0.clone();
    replace(&mut v1, 10, "oops");
    let f = compose(vec![edit(&v0, &v1), edit(&v1, &v0)], &v0);
    assert!(f.hunks.is_empty());
    assert_eq!((f.added, f.removed, f.edits), (0, 0, 2));
}

#[test]
fn a_later_edit_above_keeps_the_old_numbers_of_the_one_below() {
    let v0 = file(40);
    let mut v1 = v0.clone();
    replace(&mut v1, 30, "thirty");
    let mut v2 = v1.clone();
    v2.splice(4..4, ["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    let f = compose(vec![edit(&v0, &v1), edit(&v1, &v2)], &v2);
    assert_eq!(
        changes(&f),
        ["+5 a", "+6 b", "+7 c", "-30 line 30", "+33 thirty"]
    );
    assert_eq!(applied(&f, &v0), v2);
}

#[test]
fn edits_side_by_side_become_one_stretch() {
    let v0 = file(20);
    let mut v1 = v0.clone();
    replace(&mut v1, 8, "eight");
    let mut v2 = v1.clone();
    replace(&mut v2, 12, "twelve");
    let f = compose(vec![edit(&v0, &v1), edit(&v1, &v2)], &v2);
    assert_eq!(f.hunks.len(), 1);
    assert_eq!(applied(&f, &v0), v2);
}

#[test]
fn edits_of_a_file_made_this_turn_stay_all_new() {
    let v0 = file(5);
    let mut v1 = v0.clone();
    replace(&mut v1, 2, "two");
    let f = compose(vec![Edit::Created(join(&v0)), edit(&v0, &v1)], &v1);
    assert!(f.created);
    assert_eq!((f.added, f.removed), (5, 0));
    assert!(f.hunks[0].lines.iter().all(|l| l.kind == LineKind::Added));
    assert_eq!(f.hunks[0].lines[1].text, "two");
}

#[test]
fn an_edit_that_disagrees_shows_every_edit_and_says_so() {
    let v0 = file(20);
    let mut v1 = v0.clone();
    replace(&mut v1, 10, "ten");
    // Something else wrote line 10 between the two edits.
    let mut other = v1.clone();
    replace(&mut other, 10, "someone else");
    let mut v2 = other.clone();
    replace(&mut v2, 10, "again");
    let f = compose(vec![edit(&v0, &v1), edit(&other, &v2)], &v2);
    assert!(f.one_by_one);
    assert_eq!(f.hunks.len(), 2);
    assert_eq!(f.disk, Disk::Same, "the steps say nothing of the file now");
}

#[test]
fn the_disk_is_compared_with_what_the_edits_left() {
    let v0 = file(20);
    let mut v1 = v0.clone();
    replace(&mut v1, 10, "ten");
    let mut later = v1.clone();
    replace(&mut later, 10, "changed since");
    assert_eq!(compose(vec![edit(&v0, &v1)], &later).disk, Disk::Changed);
    let mut c = Composer::default();
    c.apply(edit(&v0, &v1));
    assert_eq!(c.finish("f", None).disk, Disk::Gone);
    // Lines added elsewhere above move what the edit left.
    let mut moved = v1.clone();
    moved.insert(0, "new top".to_owned());
    assert_eq!(compose(vec![edit(&v0, &v1)], &moved).disk, Disk::Changed);
}

#[test]
fn past_the_cap_counts_stay_whole() {
    let big = file(SHOWN + 50);
    let f = compose(vec![Edit::Created(join(&big))], &big);
    assert!(f.cut);
    assert_eq!(f.added as usize, SHOWN + 50);
    assert_eq!(f.hunks[0].lines.len(), SHOWN);
}

#[test]
fn a_unified_diff_reads_back_with_blank_context_and_no_newline_marks() {
    let hunks = parse_unified(
        "--- a\n+++ b\n@@ -3,3 +3,3 @@ fn x\n a\n\n-b\n+c\n\\ No newline at end of file\n@@ -0,0 +1 @@\n+top\n",
    );
    assert_eq!(hunks.len(), 2);
    assert_eq!(hunks[0].lines, [" a", " ", "-b", "+c"]);
    assert_eq!((hunks[1].old_start, hunks[1].new_start), (0, 1));
    assert_eq!(hunks[1].lines, ["+top"]);
}

#[test]
fn a_pure_insert_goes_after_the_line_it_names() {
    let v0 = file(3);
    let mut c = Composer::default();
    c.apply(Edit::Patched(parse_unified("@@ -2,0 +3 @@\n+new\n")));
    let f = c.finish("f", None);
    assert_eq!(changes(&f), ["+3 new"]);
    assert_eq!(applied(&f, &v0), ["line 1", "line 2", "new", "line 3"]);
}

/// Whatever the edits, the sum takes the file from where it started to where it ended, and
/// says only as much as the two differ by.
#[test]
fn any_series_of_edits_sums_to_the_file_it_ended_as() {
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    for round in 0..400 {
        let v0 = file(1 + next(120));
        let mut now = v0.clone();
        let mut edits = Vec::new();
        for step in 0..1 + next(8) {
            let mut after = now.clone();
            for _ in 0..1 + next(4) {
                let at = next(after.len() + 1);
                match next(3) {
                    0 => after.insert(at, format!("new {round}.{step}.{at}")),
                    1 if at < after.len() => {
                        after.remove(at);
                    }
                    _ if at < after.len() => after[at] = format!("was {round}.{step}.{at}"),
                    _ => {}
                }
            }
            edits.push(edit(&now, &after));
            now = after;
        }
        let mut sum = Composer::default();
        edits.iter().cloned().for_each(|e| sum.apply(e));
        let listed = sum.summary("f", Vec::new(), true);
        let f = compose(edits, &now);
        assert_eq!(
            (listed.added, listed.removed),
            (f.added, f.removed),
            "round {round}"
        );
        assert!(!f.one_by_one, "round {round}");
        assert_eq!(applied(&f, &v0), now, "round {round}");
        assert_eq!(f.disk, Disk::Same, "round {round}");
        if now == v0 {
            assert!(f.hunks.is_empty(), "round {round}");
        }
    }
}

#[test]
fn a_card_keeps_its_totals_past_the_files_it_lists() {
    use crate::content::Content;
    let mut card = Content::default();
    for i in 0..5000u32 {
        card.touch(&format!("/r/f{i}"), 10, 2, 1);
    }
    assert_eq!(card.turn_files().count(), 64, "the list stays small");
    assert_eq!(card.turn_file_count(), 5000);
    assert_eq!(card.turn_lines(), (10_000, 5000));
    // A new turn starts counting afresh, its old files not among them.
    card.turn_started = 11;
    assert_eq!((card.turn_file_count(), card.turn_lines()), (0, (0, 0)));
    card.touch("/r/new", 12, 1, 0);
    assert_eq!((card.turn_file_count(), card.turn_lines()), (1, (1, 0)));
    // Files from before this turn fall off without counting.
    for i in 0..70u32 {
        card.touch(&format!("/r/n{i}"), 12, 1, 0);
    }
    assert_eq!(card.turn_file_count(), 71);
}
