//! Explicit timestamp formatting and source TimeSpan component formatting.
use super::{ShardClock, ShardError};
pub(super) const MIN_UNIX: i64 = -62_135_596_800_000;
const MAX_UNIX: i64 = 253_402_300_799_999;
pub(super) fn validate(clock: ShardClock) -> Result<(), ShardError> {
    if !(MIN_UNIX..=MAX_UNIX).contains(&clock.unix_millis)
        || clock.local_offset_seconds.unsigned_abs() > 86_400
    {
        Err(ShardError::Clock)
    } else {
        Ok(())
    }
}
pub(super) fn labels(unix: i64, offset: i32) -> Result<(String, String), ShardError> {
    if !(MIN_UNIX..=MAX_UNIX).contains(&unix) {
        return Err(ShardError::Clock);
    }
    Ok((
        format_date((unix + i64::from(offset) * 1000).clamp(MIN_UNIX, MAX_UNIX)),
        format_date(unix),
    ))
}
fn format_date(unix: i64) -> String {
    let seconds = unix.div_euclid(1000);
    let day_seconds = seconds.rem_euclid(86400);
    // Proleptic Gregorian civil date from an explicit Unix day, including year1.
    let z = seconds.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = day_seconds / 3600;
    format!(
        "{year:04}-{month:02}-{day:02} {}:{:02}:{:02} {}",
        if hour % 12 == 0 { 12 } else { hour % 12 },
        day_seconds / 60 % 60,
        day_seconds % 60,
        if hour < 12 { "AM" } else { "PM" }
    )
}
pub(super) fn duration(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    let mut text = String::new();
    if h > 0 {
        text.push_str(&format!("{h} hour{}", if h == 1 { "" } else { "s" }));
    }
    if m > 0 {
        if !text.is_empty() {
            text.push_str(", ");
        }
        text.push_str(&format!("{m} minute{}", if m == 1 { "" } else { "s" }));
    }
    if s > 0 {
        if !text.is_empty() {
            text.push_str(" and ");
        }
        text.push_str(&format!("{s} second{}", if s == 1 { "" } else { "s" }));
    }
    text
}
pub(super) fn notice(sender: &str, millis: u64, immediate: bool) -> String {
    if immediate {
        return format!(
            "Broadcast from {sender}> ATTENTION - This Asheron's Call Server is shutting down NOW!!!!"
        );
    }
    let punctuation = if millis <= 60_000 { "!" } else { "." };
    format!(
        "Broadcast from {sender}> {} - This Asheron's Call Server will be shutting down in {}{punctuation}{}",
        if millis > 90_000 {
            "ATTENTION"
        } else {
            "WARNING"
        },
        duration(millis / 1000),
        if millis <= 180_000 {
            format!(" Please log out{punctuation}")
        } else {
            String::new()
        }
    )
}
