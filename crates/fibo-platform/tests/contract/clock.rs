use fibo_platform::clock::local;

#[test]
fn fields_are_in_range_and_agree_with_the_offset() {
    // Over a year, so clock changes are included wherever this runs.
    for at in (1_767_225_600u64..1_798_761_600).step_by(6 * 3600 + 17) {
        let t = local(at);
        assert!((0..24).contains(&t.hour), "{t:?}");
        assert!((0..60).contains(&t.minute), "{t:?}");
        assert!((0..=60).contains(&t.second), "{t:?}");
        assert!((0..7).contains(&t.weekday), "{t:?}");
        assert!((1..=12).contains(&t.month), "{t:?}");
        assert!((1..=31).contains(&t.day), "{t:?}");
        assert!(t.utc_offset.abs() <= 14 * 3600, "{t:?}");
        let seconds_in_day = (at as i64 + t.utc_offset).rem_euclid(86_400);
        assert_eq!(
            seconds_in_day,
            i64::from(t.hour * 3600 + t.minute * 60 + t.second),
            "{t:?}"
        );
        // 1970-01-01 was a Thursday.
        let weekday = ((at as i64 + t.utc_offset).div_euclid(86_400) + 4).rem_euclid(7);
        assert_eq!(weekday, i64::from(t.weekday), "{t:?}");
    }
}
