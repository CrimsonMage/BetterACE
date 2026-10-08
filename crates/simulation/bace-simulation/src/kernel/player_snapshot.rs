//! Capture committed state without logging out or taking any gameplay owner.
use super::*;
impl Kernel {
    pub fn read_player_snapshot(
        &mut self,
        binding: CharacterBinding,
    ) -> Result<crate::PlayerReadSnapshot, CharacterRegistrationError> {
        self.capture_player_snapshot(binding, None)
    }
    pub fn read_player_operation_snapshot(
        &mut self,
        binding: CharacterBinding,
        operation: crate::PlayerSnapshotOperation,
        revision: u64,
    ) -> Result<crate::PlayerReadSnapshot, CharacterRegistrationError> {
        self.capture_player_snapshot(binding, Some((operation, revision)))
    }
    fn capture_player_snapshot(
        &mut self,
        binding: CharacterBinding,
        operation: Option<(crate::PlayerSnapshotOperation, u64)>,
    ) -> Result<crate::PlayerReadSnapshot, CharacterRegistrationError> {
        use CharacterRegistrationError as E;
        if self
            .equipment_mana
            .players
            .get(&binding.actor)
            .is_some_and(|p| p.entry_cleanup)
        {
            return Err(E::DurabilityPending);
        }
        if let Some((crate::PlayerSnapshotOperation::PhysicalAmmo(id), revision)) = operation
            && !self.validate_physical_resource_snapshot(id, binding, revision)
        {
            return Err(E::DurabilityPending);
        }
        if let Some((crate::PlayerSnapshotOperation::Portal(id), revision)) = operation
            && self.pending_portal_proposal(id).is_none_or(|ticket| {
                matches!(ticket.origin, crate::PortalServiceOrigin::Emote { .. })
                    || !ticket
                        .participants
                        .iter()
                        .any(|(actor, before, _)| *actor == binding.actor && *before == revision)
            })
        {
            return Err(E::DurabilityPending);
        }

        if let Some((crate::PlayerSnapshotOperation::Inventory(id), revision)) = operation
            && !self.validate_inventory_snapshot(binding, id, revision)
            && !self.validate_magic_resource_snapshot(binding, id, revision)
        {
            return Err(E::DurabilityPending);
        }

        if let Some((crate::PlayerSnapshotOperation::Pet(id), revision)) = operation
            && (!self.inventory.reserved(binding.actor)
                || self.inventory.pending_ticket(id).is_none_or(|ticket| {
                    ticket.actor != binding.actor
                        || !self.pets.pending.contains_key(&id)
                            && !self.pets.retiring.contains_key(&id)
                })
                || self
                    .characters
                    .get(binding.actor)
                    .is_none_or(|character| character.revision() != revision))
        {
            return Err(E::DurabilityPending);
        }

        if let Some((crate::PlayerSnapshotOperation::AttributeTransfer(id), revision)) = operation
            && self
                .attribute_transfers
                .pending
                .values()
                .find(|ticket| {
                    ticket.character.operation == id
                        && ticket.character.context.actor == binding.actor
                })
                .is_none_or(|ticket| {
                    ticket.character.proposal.expected_revision != revision
                        || self.inventory.pending_ticket(ticket.inventory.operation)
                            != Some(&ticket.inventory)
                        || !self.inventory.reserved(binding.actor)
                })
        {
            return Err(E::DurabilityPending);
        }

        if let Some((crate::PlayerSnapshotOperation::Crafting(id), revision)) = operation
            && self.crafting.pending(id).is_none_or(|ticket| {
                let expected = match &ticket.decision {
                    crate::CraftingDecision::Tinker(p) => p.expected_actor_revision,
                    crate::CraftingDecision::Salvage(p) => p.expected_actor_revision,
                };
                ticket.actor != binding.actor
                    || expected != revision
                    || !self.inventory.reserved(binding.actor)
            })
        {
            return Err(E::DurabilityPending);
        }
        if let Some((crate::PlayerSnapshotOperation::Npc { ticket }, revision)) = operation
            && !self.validate_npc_snapshot(ticket, binding.actor, revision)
        {
            return Err(E::DurabilityPending);
        }
        if let Some((crate::PlayerSnapshotOperation::NpcHandIn { operation }, revision)) = operation
            && !self.validate_npc_handin_snapshot(operation, binding.actor, revision)
        {
            return Err(E::DurabilityPending);
        }
        if let Some((op, revision)) = operation {
            self.characters
                .read_operation_snapshot(binding, op, revision)?;
        } else {
            self.characters.can_take_complete(binding)?;
        }
        if operation.is_none()
            && (self.world.has_reserved_vitals(binding.actor)
                || self.inventory.reserved(binding.actor)
                || self.npcs.pending_participant(binding.actor)
                || self.housing.reserved(binding.actor)
                || self.pets.reserved(binding.actor)
                || self.portals.reserved(binding.actor)
                || self.staff.pending_for(binding.actor))
        {
            return Err(E::DurabilityPending);
        }
        let ids: Vec<_> = self
            .inventory
            .items()
            .filter(|item| self.inventory.owned(binding.actor, item.id))
            .map(|item| item.id)
            .take(1025)
            .collect();
        if ids.len() > 1024 {
            return Err(E::Capacity);
        }
        if ids
            .iter()
            .copied()
            .chain(std::iter::once(binding.actor))
            .any(|id| {
                (operation.is_none() && self.magic.registry_reserved(id))
                    || self.magic.registry_failure(id).is_some()
            })
        {
            return Err(E::DurabilityPending);
        }
        let count: usize = ids
            .iter()
            .copied()
            .chain(std::iter::once(binding.actor))
            .filter_map(|id| self.magic.registry(id))
            .map(|r| r.entries().len())
            .sum();
        if count > 65536 {
            return Err(E::Capacity);
        }
        let item_experience: Vec<_> = ids
            .iter()
            .filter_map(|id| self.item_experience.items.get(id))
            .collect();
        let set_entries = item_experience
            .iter()
            .try_fold(0usize, |count, item| {
                item.set.as_ref().map_or(Some(count), |set| {
                    set.tiers
                        .values()
                        .try_fold(count, |sum, entries| sum.checked_add(entries.len()))
                })
            })
            .ok_or(E::Capacity)?;
        if set_entries > 65536 {
            return Err(E::Capacity);
        }
        let item_experience = item_experience.into_iter().cloned().collect();
        // These are accepted-state revision synchronization, not ownership transfer.
        self.sync_social_age().map_err(|_| E::DurabilityPending)?;
        self.sync_player_world().map_err(|_| E::DurabilityPending)?;
        self.sync_recovery_revisions()
            .map_err(|_| E::DurabilityPending)?;
        self.sync_registry_revisions()
            .map_err(|_| E::DurabilityPending)?;
        let recovery = if self.magic.recovery_revision(binding.actor).is_some() {
            Some(
                self.magic
                    .read_recovery_snapshot(binding.actor, self.tick as f64 / 30.0)
                    .map_err(|_| E::DurabilityPending)?,
            )
        } else {
            None
        };
        let skill_values = match self.combat.skills.get(&binding.actor) {
            Some(profile) => super::skill_refresh::project_skills(
                self.characters
                    .get(binding.actor)
                    .ok_or(E::CompleteStateRequired)?,
                &profile.prepared,
                self.magic.registry(binding.actor),
            )
            .map_err(|_| E::CompleteStateRequired)?,
            None => Vec::new(),
        };
        Ok(crate::PlayerReadSnapshot {
            gag: self.gag_snapshot(binding.actor),
            recall_destinations: self.capture_recall_destinations(binding),
            equipment_mana: self.equipment_mana_snapshot(binding.actor),
            item_experience,
            skill_values,
            combat_mode: self.world.combatant(binding.actor).map(|c| c.mode()),
            staff: self.staff.registration(binding.actor),
            target_selection: self.staff.selections.get(&binding.actor).copied(),
            entry_friends: self
                .social
                .directory
                .preferences(binding.actor)
                .map(|prefs| {
                    prefs
                        .friends
                        .iter()
                        .map(|id| {
                            let presence = self
                                .social
                                .directory
                                .presence(*id)
                                .ok_or(E::CompleteStateRequired)?;
                            Ok(bace_gameplay_api::social::SocialFriend {
                                character: *id,
                                name: presence.identity.name.clone(),
                                online: presence.online && !presence.appear_offline,
                            })
                        })
                        .collect::<Result<Vec<_>, E>>()
                })
                .transpose()?
                .unwrap_or_default(),
            entry_motion: self.world.source_motion_state(binding.actor).or_else(|| {
                let ui = self.characters.ui(binding.actor)?;
                let _ = ui;
                if self.characters.read_snapshot(binding).ok()?.ui()?.entered {
                    return None;
                }
                let profile = &self.avatar_locomotion.get(&binding.actor)?.profile;
                let drive = profile
                    .interpret(bace_motion::LocomotionControls::default(), 1.0)
                    .ok()?;
                Some(bace_motion::SourceMotionState {
                    style: profile.style,
                    substate: drive.forward_motion,
                    speed: drive.forward_rate,
                })
            }),
            entry_physics: self
                .world
                .actor_state(binding.actor)
                .map_err(|_| E::MissingActor)?
                .1,
            chat_age: self.social_player_age(binding.actor),
            physical_recovery: self.capture_physical_recovery(binding.actor),
            operation,
            binding,
            tick: self.tick,
            character: if let Some((op, revision)) = operation {
                self.characters
                    .read_operation_snapshot(binding, op, revision)?
            } else {
                self.characters.read_snapshot(binding)?
            },
            world: self
                .player_world_snapshot(binding.actor)
                .map_err(|_| E::MissingActor)?,
            death: self.player_deaths.states.get(&binding.actor).cloned(),
            social: self.social_preferences(binding.actor).cloned(),
            portal_links: self.portal_links(binding.actor).cloned(),
            recovery,
            enchantments: copy_registry(self.magic.registry(binding.actor))?,
            items: ids
                .iter()
                .map(|id| {
                    self.inventory
                        .item(*id)
                        .expect("captured owned identity")
                        .clone()
                })
                .collect(),
            item_enchantments: ids
                .into_iter()
                .filter_map(|id| self.magic.registry(id).map(|r| (id, r)))
                .map(|(id, r)| copy_registry(Some(r)).map(|r| (id, r.expect("selected registry"))))
                .collect::<Result<_, _>>()?,
        })
    }
}

fn copy_registry(
    registry: Option<&bace_magic::EnchantmentRegistry>,
) -> Result<Option<bace_magic::EnchantmentRegistry>, CharacterRegistrationError> {
    registry
        .map(|r| {
            bace_magic::EnchantmentRegistry::restore(
                r.capacity(),
                r.revision(),
                r.entries().to_vec(),
            )
            .map_err(|_| CharacterRegistrationError::DurabilityPending)
        })
        .transpose()
}

impl Kernel {
    pub fn peek_player_snapshot_outcome(&self) -> Option<&crate::PlayerSnapshotOutcome> {
        self.player_snapshot_outcomes.front()
    }
    pub fn take_player_snapshot_outcome(&mut self) -> Option<crate::PlayerSnapshotOutcome> {
        self.player_snapshot_outcomes.pop_front()
    }
    pub fn has_player_snapshot_work(&self) -> bool {
        !self.player_snapshot_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::PlayerSnapshot(_)))
    }
    pub(super) fn handle_player_snapshot(&mut self, request: crate::PlayerSnapshotRequest) {
        let result = if request.correlation == 0 {
            Err(CharacterRegistrationError::OwnershipMismatch)
        } else {
            self.capture_player_snapshot(request.binding, request.operation)
                .map(std::sync::Arc::new)
        };
        self.player_snapshot_outcomes
            .push_back(crate::PlayerSnapshotOutcome {
                correlation: request.correlation,
                result,
            });
    }
}
#[cfg(test)]
#[path = "player_snapshot_tests.rs"]
mod tests;
