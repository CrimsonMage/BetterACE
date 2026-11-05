//! Age text against the GDLE `GetAgeString` oracle fixture (`fixtures/age.csv`).
//!
//! The fixture is produced by `oracle/age_generate.py` from the verbatim pinned GDLE function.
//! `exact` rows are inputs GDLE stores without wrap, and the port must match them exactly.
//! `signed_wrap` rows are above i32::MAX, where GDLE's signed `int` goes negative; the port
//! must not reproduce that sign.
use bace_character::age_text;

const FIXTURE: &str = include_str!("fixtures/age.csv");
const GDLE_PREFIX: &str = "You have played for ";

struct Row<'a> {
    seconds: u32,
    gdle_int: i64,
    status: &'a str,
    body: &'a str,
}

fn rows() -> Vec<Row<'static>> {
    FIXTURE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let mut fields = line.splitn(4, ',');
            let seconds = fields.next().unwrap().parse().unwrap();
            let gdle_int = fields.next().unwrap().parse().unwrap();
            let status = fields.next().unwrap();
            let body = fields.next().unwrap();
            Row {
                seconds,
                gdle_int,
                status,
                body,
            }
        })
        .collect()
}

fn render(seconds: u32) -> String {
    let mut out = String::with_capacity(24);
    age_text(seconds, &mut out);
    out
}

#[test]
fn exact_gdle_rows_match_the_port() {
    let mut exact = 0;
    for row in rows().iter().filter(|row| row.status == "exact") {
        assert_eq!(i64::from(row.seconds), row.gdle_int, "row {}", row.seconds);
        assert_eq!(render(row.seconds), row.body, "seconds {}", row.seconds);
        exact += 1;
    }
    // Fixed boundaries plus the 100 seeded values below 2^31.
    assert!(exact >= 100 + 17, "only {exact} exact rows");
}

#[test]
fn signed_wrap_rows_keep_the_unsigned_decomposition() {
    let mut wrapped = 0;
    for row in rows().iter().filter(|row| row.status == "signed_wrap") {
        assert!(
            row.gdle_int < 0,
            "signed_wrap row {} is not negative in GDLE",
            row.seconds
        );
        let text = render(row.seconds);
        assert!(!text.contains('-'), "seconds {}: {text}", row.seconds);
        assert_ne!(text, row.body);
        wrapped += 1;
    }
    assert!(wrapped >= 50, "only {wrapped} signed_wrap rows");
}

#[test]
fn boundary_vectors_match_hand_checked_gdle_text() {
    let vectors: [(u32, &str); 20] = [
        (0, "0s"),
        (1, "1s"),
        (59, "59s"),
        (60, "1m 0s"),
        (3599, "59m 59s"),
        (3600, "1h 0s"),
        (86399, "23h 59m 59s"),
        (86400, "1d 0s"),
        (2591999, "29d 23h 59m 59s"),
        (2592000, "30d 0s"),
        (2592001, "30d 1s"),
        (2629799, "30d 10h 29m 59s"),
        (2629800, "1mo 0s"),
        (2629801, "1mo 1s"),
        (31536000, "11mo 30d 4h 30m 0s"),
        (34560000, "13mo 4d 7h 30m 0s"),
        (2147483647, "816mo 18d 3h 14m 7s"),
        (0x7FFF_FFFF + 1, "816mo 18d 3h 14m 8s"),
        (u32::MAX, "1633mo 5d 19h 58m 15s"),
        (u32::MAX - 1, "1633mo 5d 19h 58m 14s"),
    ];
    for (seconds, expected) in vectors {
        assert_eq!(render(seconds), expected, "seconds {seconds}");
    }
}

#[test]
fn caller_buffer_is_cleared_and_never_reallocated() {
    let mut out = String::with_capacity(64);
    out.push_str("stale text that must not survive");
    let capacity = out.capacity();
    for row in rows() {
        age_text(row.seconds, &mut out);
        assert_eq!(out.capacity(), capacity, "reallocated for {}", row.seconds);
        assert!(out.len() <= 22, "body {out} longer than 22 bytes");
        assert!(!out.starts_with(GDLE_PREFIX), "prefix leaked into body");
        assert!(!out.contains("stale"));
    }
}

#[test]
fn no_sign_or_prefix_for_any_u32_extreme() {
    for seconds in [0, 1, u32::MAX, u32::MAX / 2, u32::MAX / 2 + 1] {
        let text = render(seconds);
        assert!(!text.contains('-') && !text.ends_with('.'), "{text}");
        assert!(text.ends_with('s'), "{text}");
    }
}
