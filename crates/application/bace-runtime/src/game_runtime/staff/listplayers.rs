//! Pinned DeveloperCommands.HandleListPlayers, bound to entered sessions.
use super::*;
use bace_admin::StaffOperation;
use bace_auth::{AccessLevel, StaffPrincipal};

impl GameRuntime {
    pub(in crate::game_runtime::staff) fn apply_staff_listplayers(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        command: &bace_admin::AuthorizedCommand,
        principal: StaffPrincipal,
    ) -> Result<bool, String> {
        if command.spec.name != "listplayers" {
            return Ok(false);
        }
        let Some(StaffOperation::ListPlayers { access }) =
            bace_admin::prepare_staff_operation(command)
                .map_err(|error| format!("staff listplayers parameters: {error:?}"))?
        else {
            return Err("staff listplayers operation mismatch".into());
        };
        let issuer = self
            .sessions
            .get(&key)
            .ok_or("staff listplayers issuer missing")?;
        let loaded = issuer
            .loading
            .as_ref()
            .ok_or("staff listplayers issuer not entered")?;
        if !issuer.connected
            || issuer.disconnected
            || issuer.terminated
            || loaded.loaded.binding.actor != context.actor
            || loaded.loaded.binding.account != context.account
            || loaded.loaded.binding.session != context.session
            || issuer.account.id != context.account
            || issuer.account.access_level != principal.account_access
            || !principal.allows(AccessLevel::Developer, command.sudo)
            || !matches!(loaded.phase, lifecycle::Phase::Entered)
            || !self.players.entered(context.actor)
        {
            return Err("staff listplayers issuer authorization changed".into());
        }
        let binding = loaded.loaded.binding;
        let filter = parse_access_filter(access.as_deref());
        let lines = match filter {
            Err(()) => vec!["Invalid AccessLevel value".to_owned()],
            Ok(filter) => {
                let mut lines = Vec::new();
                if let Some((_, label)) = &filter {
                    lines.push(format!("Listing only {label}s:\n"));
                }
                let mut count = 0usize;
                for (candidate, session) in &self.sessions {
                    let Some(player) = session.loading.as_ref() else {
                        continue;
                    };
                    let player_binding = player.loaded.binding;
                    if !session.connected
                        || session.disconnected
                        || session.terminated
                        || !matches!(player.phase, lifecycle::Phase::Entered)
                        || !self.players.entered(player_binding.actor)
                        || self
                            .players
                            .replication(player_binding.actor)
                            .is_none_or(|replica| {
                                replica.key != *candidate || replica.binding != player_binding
                            })
                        || player_binding.account != session.account.id
                        || player_binding.session.0 != candidate.generation
                        || filter
                            .as_ref()
                            .is_some_and(|(level, _)| *level != session.account.access_level as i32)
                    {
                        continue;
                    }
                    count += 1;
                    if count > self.limits.sessions {
                        return Err("staff listplayers online capacity".into());
                    }
                    lines.push(format!(
                        "{} : {}\n",
                        player.loaded.player.player.name, session.account.id.0
                    ));
                }
                lines.push(format!("Total connected Players: {count}\n"));
                lines
            }
        };
        let messages = chunk_source_lines(&lines, 4000)?;
        if self.staff.output.len().saturating_add(messages.len()) > self.limits.messages {
            return Err("staff listplayers output capacity".into());
        }
        let replica = self
            .players
            .replication(context.actor)
            .ok_or("staff listplayers recipient missing")?;
        if replica.key != key || replica.binding != binding {
            return Err("staff listplayers recipient changed".into());
        }
        let limits = bace_replication::BatchLimits {
            max_messages: 1,
            max_bytes: self.limits.message_bytes,
            max_message_bytes: self.limits.message_bytes,
            max_string_bytes: 4000,
        };
        let commands = messages
            .iter()
            .map(|message| {
                let batch =
                    bace_replication::project_staff_response(replica.binding, message, 0, limits)
                        .map_err(|error| format!("staff listplayers response: {error:?}"))?;
                crate::game_messages::session_batch_command(replica.key, batch)
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.staff.output.extend(commands);
        Ok(true)
    }
}

fn parse_access_filter(raw: Option<&str>) -> Result<Option<(i32, String)>, ()> {
    let Some(raw) = raw else { return Ok(None) };
    let raw = raw.trim();
    let value = match raw.to_ascii_lowercase().as_str() {
        "player" => 0,
        "advocate" => 1,
        "sentinel" => 2,
        "envoy" => 3,
        "developer" => 4,
        "admin" => 5,
        // ACE first calls Enum.TryParse on the signed Int32-backed enum. This
        // accepts unnamed values, including negative values, before its
        // UInt16 conversion fallback is reached.
        _ => raw.parse::<i32>().map_err(|_| ())?,
    };
    let label = match value {
        0 => "Player".into(),
        1 => "Advocate".into(),
        2 => "Sentinel".into(),
        3 => "Envoy".into(),
        4 => "Developer".into(),
        5 => "Admin".into(),
        _ => value.to_string(),
    };
    Ok(Some((value, label)))
}

fn chunk_source_lines(lines: &[String], max: usize) -> Result<Vec<String>, String> {
    if max == 0 || max > 4096 {
        return Err("staff listplayers text bound".into());
    }
    let mut result = Vec::new();
    let mut current = String::new();
    for line in lines {
        if line.len() > max {
            return Err("staff listplayers single line exceeds bound".into());
        }
        if current.len().saturating_add(line.len()) > max {
            result.push(std::mem::take(&mut current));
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        result.push(current);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
