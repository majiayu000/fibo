use fibo_platform::fs::{create_private_dir, link_dir, link_program};
use std::io::ErrorKind;

#[test]
fn a_private_dir_is_made_with_its_parents_and_may_exist() {
    let root = crate::scratch("private");
    let dir = root.join("a").join("b");
    create_private_dir(&dir).unwrap();
    create_private_dir(&dir).unwrap();
    assert!(dir.is_dir());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_directory_link_reaches_the_directory() {
    let root = crate::scratch("link-dir");
    let (to, at) = (root.join("real"), root.join("link"));
    std::fs::create_dir(&to).unwrap();
    std::fs::write(to.join("f"), "x").unwrap();
    match link_dir(&to, &at) {
        Ok(()) => assert_eq!(std::fs::read_to_string(at.join("f")).unwrap(), "x"),
        // The one refusal the interface allows: an account without the right to make links.
        Err(e) if cfg!(windows) => eprintln!("refused: {e}"),
        Err(e) => panic!("{e}"),
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_program_link_reaches_the_program_or_says_it_is_unsupported() {
    let root = crate::scratch("link-program");
    let (to, at) = (root.join("real"), root.join("link"));
    std::fs::write(&to, "x").unwrap();
    match link_program(&to, &at) {
        Ok(()) => assert_eq!(std::fs::read_to_string(&at).unwrap(), "x"),
        Err(e) => assert_eq!(e.kind(), ErrorKind::Unsupported, "{e}"),
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_id_follows_the_file_and_not_its_name() {
    use fibo_platform::fs::{file_id, open_file_id};
    let dir = crate::scratch("fs-id");
    let a = dir.join("a");
    let b = dir.join("b");
    std::fs::write(&a, "1").unwrap();
    std::fs::write(&b, "2").unwrap();
    let id = |p: &std::path::Path| file_id(p, &std::fs::metadata(p).unwrap()).unwrap();
    let first = id(&a);
    assert_eq!(first, id(&a));
    assert_ne!(first, id(&b));
    let open = std::fs::File::open(&a).unwrap();
    assert_eq!(open_file_id(&open).unwrap(), first);
    drop(open);
    // An append keeps it, moving keeps it, a new file in its place does not.
    use std::io::Write;
    let mut append = std::fs::OpenOptions::new().append(true).open(&a).unwrap();
    append.write_all(b"more").unwrap();
    drop(append);
    assert_eq!(id(&a), first);
    let moved = dir.join("moved");
    std::fs::rename(&a, &moved).unwrap();
    assert_eq!(id(&moved), first);
    std::fs::write(&a, "new").unwrap();
    assert_ne!(id(&a), first);
    // Asking holds nothing up.
    let held = std::fs::File::open(&b).unwrap();
    id(&b);
    drop(held);
}

#[test]
fn a_private_file_can_be_written_and_read_back() {
    let dir = crate::scratch("fs-private");
    let path = dir.join("secret");
    use std::io::Write;
    let mut file = fibo_platform::fs::private_file()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(b"x").unwrap();
    drop(file);
    assert_eq!(std::fs::read(&path).unwrap(), b"x");
    assert!(
        fibo_platform::fs::private_file()
            .write(true)
            .create_new(true)
            .open(&path)
            .is_err()
    );
}

#[test]
fn a_lock_is_held_by_one_opener_at_a_time() {
    let dir = crate::scratch("fs-lock");
    let path = dir.join("host.lock");
    let first = fibo_platform::fs::lock_file(&path).unwrap();
    let second = fibo_platform::fs::lock_file(&path).unwrap();
    first.try_lock().unwrap();
    assert!(second.try_lock().is_err());
    drop(first);
    // A test running beside this one may be starting a process, which holds a copy of the
    // descriptor, and so the lock, until it execs.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while second.try_lock().is_err() {
        assert!(
            std::time::Instant::now() < until,
            "the lock never came free"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn a_secure_folder_is_usable_again_and_again_and_a_file_is_not_one() {
    let dir = crate::scratch("fs-secure");
    let runtime = dir.join("runtime");
    fibo_platform::fs::secure_dir(&runtime).unwrap();
    fibo_platform::fs::secure_dir(&runtime).unwrap();
    std::fs::write(runtime.join("inside"), "x").unwrap();
    std::fs::create_dir(runtime.join("sub")).unwrap();
    std::fs::write(runtime.join("sub/deeper"), "x").unwrap();
    assert_eq!(std::fs::read(runtime.join("sub/deeper")).unwrap(), b"x");
    let file = dir.join("file");
    std::fs::write(&file, "x").unwrap();
    assert!(fibo_platform::fs::secure_dir(&file).is_err());
}

#[test]
fn a_canonical_path_compares_equal_to_one_a_process_reports_and_can_be_a_directory() {
    let dir = crate::scratch("fs-canonical");
    let real = fibo_platform::fs::canonical(&dir).unwrap();
    assert!(real.is_dir());
    assert!(!real.to_string_lossy().starts_with(r"\\?\"), "{real:?}");
    assert_eq!(fibo_platform::fs::canonical(&real.join(".")).unwrap(), real);
    assert!(fibo_platform::fs::canonical(&dir.join("not-there")).is_err());
}
