//! Pinned source handler ordering; one immutable output batch before adoption.
use super::{
    ShardChat, ShardClock, ShardDrainKind, ShardEffect, ShardError, ShardIssuer, ShardShutdown,
    State, time,
};
use bace_admin::ShardOperation;
pub(super) fn apply(
    state: &mut State,
    op: &ShardOperation,
    issuer: ShardIssuer<'_>,
    clock: ShardClock,
    out: &mut Vec<ShardEffect>,
    drains: &mut Vec<ShardDrainKind>,
) -> Result<(), ShardError> {
    let name = issuer.name();
    match op {
        ShardOperation::CancelShutdown => {
            let (local, utc) = time::labels(state.deadline_unix, clock.local_offset_seconds)?;
            let text = format!(
                "{name} has requested the pending shut down @ {local} ({utc} UTC) be cancelled."
            );
            log(out, text.clone());
            audit(out, issuer, text);
            if matches!(
                state.shutdown,
                ShardShutdown::Draining(_) | ShardShutdown::Complete
            ) {
                reply(
                    out,
                    issuer,
                    ShardChat::Broadcast,
                    "Shutdown has entered durable draining and cannot be cancelled.".into(),
                );
            } else {
                if matches!(state.shutdown, ShardShutdown::Countdown { .. }) {
                    let (local, utc) = time::labels(clock.unix_millis, clock.local_offset_seconds)?;
                    log(
                        out,
                        format!("The server shut down has been cancelled @ {local} ({utc} UTC)"),
                    );
                    out.push(ShardEffect::Broadcast {text:"Broadcast from System> ATTENTION - This Asheron's Call Server shut down has been cancelled.".into()});
                }
                state.shutdown = ShardShutdown::Idle;
                state.deadline_unix = time::MIN_UNIX;
            }
        }
        ShardOperation::SetShutdownInterval(interval) => match interval {
            Some(interval) => {
                audit(
                    out,
                    issuer,
                    format!(
                        "{name} has requested the shut down interval be changed from {} seconds to {interval} seconds.",
                        state.interval
                    ),
                );
                set_interval(state, out, *interval);
                reply(
                    out,
                    issuer,
                    ShardChat::Broadcast,
                    format!(
                        "Shutdown Interval (seconds to shutdown server) has been set to {interval}."
                    ),
                );
            }
            None => reply(
                out,
                issuer,
                ShardChat::Broadcast,
                "Usage: /set-shutdown-interval <00000>".into(),
            ),
        },
        ShardOperation::StopNow { message } => {
            audit(
                out,
                issuer,
                format!("{name} has initiated an immediate server shut down."),
            );
            set_interval(state, out, 0);
            shutdown(state, issuer, clock, message, out)?;
        }
        ShardOperation::Shutdown { message } => shutdown(state, issuer, clock, message, out)?,
        ShardOperation::World { open, boot } => {
            let text = match open {
                Some(true) if state.world_open => "World is already open.".into(),
                Some(false) if !state.world_open => "World is already closed.".into(),
                Some(true) => "Opening world to players...".into(),
                Some(false) if *boot => "Closing world, and booting all online players.".into(),
                Some(false) => "Closing world...".into(),
                None => format!(
                    "World is currently {}\nPlease specify state to change\n@world [open | close] <boot>\nIf closing world, using @world close boot will force players to logoff immediately",
                    if state.world_open { "Open" } else { "Closed" }
                ),
            };
            reply(out, issuer, ShardChat::WorldBroadcast, text);
            if let Some(open) = open
                && *open != state.world_open
            {
                state.world_open = *open;
                audit(
                    out,
                    issuer,
                    if *open {
                        "World is now open"
                    } else if *boot {
                        "World is now closed, and booting all online players."
                    } else {
                        "World is now closed"
                    }
                    .into(),
                );
                if !open && *boot {
                    drains.push(ShardDrainKind::BootOrdinaryPlayers);
                }
            }
        }
    }
    Ok(())
}
fn set_interval(state: &mut State, out: &mut Vec<ShardEffect>, interval: u32) {
    state.interval = interval;
    log(out, format!("Server shutdown interval reset: {interval}"));
}
fn shutdown(
    state: &mut State,
    issuer: ShardIssuer<'_>,
    clock: ShardClock,
    message: &str,
    out: &mut Vec<ShardEffect>,
) -> Result<(), ShardError> {
    if state.shutdown != ShardShutdown::Idle {
        reply(
            out,
            issuer,
            ShardChat::Broadcast,
            "Shutdown is already in progress.".into(),
        );
        return Ok(());
    }
    let interval = u64::from(state.interval) * 1000;
    let deadline = clock
        .monotonic_millis
        .checked_add(interval)
        .and_then(|d| d.checked_add(1000).map(|_| d))
        .ok_or(ShardError::Clock)?;
    let unix = clock
        .unix_millis
        .checked_add(interval as i64)
        .ok_or(ShardError::Clock)?;
    time::labels(unix, clock.local_offset_seconds)?;
    let name = issuer.name();
    let (local, utc) = time::labels(clock.unix_millis, clock.local_offset_seconds)?;
    let text = format!("{name} initiated a complete server shutdown @ {local} ({utc} UTC)");
    log(out, text.clone());
    log(
        out,
        format!(
            "The server will shut down in {}",
            if state.interval > 120 {
                format!("{} minutes.", state.interval / 60)
            } else {
                format!("{} seconds.", state.interval)
            }
        ),
    );
    audit(out, issuer, text);
    if !message.is_empty() {
        log(out, format!("Admin message: {message}"));
        audit(
            out,
            issuer,
            format!("{name} sent the following message for the shutdown: {message}"),
        );
    }
    let sender = if message.is_empty() || name == "CONSOLE" {
        "System"
    } else {
        &name
    };
    let generic = time::notice(sender, interval, interval == 0);
    out.push(ShardEffect::Broadcast {
        text: if message.is_empty() {
            generic
        } else {
            format!("Broadcast from {sender}> {message}\n{generic}")
        },
    });
    state.shutdown = ShardShutdown::Countdown {
        deadline_millis: deadline,
    };
    state.deadline_unix = unix;
    state.last_notice = clock.monotonic_millis;
    Ok(())
}
fn reply(out: &mut Vec<ShardEffect>, issuer: ShardIssuer<'_>, chat: ShardChat, text: String) {
    out.push(ShardEffect::Reply {
        recipient: issuer.actor(),
        chat,
        text,
    });
}
fn audit(out: &mut Vec<ShardEffect>, issuer: ShardIssuer<'_>, text: String) {
    out.push(ShardEffect::Audit {
        actor: issuer.actor(),
        text,
    });
}
fn log(out: &mut Vec<ShardEffect>, text: String) {
    out.push(ShardEffect::Log { text });
}
