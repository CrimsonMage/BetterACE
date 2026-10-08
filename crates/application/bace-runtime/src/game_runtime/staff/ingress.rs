use super::*;
use crate::game_runtime::social::SocialIngress;
use crate::staff_game_dispatch::{PreparedStaffGame, StaffGameInput, prepare_staff_game_command};
use bace_admin::StaffTarget;
use bace_auth::{CharacterPrivileges, StaffPrincipal};
impl GameRuntime {
    pub(in crate::game_runtime) fn handle_staff_dispatch(
        &mut self,
        key: SessionKey,
        dispatch: GameplayDispatch,
    ) -> Result<SocialIngress, String> {
        if self.staff.has_pending() {
            return Ok(SocialIngress::Blocked);
        }
        let session = self.sessions.get(&key).ok_or("staff session missing")?;
        let loading = session.loading.as_ref().ok_or("staff player not loaded")?;
        let context = match &dispatch {
            GameplayDispatch::StaffLine { context, .. }
            | GameplayDispatch::Map { context, .. }
            | GameplayDispatch::TargetQuery { context, .. } => *context,
            _ => return Ok(SocialIngress::Unsupported),
        };
        if !self.players.entered(context.actor)
            || loading.loaded.binding.actor != context.actor
            || loading.loaded.binding.account != context.account
            || loading.loaded.binding.session != context.session
        {
            return Err("staff authenticated binding mismatch".into());
        }
        let token = self.token()?;
        let request = match dispatch {
            GameplayDispatch::TargetQuery { kind, target, .. } => StaffRequest::Query(kind, target),
            GameplayDispatch::Map { request, .. } => StaffRequest::Map(request),
            GameplayDispatch::StaffLine { line, .. } => StaffRequest::Line(line),
            _ => unreachable!(),
        };
        self.staff.pending = Some(Pending {
            key,
            context,
            token,
            phase: Phase::Capture(request),
        });
        self.staff.failure = None;
        Ok(SocialIngress::Accepted)
    }
    pub(super) fn prepare_staff_captured(
        &self,
        pending: &Pending,
        request: &StaffRequest,
        snapshot: &bace_simulation::PlayerReadSnapshot,
        unix: u64,
    ) -> Result<Phase, String> {
        let registration = snapshot
            .staff()
            .ok_or("staff live registration unavailable")?;
        if registration.binding != snapshot.binding() {
            return Err("staff live registration binding mismatch".into());
        }
        let p = registration.privileges;
        let principal = StaffPrincipal {
            account_access: bace_auth::AccessLevel::try_from(p.account_access)
                .map_err(|_| "staff access level")?,
            in_world: true,
            character: CharacterPrivileges {
                advocate: p.advocate,
                sentinel: p.sentinel,
                envoy: p.envoy,
                developer: p.developer,
                admin: p.admin,
                psr: p.psr,
            },
        };
        let context = pending.context;
        let token = pending.token;
        let phase = match request {
            StaffRequest::Query(kind, target) => {
                let mana = if *kind == bace_gameplay_api::selection::TargetQueryKind::ItemMana {
                    if let Some(item) = snapshot.items().iter().find(|i| i.id == *target) {
                        let sources = self.online_saves.captured_inventory_baselines(snapshot)?;
                        let saved = sources
                            .iter()
                            .find(|s| {
                                s.entity.object_id == target.0
                                    && s.entity.state.weenie_id == item.template
                            })
                            .ok_or("query item source unavailable")?;
                        let property = |id| {
                            saved
                                .entity
                                .state
                                .properties
                                .ints
                                .iter()
                                .find(|p| p.id == id)
                                .map(|p| p.value)
                        };
                        Some(bace_gameplay_api::selection::PreparedItemManaQuery {
                            revision: item.revision,
                            current: property(107),
                            maximum: property(108),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
                Phase::Command(Box::new(StaffCommand {
                    token,
                    action: bace_gameplay_api::staff::StaffAction::QueryTarget {
                        context,
                        kind: *kind,
                        target: *target,
                        mana,
                    },
                }))
            }

            StaffRequest::Map(request) => {
                if !principal.character.map_teleport() {
                    return Err("map teleport is not authorized".into());
                }
                Phase::MapPrepare(StaffMapWork {
                    token,
                    request: *request,
                    expected_epoch: snapshot.entry_physics().epoch(),
                })
            }
            StaffRequest::Line(line) => {
                // Names resolve only to accepted online bindings; no synthetic
                // player presence or guessed target identity enters the owner.
                let mut names = BTreeMap::new();
                for session in self.sessions.values() {
                    if let Some(loading) = &session.loading
                        && self.players.entered(loading.loaded.binding.actor)
                    {
                        names.insert(
                            loading.loaded.player.player.name.to_lowercase(),
                            loading.loaded.binding.actor,
                        );
                    }
                }
                let equipment = if staff_equipment_needed(line) {
                    let baselines = self.online_saves.inventory_baselines(context.actor.0);
                    snapshot.items().iter().filter(|item|matches!(item.place,bace_inventory::ItemPlace::Contained{equipped,..} if equipped!=0)).map(|item|{
                        let source=baselines.iter().find(|source|source.entity.object_id==item.id.0 && source.entity.state.weenie_id==item.template).ok_or("staff equipment source missing")?;
                        crate::staff_game_dispatch::prepare_staff_bane_item(item.id,item.revision,&source.entity.state)
                    }).collect::<Result<Vec<_>,String>>()?
                } else {
                    Vec::new()
                };
                let prepared = prepare_staff_game_command(
                    StaffGameInput {
                        line,
                        principal,
                        context,
                        token,
                        definitions: &self.staff.definitions,
                        equipment,
                    },
                    |target| match target {
                        StaffTarget::SelfActor => Ok(context.actor),
                        StaffTarget::Named(name) => names
                            .get(&name.trim_start_matches('+').to_lowercase())
                            .copied()
                            .ok_or_else(|| "staff named player is not online".into()),
                        StaffTarget::Selected => snapshot
                            .target_selection()
                            .ok_or("staff selection owner unavailable")?
                            .requested_appraisal
                            .ok_or_else(|| "GetLastAppraisedObject() - no appraisal target".into()),
                        StaffTarget::SelectedGenerator => snapshot
                            .target_selection()
                            .ok_or_else(|| "staff selection owner unavailable".to_owned())
                            .map(|selection| {
                                // ACE HandleRegen silently does nothing with no selection.
                                // EntityId(0) is the explicit no-target operation input.
                                selected_generator_target(&selection)
                                    .unwrap_or(bace_types::EntityId(0))
                            }),
                        StaffTarget::SelectedOrSelf => {
                            let selected = snapshot
                                .target_selection()
                                .ok_or("staff selection owner unavailable")?;
                            Ok(selected
                                .health
                                .or(selected.mana)
                                .or(selected.current_appraisal)
                                .unwrap_or(context.actor))
                        }
                    },
                )?;
                match prepared {
                    PreparedStaffGame::Simulation(mut command) => {
                        if let bace_gameplay_api::staff::StaffAction::Heal {
                            target,
                            target_name,
                            ..
                        } = &mut command.action
                            && let Some(name) =
                                self.visibility.service.registered_object_name(*target)
                        {
                            if name.is_empty() || name.len() > 2048 {
                                return Err("staff heal accepted target name bounds".into());
                            }
                            *target_name = Some(name.to_owned());
                        }
                        self.prepare_staff_native(pending, request, command, snapshot, unix)?
                    }
                    PreparedStaffGame::Other(command) => Phase::Other {
                        line: line.clone(),
                        command,
                        principal,
                    },
                }
            }
        };
        Ok(phase)
    }
}

fn selected_generator_target(
    selection: &bace_gameplay_api::selection::TargetSelection,
) -> Option<bace_types::EntityId> {
    selection
        .health
        .or(selection.mana)
        .or(selection.current_appraisal)
}

fn staff_equipment_needed(line: &str) -> bool {
    let Ok(parsed) = bace_admin::parse_command(line) else {
        return false;
    };
    let name = if parsed.name.eq_ignore_ascii_case("sudo") {
        parsed.arguments.first().map(String::as_str)
    } else {
        Some(parsed.name.as_str())
    };
    name.is_some_and(|name| {
        name.eq_ignore_ascii_case("buff") || name.eq_ignore_ascii_case("fellowbuff")
    })
}

#[cfg(test)]
mod generator_selection_tests {
    use super::*;

    #[test]
    fn ace_regen_uses_health_then_mana_then_current_appraisal() {
        let mut selected = bace_gameplay_api::selection::TargetSelection {
            health: Some(bace_types::EntityId(1)),
            mana: Some(bace_types::EntityId(2)),
            requested_appraisal: Some(bace_types::EntityId(3)),
            current_appraisal: Some(bace_types::EntityId(4)),
            ..Default::default()
        };
        assert_eq!(selected_generator_target(&selected), selected.health);
        selected.health = None;
        assert_eq!(selected_generator_target(&selected), selected.mana);
        selected.mana = None;
        assert_eq!(
            selected_generator_target(&selected),
            selected.current_appraisal
        );
        selected.current_appraisal = None;
        assert_eq!(selected_generator_target(&selected), None);
    }

    #[test]
    fn only_buff_family_needs_equipment_source_preparation() {
        assert!(staff_equipment_needed("@buff"));
        assert!(staff_equipment_needed("@fellowbuff"));
        assert!(staff_equipment_needed("sudo buff"));
        assert!(staff_equipment_needed("sudo FELLOWBUFF"));
        for line in ["@regen", "@myiid", "@time", "@heal", "@acehelp buff"] {
            assert!(!staff_equipment_needed(line));
        }
    }
}
