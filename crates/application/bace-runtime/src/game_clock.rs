//! Generator day/night from pinned ACE.Common.DerethDateTime. The source loops
//! through rounded hours; arithmetic below produces the same wrapped hour without
//! work proportional to server age. Explicit portal seconds are the only input.
pub fn generator_hour(portal_seconds: f64) -> Result<u8, &'static str> {
    let maximum = crate::network::PortalClock::MAX_SECONDS;
    if !portal_seconds.is_finite() || !(0.0..=maximum).contains(&portal_seconds) {
        return Err("generator portal clock outside admitted range");
    }
    if portal_seconds == maximum {
        return Ok(8);
    }
    let hours = portal_seconds / (7620.0 / 16.0);
    let rounded = (hours * 4.0).round_ties_even() / 4.0;
    let fraction = ((rounded - rounded.trunc()) * 100.0) as u32;
    let increments = rounded.ceil() as u64 - u64::from(fraction == 25);
    Ok(((7 + increments) % 16 + 1) as u8)
}
pub fn generator_is_day(portal_seconds: f64) -> Result<bool, &'static str> {
    Ok((5..=12).contains(&generator_hour(portal_seconds)?))
}
#[cfg(test)]
#[path = "game_clock_tests.rs"]
mod tests;
