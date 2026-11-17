use super::*;

#[test]
fn timestamps_keep_rfc3339_offsets_and_unknown_values_stay_zero() {
    for text in [
        "2026-08-25T03:21:00Z",
        "2026-08-25T11:21:00+08:00",
        "2026-08-24T23:51:00-03:30",
        "2026-08-25T03:21:00.123Z",
    ] {
        assert_eq!(seconds(&json!(text)), 1_787_628_060, "{text}");
    }
    assert_eq!(seconds(&json!(1_787_628_060_u64)), 1_787_628_060);
    for value in [
        json!("1969-12-31T23:59:59Z"),
        json!("bad"),
        json!("2026-02-30T03:21:00Z"),
        json!("2026-08-25T03:21:00"),
        json!("1787628060"),
        json!(-1),
        Value::Null,
    ] {
        assert_eq!(seconds(&value), 0, "{value}");
    }
}
