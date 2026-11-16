use fibo_platform::launch::current;

/// Whatever a platform knows, it reports in range; what it does not know is None.
#[test]
fn a_launch_reads_in_range() {
    let launch = current();
    assert!((-20..=20).contains(&launch.nice), "{launch:?}");
    if let Some(program) = &launch.responsible {
        assert!(program.is_absolute(), "{launch:?}");
    }
    assert_eq!(
        launch,
        current(),
        "nothing in it changes while the process runs"
    );
}
