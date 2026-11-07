use super::*;

/// Sat Nov 7 2026 16:00 in Shanghai.
const NOW: i64 = 1_794_038_400;
/// As Claude Code 2.1.319 printed it, advice included.
const REAL: &str = "You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 2% used · resets Nov 7 at 8:49pm (Asia/Shanghai)\nCurrent week (all models): 19% used · resets Nov 13 at 5:59pm (Asia/Shanghai)\nCurrent week (Fable): 0% used · resets Nov 13 at 6pm (Asia/Shanghai)\n\nWhat's contributing to your usage?\nApproximate · sessions on this machine only · shares overlap\n\nLast 24h · 4858 requests · 107 sessions\n  83%  Run fewer sessions at once · up to 17 at once\n";
/// Signed out and API-key sign-ins print the same.
const COST_ONLY: &str = "Total cost:            $0.0000\nTotal duration (API):  0s\nTotal duration (wall): 0s\nTotal code changes:    0 lines added, 0 lines removed\nUsage:                 0 input, 0 output, 0 cache read, 0 cache write";

#[test]
fn the_real_output_reads_as_three_windows_at_their_real_times() {
    let doc = read(REAL, NOW).unwrap();
    assert_eq!(doc["at"], NOW);
    assert_eq!(
        doc["session"],
        json!({"used": 2.0, "resets_at": NOW + 4 * 3600 + 49 * 60})
    );
    assert_eq!(
        doc["week"],
        json!({"used": 19.0, "resets_at": NOW + 6 * 86_400 + 2 * 3600 - 60})
    );
    assert_eq!(
        doc["models"],
        json!([{"name": "Fable", "used": 0.0, "resets_at": NOW + 6 * 86_400 + 2 * 3600}])
    );
    assert_eq!(doc["near"], false);
}

#[test]
fn every_way_it_writes_a_reset_reads_as_the_same_kind_of_time() {
    let reset = |text: &str| reset_time(text, NOW, SESSION);
    // 2.1.285 on, a named zone that is not this machine's, a bare hour, a whole date.
    assert_eq!(reset("Nov 7, 8:00pm (UTC)"), NOW + 12 * 3600);
    assert_eq!(reset("Nov 7 at 8pm (Asia/Tokyo)"), NOW + 3 * 3600);
    assert_eq!(reset("8pm (Asia/Shanghai)"), NOW + 4 * 3600);
    assert_eq!(reset("Nov 7, 2026 at 8pm (Asia/Shanghai)"), NOW + 4 * 3600);
    assert_eq!(reset("12:30am (Asia/Shanghai)"), NOW + 8 * 3600 + 30 * 60);
    assert_eq!(reset("12pm (Asia/Shanghai)"), NOW + 20 * 3600);
    // Unreadable: says nothing of when, and the window still shows.
    assert_eq!(reset("soon"), 0);
    assert_eq!(reset(""), 0);
    assert_eq!(reset("Foo 7 at 8pm (UTC)"), 0);
    assert_eq!(reset("Nov 99 at 8pm (UTC)"), 0);
}

#[test]
fn a_date_with_no_year_is_the_one_within_the_window() {
    // Dec 31 2026 16:00 in Shanghai, with a week renewing in January.
    let eve = 1_798_704_000;
    let doc = read(
        "Current week (all models): 1% used · resets Jan 3 at 6pm (Asia/Shanghai)",
        eve,
    )
    .unwrap();
    assert_eq!(doc["week"]["resets_at"], eve + 3 * 86_400 + 2 * 3600);
    // Read two days later, Dec 31 is already past, and the window reads as renewed.
    let doc = read(
        "Current session: 1% used · resets Dec 31 at 11pm (Asia/Shanghai)",
        eve + 2 * 86_400,
    )
    .unwrap();
    assert_eq!(doc["session"]["resets_at"], eve + 7 * 3600);
}

/// magpie's #4995: read as an HTTP status, this count had the account refused.
#[test]
fn a_count_of_403_requests_is_no_refusal_but_a_real_403_is() {
    let counted = "Current session: 5% used\nLast 7d · 403 requests · 388 sessions";
    assert_eq!(read(counted, NOW).unwrap()["session"]["used"], 5.0);
    let both =
        "Current session: 5% used\nLast 7d · 403 requests · 388 sessions · HTTP 403 Forbidden";
    assert!(read(both, NOW).unwrap_err().contains("HTTP 403"));
    // Below the figures it is advice, whatever it says.
    let advice =
        "Current session: 5% used\nWhat's contributing to your usage?\n  10%  not logged in sessions";
    assert_eq!(read(advice, NOW).unwrap()["session"]["used"], 5.0);
}

#[test]
fn past_the_limit_the_windows_still_read_and_sooner() {
    let over = "You are currently using your overages to power your Claude Code usage\nCurrent session: 100% used · resets 8pm (Asia/Shanghai)\nCurrent week (Sonnet only): 95% used";
    let doc = read(over, NOW).unwrap();
    assert_eq!(doc["near"], true);
    assert_eq!(doc["models"][0]["name"], "Sonnet", "{doc}");
    // One model near its end stops only that model: no need to ask every minute.
    let model = read(
        "Current session: 1% used\nCurrent week (Fable): 95% used",
        NOW,
    )
    .unwrap();
    assert_eq!(model["near"], false);
}

/// The two ways to have no windows must not read alike: one clears, the other keeps.
#[test]
fn no_limits_to_tell_differs_from_not_being_able_to_tell() {
    let none = read(COST_ONLY, NOW).unwrap();
    assert_eq!(none, json!({ "at": NOW }));
    for text in [
        "",
        "You are currently using your subscription to power your Claude Code usage",
        "Claude is temporarily limiting requests to the usage page (not your usage limit)",
        "Failed to load usage data",
    ] {
        assert!(read(text, NOW).is_err(), "{text:?}");
    }
}

#[test]
fn colors_do_not_hide_a_window() {
    let doc = read("\u{1b}[1mCurrent session: 7% used\u{1b}[0m", NOW).unwrap();
    assert_eq!(doc["session"]["used"], 7.0);
}
