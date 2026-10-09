//! Native coverage labels deliberately distinguish catalog provenance from execution.
use super::*;
use bace_admin::{
    CommandCompatibility,
    command_catalog::{CommandSpec, commands},
    command_compatibility,
};
fn status(spec: &CommandSpec) -> String {
    match command_compatibility(spec) {
        CommandCompatibility::SourceTodo => "unsupported: source TODO".into(),
        CommandCompatibility::Incompatible(reason) => {
            format!("intentionally incompatible: {reason}")
        }
        CommandCompatibility::NativeOwnerRequired => match spec.name {
            "teleto" | "teletome" | "telereturn" | "telepoi" | "teledungeon" | "telexyz"
            | "teledist" | "teleloc" => "parsed only; live owner route unavailable".into(),
            "regen" => {
                "native selected-generator route; generator lifecycle retains effects".into()
            }
            "targetloc" => {
                "native selected-target four-line route; explicit GUID lookup unsupported".into()
            }
            "getenchantments" => {
                "native selected-target route; target owner state may be unavailable".into()
            }
            "myloc" => "native authoritative three-line self-position route".into(),
            "gps" => "native authoritative one-line Developer GPS route".into(),
            "whoami" => "native private Developer GUID inspection route".into(),
            "listplayers" => {
                "native entered-session Developer listing; host Console route unavailable".into()
            }
            "time" => "native in-world route; host console route unavailable".into(),
            "heal" => "native player-heal route; source rejection chat".into(),
            "boot" => {
                "native online-session boot route; final packet and Audit feed retained".into()
            }
            "ban" | "unban" | "banlist" => {
                "native durable account-ban route; Audit and online boot follow receipt".into()
            }
            "gag"
            | "ungag"
            | "acecommands"
            | "acehelp"
            | "myiid"
            | "grantxp"
            | "run"
            | "castspell"
            | "buff"
            | "fellowbuff"
            | "addspell"
            | "removespell"
            | "accountcreate"
            | "accountget"
            | "set-accountaccess"
            | "set-accountpassword"
            | "passwd"
            | "cancel-shutdown"
            | "set-shutdown-interval"
            | "stop-now"
            | "shutdown"
            | "world"
            | "gamecast"
            | "gamecastlocal"
            | "gamecastemote"
            | "gamecastlocalemote"
            | "we" => {
                "native route; required assets, target and durable receipts still apply".into()
            }
            _ => "compatible candidate; live owner not wired".into(),
        },
    }
}
impl GameRuntime {
    pub(in crate::game_runtime::staff) fn apply_staff_help(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        command: &bace_admin::AuthorizedCommand,
        principal: bace_auth::StaffPrincipal,
    ) -> Result<bool, String> {
        if !matches!(command.spec.name, "acehelp" | "acecommands") {
            return Ok(false);
        }
        if !self.staff.output.is_empty() {
            return Err("staff help output is still pending".into());
        }
        let query = command.arguments.first().map(|s| s.to_lowercase());
        let exact_access = query
            .as_deref()
            .and_then(|q| match q {
                "player" | "0" => Some(0),
                "advocate" | "1" => Some(1),
                "sentinel" | "2" => Some(2),
                "envoy" | "3" => Some(3),
                "developer" | "4" => Some(4),
                "admin" | "5" => Some(5),
                _ => None,
            })
            .filter(|level| *level <= principal.account_access as u8);
        let mut specs: Vec<_> = commands()
            .filter(|spec| {
                !spec.console_only
                    && exact_access.map_or(
                        spec.access as u8 <= principal.account_access as u8,
                        |level| spec.access as u8 == level,
                    )
            })
            .filter(|spec| {
                exact_access.is_some()
                    || query.as_ref().is_none_or(|query| {
                        query == "commands"
                            || spec.name.to_lowercase().contains(query)
                            || spec.description.to_lowercase().contains(query)
                            || format!("{:?}", spec.access).to_lowercase().contains(query)
                    })
            })
            .collect();
        specs.sort_by_key(|s| s.name);
        let mut lines=vec!["BetterACE command catalog: source metadata is not a parity or execution claim. Status labels below describe the currently wired native routes.".into()];
        for spec in specs {
            lines.push(format!(
                "@{} [{}] - {}",
                spec.name,
                status(spec),
                spec.description
            ));
            if command.spec.name == "acehelp"
                && query.as_deref() == Some(spec.name)
                && !spec.usage.is_empty()
            {
                lines.push(spec.usage.into());
            }
        }
        let replica = self
            .players
            .replication(context.actor)
            .ok_or("staff help recipient missing")?;
        if replica.key != key
            || replica.binding.session != context.session
            || replica.binding.account != context.account
        {
            return Err("staff help binding mismatch".into());
        }
        let event = StaffEvent::Inspection {
            context,
            target: context.actor,
            lines,
        };
        let batch = bace_replication::project_staff_text(
            replica.binding,
            &event,
            bace_replication::BatchLimits {
                max_messages: 1024,
                max_bytes: self.limits.message_bytes,
                max_message_bytes: self.limits.message_bytes,
                max_string_bytes: 8192,
            },
        )
        .map_err(|e| format!("staff help bounds: {e:?}"))?;
        self.staff.output.push_back(
            crate::game_messages::session_batch_command(key, batch).map_err(|e| e.to_string())?,
        );
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn help_does_not_label_catalog_or_selection_as_implemented() {
        for (name, expected) in [
            ("forcegc", "intentionally incompatible"),
            ("deaf", "source TODO"),
            ("gag", "native route"),
            ("heal", "native player-heal route"),
            ("boot", "native online-session boot route"),
            ("gamecast", "native route"),
            ("myiid", "native route"),
            ("whoami", "private Developer GUID inspection route"),
            ("listplayers", "entered-session Developer listing"),
            ("myloc", "three-line self-position route"),
            ("gps", "one-line Developer GPS route"),
            ("time", "native in-world route"),
            ("targetloc", "selected-target four-line route"),
            ("getenchantments", "selected-target route"),
            ("teleloc", "parsed only"),
            ("regen", "native selected-generator route"),
        ] {
            assert!(status(bace_admin::command_catalog::command(name).unwrap()).contains(expected));
        }
    }
}
