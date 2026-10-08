//! Source post-inventory corpse location and retained-item notices. The
//! accepted death pose is copied into both the death state and corpse save.
use bace_interactions::{PlayerDeathKind, PortalPosition};
use bace_persistence::SaveSnapshot;
use bace_simulation::PlayerDeathTicket;
use bace_storage_codec::{CorpseSaveV5, ItemPlacementV2, PlayerSaveV6};
use bace_wire::WirePosition;

pub(super) struct CorpseLocationOutput {
    position: Option<WirePosition>,
    location_chat: Option<String>,
    miser_chat: Option<String>,
    retained_chat: bool,
}

impl CorpseLocationOutput {
    pub(super) fn prepare(
        ticket: &PlayerDeathTicket,
        committed: &[SaveSnapshot],
    ) -> Result<Self, String> {
        if ticket.no_corpse.is_some() || ticket.olthoi.is_some() || ticket.corpse.0 == 0 {
            return Err("ordinary corpse location owner mismatch".into());
        }
        let corpse_row = committed
            .iter()
            .find(|row| row.object_id == ticket.corpse.0)
            .ok_or("death corpse location receipt absent")?;
        let corpse = CorpseSaveV5::decode(&corpse_row.bytes).map_err(|e| e.to_string())?;
        if corpse_row.expected_version != 0
            || corpse_row.mutation_revision != corpse.corpse.entity.mutation_revision
            || corpse.corpse.entity.object_id != ticket.corpse.0
            || corpse.operation != Some(ticket.operation)
        {
            return Err("death corpse location receipt identity mismatch".into());
        }
        let ItemPlacementV2::World(saved_pose) = &corpse.placement else {
            return Err("death corpse location placement absent".into());
        };
        let player_row = committed
            .iter()
            .find(|row| row.object_id == ticket.actor.0)
            .ok_or("death player location receipt absent")?;
        let player = PlayerSaveV6::decode(&player_row.bytes).map_err(|e| e.to_string())?;
        if player_row.mutation_revision != ticket.after_revision
            || player.player.entity.object_id != ticket.actor.0
            || player.player.entity.mutation_revision != ticket.after_revision
            || crate::player_death_state::restore_player_death_state(&player)
                .map_err(|e| e.to_string())?
                != ticket.after
        {
            return Err("death player location receipt identity mismatch".into());
        }
        let outdoor = saved_pose.obj_cell_id & 0xffff < 0x100;
        let position = if outdoor {
            let frozen = ticket
                .after
                .last_outside_death
                .ok_or("outdoor death position missing")?;
            if !same_pose(frozen, saved_pose) {
                return Err("outdoor death position/committed corpse pose mismatch".into());
            }
            Some(WirePosition {
                cell: frozen.cell,
                origin: frozen.origin,
                rotation: frozen.rotation,
            })
        } else {
            if ticket.after.last_outside_death != ticket.before.last_outside_death {
                return Err("indoor death changed LastOutsideDeath".into());
            }
            None
        };
        // ACE tests the full CalculateDeathItems result. Destroyed pyreals
        // and bonded items remain in that list though none enter the corpse.
        let source_dropped = !ticket.corpse_items.is_empty()
            || ticket.inventory_transcript.as_ref().is_some_and(|t| {
                t.coin_amount != 0 || !t.drops.is_empty() || !t.destroyed.is_empty()
            });
        let location_chat = if outdoor && source_dropped {
            Some(format!(
                "Your corpse is located at ({}).",
                map_coord_string(
                    saved_pose.obj_cell_id,
                    [saved_pose.position_x, saved_pose.position_y]
                )?
            ))
        } else {
            None
        };
        let augmentation = player
            .player
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|property| property.id == 231)
            .map_or(0, |property| property.value);
        if !(0..=100).contains(&augmentation) {
            return Err("death item-loss augmentation out of range".into());
        }
        let miser_chat = if ticket.kind == PlayerDeathKind::Ordinary && augmentation > 0 {
            Some(format!(
                "Your augmentation has reduced the number of items you can lose by {}!",
                augmentation * 5
            ))
        } else {
            None
        };
        Ok(Self {
            position,
            location_chat,
            miser_chat,
            retained_chat: !source_dropped && ticket.kind != PlayerDeathKind::Pkl,
        })
    }

    pub(super) fn append<'a>(&'a self, steps: &mut Vec<bace_replication::InventoryProjection<'a>>) {
        if let Some(position) = self.position {
            steps.push(bace_replication::InventoryProjection::PrivatePosition {
                position_type: 14,
                position,
            });
        }
        if let Some(text) = &self.location_chat {
            steps.push(bace_replication::InventoryProjection::System { text, chat_type: 0 });
        }
        if let Some(text) = &self.miser_chat {
            steps.push(bace_replication::InventoryProjection::System { text, chat_type: 0 });
        }
        if self.retained_chat {
            steps.push(bace_replication::InventoryProjection::System {
                text: "You have retained all your items. You do not need to recover your corpse!",
                chat_type: 0,
            });
        }
    }
}

fn same_pose(source: PortalPosition, saved: &bace_content::Position) -> bool {
    source.cell == saved.obj_cell_id
        && source.origin == [saved.position_x, saved.position_y, saved.position_z]
        && source.rotation
            == [
                saved.rotation_w,
                saved.rotation_x,
                saved.rotation_y,
                saved.rotation_z,
            ]
}

// ACE PositionExtensions.GetMapCoordStr and Position.ToGlobal. The source
// subtracts 0.05 after selecting the N/S and E/W suffix.
fn map_coord_string(cell: u32, local: [f32; 2]) -> Result<String, String> {
    if cell & 0xffff >= 0x100 || local.iter().any(|v| !v.is_finite()) {
        return Err("corpse map coordinate is not outdoor/finite".into());
    }
    let x = (((cell >> 24) & 0xff) as f32 * 192. + local[0]) / 240. - 102.;
    let y = (((cell >> 16) & 0xff) as f32 * 192. + local[1]) / 240. - 102.;
    if !x.is_finite() || !y.is_finite() {
        return Err("corpse map coordinate overflow".into());
    }
    Ok(format!(
        "{:.1}{}, {:.1}{}",
        y.abs() - 0.05,
        if y >= 0. { "N" } else { "S" },
        x.abs() - 0.05,
        if x >= 0. { "E" } else { "W" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Position, WeenieV1};
    use bace_storage_codec::{
        CorpseSaveV1, CorpseSaveV2, CorpseSaveV3, CorpseSaveV4, EntitySaveV1, PlayerSaveV1,
    };
    use bace_types::{CellId, EntityId};

    fn fixture(
        outdoor: bool,
        drops: bool,
        kind: PlayerDeathKind,
    ) -> (PlayerDeathTicket, Vec<SaveSnapshot>) {
        let actor = EntityId(0x5000_0001);
        let corpse_id = EntityId(0x8000_0001);
        let position = Position {
            obj_cell_id: if outdoor { 0x8080_0001 } else { 0x8080_0100 },
            position_x: 73.,
            position_y: 121.,
            position_z: 1.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        };
        let source = |id, weenie_type, revision| EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: revision,
            state: WeenieV1 {
                schema_version: 1,
                weenie_id: 11,
                class_name: "death location fixture".into(),
                weenie_type,
                last_modified: None,
                properties: Default::default(),
            },
        };
        let before = bace_simulation::PlayerDeathState::default();
        let mut after = before.clone();
        after.num_deaths = 1;
        if outdoor {
            after.last_outside_death = Some(PortalPosition {
                cell: position.obj_cell_id,
                origin: [
                    position.position_x,
                    position.position_y,
                    position.position_z,
                ],
                rotation: [1., 0., 0., 0.],
            });
        }
        let mut player = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
            entity: source(actor.0, 10, 2),
            account_id: 1,
            name: "Death Fixture".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap();
        crate::player_death_state::freeze_player_death_state(&mut player, &after).unwrap();
        let corpse = CorpseSaveV5::migrate_v4(CorpseSaveV4 {
            previous: CorpseSaveV3 {
                previous: CorpseSaveV2 {
                    corpse: CorpseSaveV1 {
                        entity: source(corpse_id.0, 14, 1),
                        owner: Some(actor.0),
                        death_operation: "death-location-fixture".into(),
                        expires_at: 3600,
                    },
                    placement: ItemPlacementV2::World(position),
                },
                enchantments: vec![],
            },
            source: Some(actor.0),
            operation: Some(8),
        })
        .unwrap();
        let ticket = PlayerDeathTicket {
            operation: 8,
            actor,
            killer: None,
            kind,
            olthoi: None,
            before_revision: 1,
            after_revision: 2,
            before,
            after,
            purge_bad: false,
            inventory: bace_simulation::InventoryTicket {
                operation: 9,
                actor,
                proposal: bace_inventory::InventoryProposal {
                    changes: vec![],
                    participants: vec![],
                    actor_burden: 0,
                    requires_pickup_motion: false,
                },
            },
            inventory_transcript: None,
            corpse: corpse_id,
            no_corpse: None,
            corpse_items: if drops {
                vec![EntityId(0x8000_0002)]
            } else {
                vec![]
            },
            corpse_decay_seconds: 3600,
            registry_before_revision: 1,
            registry_after_revision: 2,
            before_enchantments: vec![],
            enchantments: vec![],
            destination: bace_world::WorldTeleport {
                actor,
                expected_epoch: 1,
                destination: CellId(0x8080_0001),
                position: bace_geometry::Vec3::new(1., 2., 3.),
                heading: 0.,
            },
            vitals: vec![],
            post_death_maxima: [100; 3],
            animation_ticks: 30,
        };
        let rows = vec![
            SaveSnapshot {
                object_id: actor.0,
                mutation_revision: 2,
                expected_version: 1,
                bytes: player.encode().unwrap(),
            },
            SaveSnapshot {
                object_id: corpse_id.0,
                mutation_revision: 1,
                expected_version: 0,
                bytes: corpse.encode().unwrap(),
            },
        ];
        (ticket, rows)
    }

    #[test]
    fn pinned_map_coordinate_order_and_outdoor_gate() {
        // ACE PositionExtensions.ToGlobal / GetMapCoordStr at the pinned SHA.
        assert_eq!(
            map_coord_string(0x80800001, [73., 121.]).unwrap(),
            "0.9N, 0.7E"
        );
        assert_eq!(
            map_coord_string(0x7e7e0001, [71., 119.]).unwrap(),
            "0.7S, 0.9W"
        );
        assert!(map_coord_string(0x80800100, [73., 121.]).is_err());
    }

    #[test]
    fn committed_outdoor_location_precedes_chat_and_zero_drop_has_no_location_chat() {
        let (ticket, rows) = fixture(true, true, PlayerDeathKind::Ordinary);
        let output = CorpseLocationOutput::prepare(&ticket, &rows).unwrap();
        let mut steps = vec![];
        output.append(&mut steps);
        assert!(matches!(
            steps[0],
            bace_replication::InventoryProjection::PrivatePosition {
                position_type: 14,
                ..
            }
        ));
        assert!(matches!(
            steps[1],
            bace_replication::InventoryProjection::System {
                text: "Your corpse is located at (0.9N, 0.7E).",
                ..
            }
        ));
        let (ticket, rows) = fixture(true, false, PlayerDeathKind::Ordinary);
        let output = CorpseLocationOutput::prepare(&ticket, &rows).unwrap();
        let mut steps = vec![];
        output.append(&mut steps);
        assert_eq!(steps.len(), 2);
        assert!(matches!(
            steps[0],
            bace_replication::InventoryProjection::PrivatePosition { .. }
        ));
        assert!(matches!(
            steps[1],
            bace_replication::InventoryProjection::System {
                text: "You have retained all your items. You do not need to recover your corpse!",
                ..
            }
        ));
        let (ticket, rows) = fixture(false, true, PlayerDeathKind::Ordinary);
        let output = CorpseLocationOutput::prepare(&ticket, &rows).unwrap();
        let mut steps = vec![];
        output.append(&mut steps);
        assert!(steps.is_empty());
        let (ticket, rows) = fixture(true, false, PlayerDeathKind::Pkl);
        let output = CorpseLocationOutput::prepare(&ticket, &rows).unwrap();
        let mut steps = vec![];
        output.append(&mut steps);
        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn stale_corpse_pose_or_player_death_checkpoint_cannot_publish_location() {
        let (ticket, mut rows) = fixture(true, true, PlayerDeathKind::Ordinary);
        let mut corpse = CorpseSaveV5::decode(&rows[1].bytes).unwrap();
        let ItemPlacementV2::World(position) = &mut corpse.placement else {
            panic!("world corpse fixture")
        };
        position.position_x += 1.;
        rows[1].bytes = corpse.encode().unwrap();
        assert!(CorpseLocationOutput::prepare(&ticket, &rows).is_err());
        let (ticket, mut rows) = fixture(true, true, PlayerDeathKind::Ordinary);
        rows[0].mutation_revision += 1;
        assert!(CorpseLocationOutput::prepare(&ticket, &rows).is_err());
    }

    #[test]
    fn destroyed_coins_still_trigger_source_location_chat_without_a_corpse_item() {
        let (mut ticket, rows) = fixture(true, false, PlayerDeathKind::Ordinary);
        ticket.inventory_transcript = Some(bace_simulation::DeathInventoryTranscript {
            drops: vec![],
            destroyed: vec![],
            coin_sources: vec![bace_simulation::DeathCoinSource {
                source: EntityId(0x8000_0002),
                before: 10,
                after: 5,
                whole: false,
                burden_after: 0,
                coin_value_after: 5,
            }],
            coin_drops: vec![],
            coin_amount: 5,
            pyreals_destroyed: true,
        });
        let output = CorpseLocationOutput::prepare(&ticket, &rows).unwrap();
        let mut steps = vec![];
        output.append(&mut steps);
        assert!(matches!(
            steps.as_slice(),
            [
                bace_replication::InventoryProjection::PrivatePosition { .. },
                bace_replication::InventoryProjection::System {
                    text: "Your corpse is located at (0.9N, 0.7E).",
                    ..
                }
            ]
        ));
    }
}
