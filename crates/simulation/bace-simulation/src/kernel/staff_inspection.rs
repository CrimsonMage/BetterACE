//! Read-only staff commands consume authoritative snapshots and emit bounded text.
use super::*;
use bace_gameplay_api::staff::{StaffError, StaffEvent, StaffInspection};
impl Kernel {
    pub fn staff_inspect(
        &mut self,
        context: ActionContext,
        target: EntityId,
        kind: StaffInspection,
        sudo: bool,
    ) -> Result<(), StaffError> {
        let minimum = match kind {
            StaffInspection::Identity => 3,
            StaffInspection::WhoAmI => 4,
            StaffInspection::SelfPosition
            | StaffInspection::Gps
            | StaffInspection::Position
            | StaffInspection::Vitals => 4,
            StaffInspection::Enchantments => 5,
        };
        self.authorize_staff(context, minimum, sudo)?;
        let mut lines = Vec::new();
        match kind {
            StaffInspection::Identity => lines.push(staff_identity_line(target)),
            StaffInspection::WhoAmI => {
                if target != context.actor {
                    return Err(StaffError::Invalid);
                }
                lines.push(staff_whoami_line(context.actor));
            }
            StaffInspection::SelfPosition => {
                if target != context.actor {
                    return Err(StaffError::Invalid);
                }
                let (cell, state) = self
                    .world
                    .actor_state(context.actor)
                    .map_err(|_| StaffError::MissingTarget)?;
                lines.extend(staff_self_position_lines(
                    cell,
                    state.position(),
                    state.heading_radians(),
                ));
            }
            StaffInspection::Gps => {
                if target != context.actor {
                    return Err(StaffError::Invalid);
                }
                let (cell, state) = self
                    .world
                    .actor_state(context.actor)
                    .map_err(|_| StaffError::MissingTarget)?;
                lines.push(staff_gps_line(
                    cell,
                    state.position(),
                    state.heading_radians(),
                ));
            }
            StaffInspection::Position => {
                let (cell, state) = self
                    .world
                    .actor_state(target)
                    .map_err(|_| StaffError::MissingTarget)?;
                lines.extend(staff_target_position_lines(
                    cell,
                    state.position(),
                    state.heading_radians(),
                ));
            }
            StaffInspection::Vitals => {
                for vital in [
                    bace_entity::EntityVital::Health,
                    bace_entity::EntityVital::Stamina,
                    bace_entity::EntityVital::Mana,
                ] {
                    if let Ok(pool) = self.world.vital(target, vital) {
                        lines.push(format!("{vital:?}: {}/{}", pool.current, pool.maximum));
                    }
                }
                if lines.is_empty() {
                    return Err(StaffError::MissingTarget);
                }
            }
            StaffInspection::Enchantments => {
                let registry = self
                    .magic
                    .registry(target)
                    .ok_or(StaffError::MissingTarget)?;
                if registry.entries().len() > 1024 {
                    return Err(StaffError::Capacity);
                }
                for entry in registry.entries() {
                    if registry
                        .top(entry.spec.category, self.tick as f64 / 30.)
                        .is_some_and(|top| {
                            top.spell == entry.spell && top.spec.layer == entry.spec.layer
                        })
                    {
                        lines.push(format!(
                            "Spell {} layer {} caster 0x{:08X} duration {} elapsed {} value {}",
                            entry.spell,
                            entry.spec.layer,
                            entry.caster,
                            entry.spec.duration,
                            -entry.start_time,
                            entry.spec.value
                        ));
                    }
                }
            }
        }
        self.staff.push(StaffEvent::Inspection {
            context,
            target,
            lines,
        });
        Ok(())
    }
}

/// ACE AdminCommands.HandleMyIID uses ObjectGuid.Full, Low (24 bits), and
/// High (the upper byte) in a Broadcast system-chat line.
fn staff_identity_line(target: EntityId) -> String {
    let full = target.0;
    format!(
        "GUID: {full}  - Low: {} - High: {} - (0x{full:X})",
        full & 0x00FF_FFFF,
        full >> 24
    )
}

/// ACE DeveloperCommands.HandleWhoAmI uses ObjectGuid.ToString (X8), whereas
/// Envoy HandleMyIID uses the unpadded X format in a different source line.
fn staff_whoami_line(actor: EntityId) -> String {
    let full = actor.0;
    format!(
        "GUID: {full} (0x{full:08X}) | ID(low): {} High:{}",
        full & 0x00FF_FFFF,
        full >> 24
    )
}

/// ACE DeveloperCommands.HandleMyLoc writes three Broadcast lines. World
/// position and heading come only from accepted authoritative physics state.
fn staff_self_position_lines(cell: CellId, origin: Vec3, heading_radians: f32) -> [String; 3] {
    let half = heading_radians * 0.5;
    let w = half.cos();
    let z = half.sin();
    [
        format!("CurrentLandblock: {:04X}", cell.0 >> 16),
        format!(
            "Location: 0x{:08X} [{:.6} {:.6} {:.6}] {:.6} {:.6} {:.6} {:.6}",
            cell.0, origin.x, origin.y, origin.z, w, 0.0, 0.0, z
        ),
        format!(
            "Physics : 0x{:08X} [{} {} {}] {} {} {} {}",
            cell.0, origin.x, origin.y, origin.z, w, 0.0, 0.0, z
        ),
    ]
}

/// ACE DeveloperCommands.HandleDebugGPS reads Player.Location's landblock,
/// float position and quaternion in X/Y/Z/W order. The accepted body supplies
/// the position and heading; this world models only yaw rotation.
fn staff_gps_line(cell: CellId, origin: Vec3, heading_radians: f32) -> String {
    let half = heading_radians * 0.5;
    let (z, w) = half.sin_cos();
    format!(
        "Position: [Cell: 0x{:04X} | Offset: {}, {}, {} | Facing: {}, {}, {}, {}]",
        cell.0 >> 16,
        origin.x,
        origin.y,
        origin.z,
        0.0f32,
        0.0f32,
        z,
        w
    )
}

/// DeveloperCommands.HandleTargetLoc emits four Broadcast lines. The target's
/// accepted world pose supplies both its persisted Location and physical frame;
/// this owner models yaw-only orientation. Unloaded/global objects require a
/// separate source-resolution route and cannot be invented here.
fn staff_target_position_lines(cell: CellId, origin: Vec3, heading_radians: f32) -> [String; 4] {
    let half = heading_radians * 0.5;
    let (z, w) = half.sin_cos();
    [
        format!("CurrentLandblock: 0x{:04X}", cell.0 >> 16),
        format!(
            "Location: 0x{:08X} [{:.6} {:.6} {:.6}] {:.6} {:.6} {:.6} {:.6}",
            cell.0, origin.x, origin.y, origin.z, w, 0.0, 0.0, z
        ),
        format!(
            "Physics : 0x{:08X} [{} {} {}] {} {} {} {}",
            cell.0, origin.x, origin.y, origin.z, w, 0.0, 0.0, z
        ),
        format!("CurCell: 0x{:08X}", cell.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_character::{CharacterProgression, ProgressionTables, RankTable};
    use bace_gameplay_api::{
        CharacterBinding, SessionId,
        staff::{StaffPrivileges, StaffRegistration},
    };
    use bace_types::AccountId;
    use std::sync::Arc;

    #[test]
    fn ace_myiid_guid_components_and_text() {
        // ACE Source/ACE.Entity/ObjectGuid.cs: Low = Full & 0xFFFFFF,
        // High = Full >> 24; AdminCommands.cs HandleMyIID formats this line.
        assert_eq!(
            staff_identity_line(EntityId(0x5000_0001)),
            "GUID: 1342177281  - Low: 1 - High: 80 - (0x50000001)"
        );
        assert_eq!(
            staff_identity_line(EntityId(0x7AAB_B123)),
            "GUID: 2058072355  - Low: 11252003 - High: 122 - (0x7AABB123)"
        );
    }

    #[test]
    fn ace_whoami_uses_padded_guid_and_distinct_source_line() {
        // DeveloperCommands.HandleWhoAmI interpolates ObjectGuid.ToString(),
        // which is X8 at the pinned ACE source.
        assert_eq!(
            staff_whoami_line(EntityId(0x5000_0001)),
            "GUID: 1342177281 (0x50000001) | ID(low): 1 High:80"
        );
        assert_eq!(
            staff_whoami_line(EntityId(1)),
            "GUID: 1 (0x00000001) | ID(low): 1 High:0"
        );
    }

    #[test]
    fn envoy_myiid_rechecks_owner_authority_and_emits_exact_line() {
        let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
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
                privileges: StaffPrivileges {
                    account_access: 3,
                    envoy: true,
                    ..Default::default()
                },
            })
            .unwrap();
        kernel
            .staff_inspect(context, context.actor, StaffInspection::Identity, false)
            .unwrap();
        let Some(StaffEvent::Inspection {
            context: seen,
            target,
            lines,
        }) = kernel.take_staff_event()
        else {
            panic!("missing canonical staff inspection")
        };
        assert_eq!((seen, target), (context, context.actor));
        assert_eq!(lines, ["GUID: 1  - Low: 1 - High: 0 - (0x1)"]);
        assert_eq!(
            kernel.staff_inspect(
                ActionContext {
                    session: SessionId(10),
                    sequence: 2,
                    ..context
                },
                context.actor,
                StaffInspection::Identity,
                false
            ),
            Err(StaffError::NotBound)
        );
        assert_eq!(
            kernel.staff_inspect(
                ActionContext {
                    sequence: 2,
                    ..context
                },
                context.actor,
                StaffInspection::WhoAmI,
                false,
            ),
            Err(StaffError::NotAuthorized)
        );
        kernel
            .refresh_staff(StaffRegistration {
                binding,
                privileges: StaffPrivileges {
                    account_access: 4,
                    developer: true,
                    ..Default::default()
                },
            })
            .unwrap();
        kernel
            .staff_inspect(
                ActionContext {
                    sequence: 2,
                    ..context
                },
                context.actor,
                StaffInspection::WhoAmI,
                false,
            )
            .unwrap();
        let Some(StaffEvent::Inspection { lines, target, .. }) = kernel.take_staff_event() else {
            panic!("whoami private line")
        };
        assert_eq!(target, context.actor);
        assert_eq!(lines, ["GUID: 1 (0x00000001) | ID(low): 1 High:0"]);
    }

    #[test]
    fn developer_myloc_emits_three_source_lines_from_accepted_world_state() {
        // ACE DeveloperCommands.HandleMyLoc and Position.ToLOCString;
        // Physics.Common.Position/AFrame supply the third line.
        let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
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
                privileges: StaffPrivileges {
                    account_access: 4,
                    developer: true,
                    ..Default::default()
                },
            })
            .unwrap();
        assert_eq!(
            kernel.staff_inspect(context, EntityId(2), StaffInspection::SelfPosition, false),
            Err(StaffError::Invalid)
        );
        let context = ActionContext {
            sequence: 2,
            ..context
        };
        kernel
            .staff_inspect(context, context.actor, StaffInspection::SelfPosition, false)
            .unwrap();
        let Some(StaffEvent::Inspection { target, lines, .. }) = kernel.take_staff_event() else {
            panic!("missing source location lines")
        };
        assert_eq!(target, context.actor);
        assert_eq!(
            lines,
            [
                "CurrentLandblock: 0001",
                "Location: 0x00010001 [-500.000000 -500.000000 0.500000] 1.000000 0.000000 0.000000 0.000000",
                "Physics : 0x00010001 [-500 -500 0.5] 1 0 0 0",
            ]
        );
    }

    #[test]
    fn developer_gps_emits_one_source_line_from_accepted_world_state() {
        // ACE DeveloperCommands.HandleDebugGPS interpolates Position's
        // landblock, float offsets and X/Y/Z/W quaternion in this order.
        let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
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
                privileges: StaffPrivileges {
                    account_access: 4,
                    developer: true,
                    ..Default::default()
                },
            })
            .unwrap();
        assert_eq!(
            kernel.staff_inspect(context, EntityId(2), StaffInspection::Gps, false),
            Err(StaffError::Invalid)
        );
        kernel
            .staff_inspect(
                ActionContext {
                    sequence: 2,
                    ..context
                },
                context.actor,
                StaffInspection::Gps,
                false,
            )
            .unwrap();
        let Some(StaffEvent::Inspection { target, lines, .. }) = kernel.take_staff_event() else {
            panic!("missing GPS response")
        };
        assert_eq!(target, context.actor);
        assert_eq!(
            lines,
            ["Position: [Cell: 0x0001 | Offset: -500, -500, 0.5 | Facing: 0, 0, 0, 1]"]
        );
    }

    #[test]
    fn developer_targetloc_selected_emits_four_source_lines() {
        // ACE DeveloperCommands.HandleTargetLoc emits these four Broadcast
        // lines in order, using Position.ToLOCString and Physics.Position.
        assert_eq!(
            staff_target_position_lines(
                CellId(0x0001_0001),
                Vec3 {
                    x: -500.0,
                    y: -500.0,
                    z: 0.5,
                },
                0.0,
            ),
            [
                "CurrentLandblock: 0x0001",
                "Location: 0x00010001 [-500.000000 -500.000000 0.500000] 1.000000 0.000000 0.000000 0.000000",
                "Physics : 0x00010001 [-500 -500 0.5] 1 0 0 0",
                "CurCell: 0x00010001",
            ]
        );
    }
}
