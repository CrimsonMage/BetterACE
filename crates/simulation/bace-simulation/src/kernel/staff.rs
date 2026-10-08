//! Staff handlers execute on the existing world owner after privilege checks.
use super::*;
use bace_gameplay_api::staff::{
    MapTeleportRequest, PreparedMapTeleport, StaffDestination, StaffError, StaffEvent,
    StaffRegistration,
};
impl Kernel {
    pub fn has_staff_state(&self) -> bool {
        self.staff.has_state() || !self.social_gags.pending.is_empty()
    }
    pub fn register_staff(&mut self, registration: StaffRegistration) -> Result<(), StaffError> {
        if self.characters.get(registration.binding.actor).is_none() {
            return Err(StaffError::NotBound);
        }
        self.staff.register(registration)
    }
    pub fn refresh_staff(&mut self, registration: StaffRegistration) -> Result<(), StaffError> {
        self.staff.refresh(registration)
    }
    pub fn remove_staff(&mut self, binding: CharacterBinding) -> Result<(), StaffError> {
        if self.world.health_observation_pending_for(binding.actor) {
            return Err(StaffError::Busy);
        }
        self.staff.remove(binding)?;
        self.world.clear_health_subscription(binding.actor);
        Ok(())
    }
    pub fn peek_staff_event(&self) -> Option<&StaffEvent> {
        self.staff.peek()
    }
    pub fn take_staff_event(&mut self) -> Option<StaffEvent> {
        self.staff.take()
    }
    pub(super) fn authorize_staff(
        &mut self,
        context: ActionContext,
        minimum: u8,
        sudo: bool,
    ) -> Result<(), StaffError> {
        if !self.staff.authorize(context)?.allows(minimum, sudo) {
            return Err(StaffError::NotAuthorized);
        }
        if self.characters.reserved(context.actor)
            || self.inventory.reserved(context.actor)
            || self.npcs.reserved(context.actor)
            || self.housing.reserved(context.actor)
            || self.portals.reserved(context.actor)
        {
            return Err(StaffError::Busy);
        }
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|e| match e {
                ProgressionActionRejection::StaleSequence => StaffError::Stale,
                _ => StaffError::NotBound,
            })?;
        Ok(())
    }
    pub fn staff_map_teleport(
        &mut self,
        context: ActionContext,
        request: MapTeleportRequest,
        prepared: PreparedMapTeleport,
    ) -> Result<(), StaffError> {
        if !self.staff.authorize(context)?.map_teleport() {
            return Err(StaffError::NotAuthorized);
        }
        if request.cell != prepared.requested_cell
            || request.origin[..2] != prepared.requested_xy
            || request
                .origin
                .iter()
                .chain(&request.rotation)
                .any(|v| !v.is_finite())
        {
            return Err(StaffError::Invalid);
        }
        if prepared.entirely_water {
            return Err(StaffError::Water);
        }
        self.authorize_staff(context, 0, false)?;
        self.apply_staff_teleport(
            context,
            context.actor,
            prepared.destination,
            prepared.expected_epoch,
        )
    }
    /// The adapter resolves a command's named target and destination off-thread;
    /// caller permission is rechecked here. Target geometry is still authoritative.
    pub fn staff_teleport(
        &mut self,
        context: ActionContext,
        target: EntityId,
        destination: StaffDestination,
        expected_epoch: u16,
        kind: bace_gameplay_api::staff::StaffTeleportKind,
        sudo: bool,
    ) -> Result<(), StaffError> {
        let minimum = if matches!(kind, bace_gameplay_api::staff::StaffTeleportKind::Location) {
            4
        } else {
            2
        };
        self.authorize_staff(context, minimum, sudo)?;
        self.apply_staff_teleport(context, target, destination, expected_epoch)
    }
    /// Pinned AdminCommands.HandleRegen takes the first health, mana, or
    /// current-appraisal selection in the issuer's landblock, ignores players
    /// and non-generators, then resets and immediately regenerates the source.
    pub fn staff_regenerate(
        &mut self,
        context: ActionContext,
        target: EntityId,
        sudo: bool,
    ) -> Result<(), StaffError> {
        self.authorize_staff(context, 3, sudo)?;
        if target.0 == 0 || (0x5000_0001..=0x5FFF_FFFF).contains(&target.0) {
            return Ok(());
        }
        let Some(identity) = self
            .generators
            .machines
            .get(&target)
            .map(|machine| machine.definition().identity)
        else {
            return Ok(());
        };
        let issuer_cell = self
            .world
            .actor_state(context.actor)
            .map_err(|_| StaffError::NotBound)?
            .0;
        let Ok((target_cell, _)) = self.world.actor_state(target) else {
            return Ok(());
        };
        if issuer_cell.0 >> 16 != target_cell.0 >> 16 {
            return Ok(());
        }
        self.generator_control(identity, crate::GeneratorControl::Regenerate)
            .map_err(|error| match error {
                crate::GeneratorServiceError::Busy => StaffError::Busy,
                crate::GeneratorServiceError::Capacity => StaffError::Capacity,
                crate::GeneratorServiceError::Stale | crate::GeneratorServiceError::Missing => {
                    StaffError::Stale
                }
                _ => StaffError::Invalid,
            })
    }
    fn apply_staff_teleport(
        &mut self,
        context: ActionContext,
        target: EntityId,
        destination: StaffDestination,
        expected_epoch: u16,
    ) -> Result<(), StaffError> {
        if destination.cell == 0
            || destination
                .origin
                .iter()
                .chain(&destination.rotation)
                .any(|v| !v.is_finite())
        {
            return Err(StaffError::Invalid);
        }
        if self.inventory.reserved(target)
            || self.characters.reserved(target)
            || self.npcs.reserved(target)
            || self.housing.reserved(target)
            || self.portals.reserved(target)
            || self.magic.busy(target)
        {
            return Err(StaffError::Busy);
        }
        let (cell, state) = self
            .world
            .actor_state(target)
            .map_err(|_| StaffError::MissingTarget)?;
        let before = StaffDestination {
            cell: cell.0,
            origin: [state.position().x, state.position().y, state.position().z],
            rotation: [
                (state.heading_radians() * 0.5).cos(),
                0.,
                0.,
                (state.heading_radians() * 0.5).sin(),
            ],
        };
        let [w, x, y, z] = destination.rotation;
        let norm = w * w + x * x + y * y + z * z;
        if !norm.is_finite() || !(0.999..=1.001).contains(&norm) {
            return Err(StaffError::Invalid);
        }
        let teleport = bace_world::WorldTeleport {
            actor: target,
            expected_epoch,
            destination: CellId(destination.cell),
            position: Vec3::new(
                destination.origin[0],
                destination.origin[1],
                destination.origin[2],
            ),
            heading: (2. * (w * z + x * y)).atan2(1. - 2. * (y * y + z * z)),
        };
        self.world
            .validate_teleport_batch(&[teleport])
            .map_err(|_| StaffError::MissingGeometry)?;
        self.world
            .teleport_batch(&[teleport])
            .map_err(|_| StaffError::MissingGeometry)?;
        let accepted = self
            .world
            .actor_state(target)
            .map_err(|_| StaffError::MissingTarget)?
            .1;
        let velocity = accepted.velocity();
        self.staff.push(StaffEvent::Teleported {
            context,
            target,
            before,
            after: destination,
            epoch: accepted.epoch(),
            velocity: [velocity.x, velocity.y, velocity.z],
            grounded: accepted.grounded(),
        });
        Ok(())
    }
    /// Pinned AdminCommands.HandleHeal restores all three vitals of a player,
    /// and sends private Broadcast chat for absent or selected nonplayer targets.
    pub fn staff_heal(
        &mut self,
        context: ActionContext,
        target: EntityId,
        target_name: Option<&str>,
        sudo: bool,
    ) -> Result<(), StaffError> {
        self.authorize_staff(context, 3, sudo)?;
        if !self.staff.room(1) {
            return Err(StaffError::Capacity);
        }
        if !self.target_in_landblock_scope(context.actor, target)? {
            self.staff.push(StaffEvent::Inspection {
                context,
                target,
                lines: vec!["Unable to locate what you have selected.".into()],
            });
            return Ok(());
        }
        if self
            .world
            .combatant(target)
            .is_none_or(|c| !c.profile().player)
        {
            let line = match target_name {
                Some(name) if !name.is_empty() && name.len() <= 2048 => {
                    format!("You cannot heal {name} because it is not a player.")
                }
                _ => "Unable to locate what you have selected.".into(),
            };
            self.staff.push(StaffEvent::Inspection {
                context,
                target,
                lines: vec![line],
            });
            return Ok(());
        }
        if self.characters.reserved(target)
            || self.inventory.reserved(target)
            || self.npcs.reserved(target)
        {
            return Err(StaffError::Busy);
        }
        let mut mutations = Vec::with_capacity(3);
        let mut vitals = Vec::with_capacity(3);
        for (id, vital) in [
            (2, bace_entity::EntityVital::Health),
            (4, bace_entity::EntityVital::Stamina),
            (6, bace_entity::EntityVital::Mana),
        ] {
            if let Ok(pool) = self.world.vital(target, vital) {
                mutations.push(bace_entity::VitalMutation {
                    actor: target,
                    vital,
                    before: pool.current,
                    after: pool.maximum,
                });
                vitals.push((id, pool.maximum));
            }
        }
        if mutations.len() != 3 {
            return Err(StaffError::MissingTarget);
        }
        self.world
            .apply_vital_batch(&mutations, None)
            .map_err(|_| StaffError::Busy)?;
        self.staff.push(StaffEvent::Healed {
            context,
            target,
            vitals,
        });
        Ok(())
    }
}

#[cfg(test)]
mod regen_tests {
    use super::*;
    use bace_character::{CharacterProgression, ProgressionTables, RankTable};
    use bace_gameplay_api::{
        SessionId,
        staff::{StaffAction, StaffCommand, StaffPrivileges},
    };
    use bace_types::AccountId;
    use std::sync::Arc;

    #[test]
    fn ace_regen_no_selection_and_player_selection_do_not_mutate_generator() {
        let mut kernel = crate::synthetic_scenario(1, 1).unwrap();
        let rank = RankTable::new(&[0, 10]).unwrap();
        let context = ActionContext {
            actor: EntityId(1),
            account: AccountId(7),
            session: SessionId(9),
            sequence: 1,
        };
        let binding = CharacterBinding {
            actor: context.actor,
            account: context.account,
            session: context.session,
        };
        kernel
            .register_character(
                binding,
                CharacterProgression::new(
                    &[],
                    Arc::new(ProgressionTables {
                        attributes: rank.clone(),
                        vitals: rank.clone(),
                        trained_skills: rank.clone(),
                        specialized_skills: rank,
                    }),
                    0,
                    0,
                )
                .unwrap(),
            )
            .unwrap();
        kernel
            .register_staff(StaffRegistration {
                binding,
                privileges: StaffPrivileges::default(),
            })
            .unwrap();
        kernel
            .apply_staff_command(StaffCommand {
                token: 1,
                action: StaffAction::Regenerate {
                    context,
                    target: EntityId(0),
                    sudo: false,
                },
            })
            .unwrap();
        assert!(matches!(
            kernel.take_staff_event(),
            Some(StaffEvent::Outcome {
                result: Err(StaffError::NotAuthorized),
                ..
            })
        ));
        kernel
            .refresh_staff(StaffRegistration {
                binding,
                privileges: StaffPrivileges {
                    account_access: 3,
                    envoy: true,
                    ..Default::default()
                },
            })
            .unwrap();
        for (token, target) in [(2, EntityId(0)), (3, EntityId(1))] {
            kernel
                .apply_staff_command(StaffCommand {
                    token,
                    action: StaffAction::Regenerate {
                        context: ActionContext {
                            sequence: token as u32 - 1,
                            ..context
                        },
                        target,
                        sudo: false,
                    },
                })
                .unwrap();
            assert!(matches!(
                kernel.take_staff_event(),
                Some(StaffEvent::Outcome {
                    token: seen,
                    result: Ok(()),
                    ..
                }) if seen == token
            ));
            assert!(kernel.take_generator_lifecycle().is_none());
        }
    }
}
