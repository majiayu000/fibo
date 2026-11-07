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
