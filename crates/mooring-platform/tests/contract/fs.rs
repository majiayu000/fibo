use mooring_platform::fs::{create_private_dir, link_dir, link_program};
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
