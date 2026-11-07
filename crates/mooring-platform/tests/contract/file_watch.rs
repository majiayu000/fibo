use mooring_platform::file_watch::{Batch, Watch, flag, normalized};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The interface lets Windows refuse a watch for now.
fn start(roots: &[PathBuf], since: u64) -> Option<Watch> {
    match Watch::start(roots, since) {
        Ok(watch) => Some(watch),
        Err(e) if e.kind() == ErrorKind::Unsupported => None,
        Err(e) => panic!("{e}"),
    }
}

fn dir(name: &str) -> PathBuf {
    normalized(&crate::scratch(name)).unwrap()
}

/// Waits for the batch that `found` likes, and says what came meanwhile if none does.
fn wait(watch: &Watch, what: &str, mut found: impl FnMut(&Batch) -> bool) {
    let until = Instant::now() + Duration::from_secs(15);
    let mut seen = vec![];
    loop {
        let left = until.saturating_duration_since(Instant::now());
        match watch.rx.recv_timeout(left) {
            Ok(batch) => {
                if found(&batch) {
                    return;
                }
                seen.extend(batch.events);
            }
            Err(_) => panic!("{what}; saw {seen:?}"),
        }
    }
}

fn is(batch: &Batch, path: &Path) -> bool {
    batch.events.iter().any(|e| e.path == path)
}

/// Long enough for a stream that just started to be listening: FSEvents' start is not a
/// promise that the next write is seen as new, only that it is seen.
fn settle() {
    std::thread::sleep(Duration::from_millis(300));
}

#[test]
fn a_new_file_under_a_root_is_reported() {
    let root = dir("watch-new-file");
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    settle();
    let file = root.join("one.jsonl");
    std::fs::write(&file, "x\n").unwrap();
    wait(&watch, "the new file was not reported", |b| is(b, &file));
}

#[test]
fn a_file_in_a_new_directory_is_found() {
    let root = dir("watch-new-dir");
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    settle();
    let sub = root.join("a/b");
    std::fs::create_dir_all(&sub).unwrap();
    let file = sub.join("one.jsonl");
    std::fs::write(&file, "x\n").unwrap();
    // The directory's own event is enough: it means looking through it.
    let a = root.join("a");
    wait(&watch, "the new directory was not reported", |b| {
        b.events
            .iter()
            .any(|e| e.path == file || (e.path.starts_with(&a) && e.flags & flag::IS_DIR != 0))
    });
    // And from now on changes below it are seen too.
    settle();
    let second = sub.join("two.jsonl");
    std::fs::write(&second, "x\n").unwrap();
    wait(&watch, "a file below it was not reported", |b| {
        is(b, &second)
    });
}

#[test]
fn a_root_that_appears_later_is_watched() {
    let base = dir("watch-later");
    let root = base.join("not/yet");
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    settle();
    std::fs::create_dir_all(&root).unwrap();
    wait(&watch, "the root's arrival was not reported", |b| {
        b.events.iter().any(|e| e.path.starts_with(&root))
    });
    settle();
    let file = root.join("one.jsonl");
    std::fs::write(&file, "x\n").unwrap();
    wait(&watch, "a file in it was not reported", |b| is(b, &file));
}

#[test]
fn what_is_beside_a_root_is_not_reported() {
    let base = dir("watch-beside");
    let root = base.join("logs");
    std::fs::create_dir_all(&root).unwrap();
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    settle();
    let beside = base.join("elsewhere.db");
    std::fs::write(&beside, "x").unwrap();
    let file = root.join("one.jsonl");
    std::fs::write(&file, "x\n").unwrap();
    wait(&watch, "the file in the root was not reported", |b| {
        assert!(!is(b, &beside), "reported a file outside the root");
        is(b, &file)
    });
}

#[test]
fn a_followed_file_reports_an_append_while_its_writer_keeps_it_open() {
    let root = dir("watch-follow");
    let file = root.join("one.jsonl");
    let mut writer = std::fs::File::create(&file).unwrap();
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    assert_eq!(
        watch.follow(std::slice::from_ref(&file)).unwrap(),
        vec![file.clone()]
    );
    assert_eq!(
        watch.follow(std::slice::from_ref(&file)).unwrap(),
        Vec::<PathBuf>::new(),
        "followed already"
    );
    // A file that is not there is no failure, and not followed.
    let absent = root.join("absent.jsonl");
    assert_eq!(
        watch.follow(&[file.clone(), absent]).unwrap(),
        Vec::<PathBuf>::new()
    );
    // Whatever creating it queued is not the answer.
    settle();
    while watch.rx.try_recv().is_ok() {}
    writer.write_all(b"x\n").unwrap();
    writer.flush().unwrap();
    wait(&watch, "the append was not reported", |b| is(b, &file));
    drop(writer);
}

#[test]
fn a_change_while_nothing_watched_is_replayed_or_asks_for_a_look() {
    let root = dir("watch-restart");
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    settle();
    let first = root.join("one.jsonl");
    std::fs::write(&first, "x\n").unwrap();
    let mut checkpoint = 0;
    wait(&watch, "the first file was not reported", |b| {
        for e in &b.events {
            checkpoint = checkpoint.max(e.id);
        }
        is(b, &first)
    });
    drop(watch);
    let second = root.join("two.jsonl");
    std::fs::write(&second, "x\n").unwrap();
    let watch = start(std::slice::from_ref(&root), checkpoint).unwrap();
    wait(
        &watch,
        "the change was neither replayed nor asked for",
        |b| {
            b.events
                .iter()
                .any(|e| e.path == second || (e.path == root && e.flags & flag::SCAN != 0))
        },
    );
}

#[test]
fn starting_from_now_does_not_ask_for_a_look() {
    let root = dir("watch-from-now");
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    settle();
    let file = root.join("one.jsonl");
    std::fs::write(&file, "x\n").unwrap();
    wait(&watch, "the file was not reported", |b| {
        for e in &b.events {
            assert_eq!(e.flags & flag::SCAN, 0, "{e:?}");
        }
        is(b, &file)
    });
}

#[test]
fn a_root_that_goes_away_says_so() {
    let base = dir("watch-root-gone");
    let root = base.join("logs");
    std::fs::create_dir_all(&root).unwrap();
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    settle();
    std::fs::rename(&root, base.join("moved")).unwrap();
    wait(&watch, "the root's move was not reported", |b| {
        b.events.iter().any(|e| e.flags & flag::ROOT_CHANGED != 0)
    });
}

#[test]
fn the_notifier_reports_a_file_as_if_it_changed() {
    let root = dir("watch-notifier");
    let Some(watch) = start(std::slice::from_ref(&root), Watch::current_id()) else {
        return;
    };
    let file = root.join("told.jsonl");
    watch.notifier().file(file.clone());
    wait(&watch, "the notifier's file was not reported", |b| {
        is(b, &file)
    });
}
