//! Player age text, ported from the pinned GDLE `CClientEvents` `GetAgeString`
//! (gdlenhanced 353cbab, Source/ClientEvents.cpp lines 69-88).
//!
//! The port writes only the unit body, `2d 17h 21m 25s`. The fixed `You have played for `
//! prefix and trailing period belong to the presentation layer; the retail 0x01C3 text
//! carries the body alone, and the client adds the rest.
//!
//! Units follow GDLE exactly: months of 2,629,800 s (30.4375 days), then days, hours and
//! minutes, each printed only when non-zero, and seconds always printed.
//!
//! Retail month behaviour is unobserved. The retail corpus evidence (`2d 17h 21m 25s`,
//! 45 instances) contains no month, so months come from GDLE and are not retail-verified.
//! Years and weeks (ACE's `y`/`w`) are deliberately absent.
//!
//! GDLE stores age in a signed `int`; values from 2^31 upward wrap negative there. This
//! port takes the full `u32` and decomposes it without sign, so it does not reproduce the
//! GDLE wrap. The oracle fixture records those inputs as `signed_wrap` rows.

const SECONDS_PER_MONTH: u32 = 2_629_800;
const SECONDS_PER_DAY: u32 = 86_400;
const SECONDS_PER_HOUR: u32 = 3_600;
const SECONDS_PER_MINUTE: u32 = 60;

/// Replaces the contents of `out` with the age body for `seconds`.
///
/// Writes into the caller's buffer and never allocates on its own. The longest body is 22
/// bytes (for example `1633mo 30d 23h 59m 59s`), so a `String` with 24 bytes of capacity
/// never reallocates.
pub fn age_text(seconds: u32, out: &mut String) {
    out.clear();
    let months = seconds / SECONDS_PER_MONTH;
    let mut rest = seconds % SECONDS_PER_MONTH;
    let days = rest / SECONDS_PER_DAY;
    rest %= SECONDS_PER_DAY;
    let hours = rest / SECONDS_PER_HOUR;
    rest %= SECONDS_PER_HOUR;
    let minutes = rest / SECONDS_PER_MINUTE;
    let secs = rest % SECONDS_PER_MINUTE;

    let mut wrote_unit = false;
    for (value, suffix) in [(months, "mo"), (days, "d"), (hours, "h"), (minutes, "m")] {
        if value != 0 {
            if wrote_unit {
                out.push(' ');
            }
            push_decimal(out, value);
            out.push_str(suffix);
            wrote_unit = true;
        }
    }
    if wrote_unit {
        out.push(' ');
    }
    push_decimal(out, secs);
    out.push('s');
}

/// Appends the decimal digits of `value` without a formatting allocation.
fn push_decimal(out: &mut String, value: u32) {
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut rest = value;
    loop {
        digits[len] = b'0' + (rest % 10) as u8;
        len += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    for index in (0..len).rev() {
        out.push(char::from(digits[index]));
    }
}
