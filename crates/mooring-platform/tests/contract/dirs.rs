use mooring_platform::dirs::{config_home, home, home_str};

#[test]
fn home_and_config_home_are_absolute() {
    let home = home().expect("the test environment has a home");
    assert!(home.is_absolute(), "{home:?}");
    let config = config_home().expect("the test environment has a config home");
    assert!(config.is_absolute(), "{config:?}");
}

#[test]
fn home_as_text_is_the_same_directory() {
    assert_eq!(home_str().map(Into::into), home());
}

#[test]
fn a_folder_someone_chose_is_not_where_the_system_starts_things() {
    let home = mooring_platform::dirs::home().unwrap();
    assert!(!mooring_platform::dirs::is_system_start(&home));
    let root = home.ancestors().last().unwrap();
    assert!(mooring_platform::dirs::is_system_start(root), "{root:?}");
}
