use super::*;
use crate::region_activation::{RegionActivationRequest, VerifiedRegionAssets};
use bace_persistence::InventoryLoadLimits;

impl crate::region_service::RegionService {
    /// Reopen the actual SQL source head, accepted historical pack, and world
    /// forest through the cold preparation path. This read-only fixture checks
    /// that the pinned NPC remains the one actor and stock is not generic loot.
    pub(crate) async fn prove_static_shop_cold_restore(
        &self,
        shop: EntityId,
        cell: u32,
    ) -> Result<usize, String> {
        let landblock = (cell >> 16) as u16;
        let active = self
            .active
            .get(&landblock)
            .ok_or("cold Shop proof requires active source region")?;
        let head = self
            .store
            .npc_source_head(shop.0)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("cold Shop proof source head missing")?;
        if !head.completed {
            return Err("cold Shop proof source head incomplete".into());
        }
        let mut source = crate::npc_region::request(&self.store, head).await?;
        let request = RegionActivationRequest {
            token: active.fence.token,
            activation_epoch: active.fence.activation_epoch,
            landblock,
            generation: self.config.generation.clone(),
            creature_policy: Some(self.config.creature_policy),
            treasure_assets: Some(self.config.treasure_assets.clone()),
            content_hash: self.config.content_hash,
            aetheria_drop_rate: self.config.aetheria_drop_rate,
        };
        let mut assets = VerifiedRegionAssets::open(&self.config.assets)?;
        let mut prepared = assets.prepare(&request)?;
        let directory = self
            .npc_directory
            .as_ref()
            .ok_or("cold Shop proof source directory missing")?;
        let original_checkpoint = source.source.head.checkpoint.clone();
        let mut forged =
            bace_storage_codec::NpcWorkflowSaveV3::decode_or_migrate(&original_checkpoint)
                .map_err(|e| e.to_string())?;
        let origin = forged
            .inventory
            .as_mut()
            .and_then(|inventory| inventory.origin.as_mut())
            .ok_or("cold Shop proof self-origin missing")?;
        if origin.generator != shop.0 {
            return Err("cold Shop proof expected an authored self-origin".into());
        }
        origin.generator = shop
            .0
            .checked_add(1)
            .ok_or("cold Shop proof generator bound")?;
        source.source.head.checkpoint = forged.encode().map_err(|e| e.to_string())?;
        let rejected = assets.prepare_pinned_npc_region(&prepared, &request, &source, directory);
        source.source.head.checkpoint = original_checkpoint;
        if !matches!(rejected, Err(ref error) if error.contains("exact parent membership admission"))
        {
            return Err("cold Shop proof accepted a forged parent generator".into());
        }
        let pin = assets.prepare_pinned_npc_region(&prepared, &request, &source, directory)?;
        crate::npc_region::install(&mut prepared, pin)?;
        let snapshots = self
            .store
            .load_world_item_tree(
                cell,
                InventoryLoadLimits {
                    max_items: 4096,
                    max_depth: 64,
                    max_total_bytes: 64 * 1024 * 1024,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        let excluded = static_shop_forest_ids(&snapshots, &prepared)?;
        if !excluded.contains(&shop.0) {
            return Err("cold Shop proof did not identify the durable root".into());
        }
        let spells = assets.prepare_world_spell_table()?;
        let (items, sources) = prepare(
            &snapshots,
            &prepared,
            &mut assets,
            &spells,
            WorldItemRestoreClock {
                epoch: self.config.world_epoch,
                unix_seconds: 1_800_000_000,
                tick: 30,
            },
        )?;
        if items
            .roots
            .iter()
            .any(|root| excluded.contains(&root.entity.0))
            || items.items.iter().any(|item| excluded.contains(&item.id.0))
            || sources.keys().any(|id| excluded.contains(id))
        {
            return Err("cold Shop was duplicated as generic world loot".into());
        }
        Ok(excluded.len())
    }
}
use bace_content::{Position, Property, WeenieV1};
use bace_persistence::StoredAggregate;
use bace_storage_codec::{
    EntitySaveV1, FrozenCreatureConstructionV1, FrozenGeneratorConstructionOriginV1,
};
fn saved(weenie_type: u32) -> ItemSaveV4 {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "restore_guard".into(),
        weenie_type,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.strings.push(Property {
        id: 16,
        value: "preserved metadata".into(),
    });
    ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: EntitySaveV1 {
            object_id: 123,
            template_revision: 8,
            mutation_revision: 9,
            state,
        },
        placement: ItemPlacementV2::World(Position {
            obj_cell_id: 0x12340001,
            position_x: 1.,
            position_y: 2.,
            position_z: 3.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        }),
    })
    .unwrap()
}
fn row(saved: &ItemSaveV4, legacy: bool) -> LocatedSnapshot {
    LocatedSnapshot {
        aggregate: StoredAggregate {
            object_id: 123,
            persisted_version: 4,
            bytes: if legacy {
                saved.previous.encode().unwrap()
            } else {
                saved.encode().unwrap()
            },
        },
        placement: crate::game_inventory::durable(&saved.placement),
        depth: 0,
    }
}
#[test]
fn ordinary_item_keeps_exact_source_and_pose() {
    let saved = saved(1);
    let row = row(&saved, false);
    let before = row.clone();
    assert_eq!(decode_for_restore(&row).unwrap().item, saved);
    assert_eq!(row.aggregate.bytes, before.aggregate.bytes);
    assert_eq!(row.placement, before.placement);
}
#[test]
fn v5_origin_survives_world_cold_decode_without_inferring_legacy_origin() {
    let saved = saved(1);
    let v5 = ItemSaveV5 {
        previous: saved.clone(),
        source_destination: Some(9),
    };
    let snapshot = LocatedSnapshot {
        aggregate: StoredAggregate {
            object_id: 123,
            persisted_version: 4,
            bytes: v5.encode().unwrap(),
        },
        placement: crate::game_inventory::durable(&saved.placement),
        depth: 0,
    };
    let decoded = decode_for_restore(&snapshot).unwrap();
    assert_eq!(decoded.item, saved);
    assert_eq!(decoded.source_destination, Some(9));
    assert_eq!(snapshot.aggregate.bytes, v5.encode().unwrap());
    assert_eq!(
        decode_for_restore(&row(&saved, false))
            .unwrap()
            .source_destination,
        None
    );
}
#[test]
fn nested_constructed_source_parent_is_a_saved_ancestor_of_its_machine() {
    let mut child = saved(10);
    child.previous.previous.placement = ItemPlacementV2::Contained {
        container: 200,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    child
        .entity
        .state
        .properties
        .instance_ids
        .push(Property { id: 6, value: 200 });
    let mut parent = saved(21);
    parent.previous.previous.entity.object_id = 200;
    parent.previous.previous.placement = ItemPlacementV2::Contained {
        container: 1,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    let saved = BTreeMap::from([(EntityId(200), parent)]);
    assert!(constructed_source_parent_valid(&child, &saved, 7));
    assert!(!constructed_source_parent_valid(
        &child,
        &BTreeMap::new(),
        7
    ));
    child.entity.state.properties.instance_ids[0].value = 999;
    assert!(!constructed_source_parent_valid(&child, &saved, 7));
}
#[test]
fn companion_and_legacy_creature_are_held_without_generic_admission_or_lost_bytes() {
    let mut saved = saved(10);
    let legacy = row(&saved, true);
    saved.construction = Some(FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 7,
            incarnation: 1,
            content_revision: 2,
            profile: 0,
            occurrence: 0,
            random_identity: [3; 16],
            random_key_version: 1,
        },
        equipment_order: vec![],
        death_roster: vec![],
    });
    for row in [legacy, row(&saved, false)] {
        let original = row.clone();
        assert!(decode_for_restore(&row).is_err());
        assert_eq!(row.aggregate.bytes, original.aggregate.bytes);
        assert_eq!(row.placement, original.placement);
        assert_eq!(
            decode(&row).unwrap().item.entity.state.weenie_type,
            10,
            "checked snapshots remain readable for reconciliation"
        );
    }
}
#[test]
fn contained_v4_creature_reaches_constructed_restore_with_exact_source_bytes() {
    let mut value = saved(10);
    value.previous.previous.placement = ItemPlacementV2::Contained {
        container: 1,
        slot: 2,
        pack_slot: false,
        equipped: 0,
    };
    value
        .entity
        .state
        .properties
        .instance_ids
        .push(Property { id: 6, value: 7 });
    value.construction = Some(FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 7,
            incarnation: 1,
            content_revision: 2,
            profile: 0,
            occurrence: 1,
            random_identity: [3; 16],
            random_key_version: 1,
        },
        equipment_order: vec![],
        death_roster: vec![],
    });
    let original = row(&value, false);
    let decoded = decode_for_restore(&original).unwrap();
    assert_eq!(decoded.item, value);
    assert_eq!(original.aggregate.bytes, value.encode().unwrap());
}
