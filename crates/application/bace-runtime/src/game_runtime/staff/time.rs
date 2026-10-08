//! ACE AdminCommands.HandleTime: three private WorldBroadcast chat messages.
//! The adapter supplies its captured UTC and portal clocks; the world owner
//! never reads wall time.
use super::*;

impl GameRuntime {
    pub(in crate::game_runtime::staff) fn apply_staff_time(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        command: &bace_admin::AuthorizedCommand,
    ) -> Result<bool, String> {
        if command.spec.name != "time" {
            return Ok(false);
        }
        if !matches!(
            bace_admin::prepare_staff_operation(command),
            Ok(Some(bace_admin::StaffOperation::Time))
        ) {
            return Err("staff time operation mismatch".into());
        }
        let elapsed = u64::try_from(self.last_elapsed.as_millis())
            .map_err(|_| "staff time elapsed overflow")?;
        let unix_millis = self
            .clock
            .unix_millis
            .checked_add(elapsed)
            .ok_or("staff UTC clock overflow")?;
        let portal_seconds = crate::network::PortalClock::new(self.clock.portal_origin, 0)
            .and_then(|clock| clock.at(elapsed))
            .map_err(str::to_owned)?;
        let messages = source_time_messages(unix_millis, portal_seconds)?;
        let replica = self
            .players
            .replication(context.actor)
            .ok_or("staff time recipient missing")?;
        if replica.key != key
            || replica.binding.actor != context.actor
            || replica.binding.session != context.session
            || replica.binding.account != context.account
        {
            return Err("staff time binding mismatch".into());
        }
        let limits = bace_replication::BatchLimits {
            max_messages: 1,
            max_bytes: self.limits.message_bytes,
            max_message_bytes: self.limits.message_bytes,
            max_string_bytes: 4096,
        };
        let commands = messages
            .iter()
            .map(|text| {
                let batch =
                    bace_replication::project_staff_response(replica.binding, text, 20, limits)
                        .map_err(|e| format!("staff time projection: {e:?}"))?;
                crate::game_messages::session_batch_command(replica.key, batch)
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.staff.output.extend(commands);
        Ok(true)
    }
}

fn source_time_messages(unix_millis: u64, portal_seconds: f64) -> Result<[String; 3], String> {
    let utc = source_utc(unix_millis)?;
    let (date, daytime) = source_dereth(portal_seconds)?;
    Ok([
        format!("The current server time in UtcNow is: {utc}"),
        format!("The current server time shown in game client is:\n{date}"),
        format!("It is currently {daytime} in game right now."),
    ])
}

/// ACE DateTimeExtensions.ToCommonString: `yyyy-MM-dd h:mm:ss tt` in UTC.
fn source_utc(unix_millis: u64) -> Result<String, String> {
    let seconds = unix_millis / 1000;
    let days = i64::try_from(seconds / 86_400).map_err(|_| "staff UTC day overflow")?;
    let second_of_day = seconds % 86_400;
    // Gregorian civil date from Unix day, with 400-year eras. Integer arithmetic
    // is bounded by the captured server clock and has no OS or locale dependency.
    let z = days.checked_add(719_468).ok_or("staff UTC era overflow")?;
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    if !(1..=9999).contains(&year) {
        return Err("staff UTC year outside DateTime range".into());
    }
    let hour = second_of_day / 3600;
    let minute = second_of_day / 60 % 60;
    let second = second_of_day % 60;
    let twelve_hour = (hour + 11) % 12 + 1;
    let meridiem = if hour < 12 { "AM" } else { "PM" };
    Ok(format!(
        "{year:04}-{month:02}-{day:02} {twelve_hour}:{minute:02}:{second:02} {meridiem}"
    ))
}

/// ACE DerethDateTime.SetDateTimeFromTicks rounds to a quarter hour, then
/// advances from Morningthaw 1, 10 P.Y. at Morntide-and-Half. The source loop
/// is replaced by equivalent bounded arithmetic.
fn source_dereth(portal_seconds: f64) -> Result<(String, &'static str), String> {
    if !portal_seconds.is_finite()
        || !(0.0..=crate::network::PortalClock::MAX_SECONDS).contains(&portal_seconds)
    {
        return Err("staff portal clock outside ACE range".into());
    }
    const MONTHS: [&str; 12] = [
        "Morningthaw",
        "Solclaim",
        "Seedsow",
        "Leafdawning",
        "Verdantine",
        "Thistledown",
        "HarvestGain",
        "Leafcull",
        "Frostfell",
        "Snowreap",
        "Coldeve",
        "Wintersebb",
    ];
    const HOURS: [&str; 16] = [
        "Darktide",
        "Darktide-and-Half",
        "Foredawn",
        "Foredawn-and-Half",
        "Dawnsong",
        "Dawnsong-and-Half",
        "Morntide",
        "Morntide-and-Half",
        "Midsong",
        "Midsong-and-Half",
        "Warmtide",
        "Warmtide-and-Half",
        "Evensong",
        "Evensong-and-Half",
        "Gloaming",
        "Gloaming-and-Half",
    ];
    let (year, month, day, hour) = if portal_seconds == crate::network::PortalClock::MAX_SECONDS {
        // DerethDateTime.Ticks handles the exact MaxValue before its loop.
        (401, 5, 2, 8)
    } else {
        let hours = portal_seconds / (7620.0 / 16.0);
        let rounded = (hours * 4.0).round_ties_even() / 4.0;
        let fraction = ((rounded - rounded.trunc()) * 100.0) as u32;
        let increments = rounded.ceil() as u64 - u64::from(fraction == 25);
        let absolute_hour = 7 + increments;
        let elapsed_days = absolute_hour / 16;
        let elapsed_months = elapsed_days / 30;
        (
            10 + elapsed_months / 12,
            (elapsed_months % 12) as usize,
            elapsed_days % 30 + 1,
            (absolute_hour % 16 + 1) as usize,
        )
    };
    let daytime = if (5..=12).contains(&hour) {
        "Day"
    } else {
        "Night"
    };
    Ok((
        format!(
            "Date: {} {day}, {year} P.Y.  Time: {}",
            MONTHS[month],
            HOURS[hour - 1]
        ),
        daytime,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ace_time_three_world_broadcast_texts_at_epoch() {
        // ACE AdminCommands.HandleTime and DerethDateTime tick-zero state.
        assert_eq!(
            source_time_messages(0, 0.).unwrap(),
            [
                "The current server time in UtcNow is: 1970-01-01 12:00:00 AM",
                "The current server time shown in game client is:\nDate: Morningthaw 1, 10 P.Y.  Time: Morntide-and-Half",
                "It is currently Day in game right now.",
            ]
        );
        assert_eq!(
            source_dereth(7620.).unwrap(),
            (
                "Date: Morningthaw 2, 10 P.Y.  Time: Morntide-and-Half".into(),
                "Day"
            )
        );
        assert_eq!(
            source_dereth(7620. * 360.).unwrap(),
            (
                "Date: Morningthaw 1, 11 P.Y.  Time: Morntide-and-Half".into(),
                "Day"
            )
        );
        assert_eq!(
            source_dereth(crate::network::PortalClock::MAX_SECONDS).unwrap(),
            (
                "Date: Thistledown 2, 401 P.Y.  Time: Morntide-and-Half".into(),
                "Day"
            )
        );
    }

    #[test]
    fn ace_time_calendar_boundaries_and_invalid_clock() {
        assert_eq!(source_utc(86_400_000).unwrap(), "1970-01-02 12:00:00 AM");
        assert_eq!(source_utc(46_800_000).unwrap(), "1970-01-01 1:00:00 PM");
        assert_eq!(
            source_dereth(7620. * 30.).unwrap().0,
            "Date: Solclaim 1, 10 P.Y.  Time: Morntide-and-Half"
        );
        assert!(source_dereth(f64::NAN).is_err());
        assert!(source_dereth(crate::network::PortalClock::MAX_SECONDS + 1.).is_err());
    }
}
