use mooring_platform::dirs::{config_home, home};

#[test]
fn home_and_config_home_are_absolute() {
    let home = home().expect("the test environment has a home");
    assert!(home.is_absolute(), "{home:?}");
    let config = config_home().expect("the test environment has a config home");
    assert!(config.is_absolute(), "{config:?}");
}
