//! Cold preparation for an inspected physical operation. Source rows remain
//! immutable, and simulation revalidates the inspection before motion admission.
use super::*;
use crate::{
    game_inventory::FrozenInventoryItem,
    region_activation::{RegionAssetManifest, VerifiedRegionAssets},
    stack_factory::prepare_split_stack,
};
use bace_content::WeenieV1;
use bace_inventory::{ItemPlace, StackSplitPreparation};
use bace_simulation::{InventoryInspection, InventoryLivePrepared};
use bace_storage_codec::{PackGeneration, PackKey, PackLookup};

pub(super) async fn prepare(
    store: bace_db_postgres::PgStore,
    generation: Arc<PackGeneration>,
    manifest: RegionAssetManifest,
    actor: WeenieV1,
    evidence: InventoryInspection,
    mut metadata: BTreeMap<u32, FrozenInventoryItem>,
    owned: std::collections::BTreeSet<u32>,
) -> Result<cold::Prepared, String> {
    let mut bytes = 0usize;
    for item in &evidence.rows {
        if let std::collections::btree_map::Entry::Vacant(entry) = metadata.entry(item.id.0) {
            let row = store
                .load(item.id.0)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("world inventory source metadata missing")?;
            bytes = bytes
                .checked_add(row.bytes.len())
                .ok_or("inventory metadata byte overflow")?;
            if bytes > 64 * 1024 * 1024 {
                return Err("inventory metadata byte capacity".into());
            }
            entry.insert(
                FrozenInventoryItem::decode(&row.bytes, None, row.persisted_version)
                    .map_err(|e| e.to_string())?,
            );
        }
        let row = &metadata[&item.id.0];
        if row.entity.object_id != item.id.0
            || row.entity.state.weenie_id != item.template
            || row.entity.mutation_revision > item.revision
        {
            return Err("inspected inventory source fence mismatch".into());
        }
    }
    let split = matches!(
        evidence.request,
        InventoryRequest::SplitToContainer { .. }
            | InventoryRequest::SplitToWorld { .. }
            | InventoryRequest::SplitToWield { .. }
    );
    let fresh_id = if split {
        let ids = store
            .allocate_dynamic_ids(1)
            .await
            .map_err(|e| e.to_string())?;
        if ids.len() != 1 {
            return Err("split identity count".into());
        }
        Some(EntityId(ids[0]))
    } else {
        None
    };
    tokio::task::spawn_blocking(move || {
        let source = &metadata
            .get(&item_id(evidence.request).0)
            .ok_or("physical source metadata missing")?
            .entity
            .state;
        if source.properties.bools.iter().any(|p| p.id == 1 && p.value)
            || crate::generator_preparation::is_creature_template(source.weenie_type)
        {
            return Err("stuck/creature inventory source".into());
        }
        let mut assets = VerifiedRegionAssets::open(&manifest)?;
        let avatar = assets.prepare_avatar_dat(&actor)?;
        let state = evidence.motion.unwrap_or(bace_motion::SourceMotionState {
            style: avatar.motions.default_style,
            substate: *avatar
                .motions
                .style_defaults
                .get(&avatar.motions.default_style)
                .ok_or("avatar default motion missing")?,
            speed: 1.,
        });
        let scale = actor
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1., |p| p.value as f32);
        let mut motions = BTreeMap::new();
        for motion in [0x40000018, 0x40000136, 0x40000137, 0x40000138, 0x40000139] {
            motions.insert(
                motion,
                crate::world_admission::prepare_motion_chain(
                    &avatar.motions,
                    &avatar.animations,
                    crate::world_admission::MotionChainRequest {
                        style: state.style,
                        current_motion: state.substate,
                        current_speed: state.speed,
                        action: motion,
                        action_speed: 1.,
                        scale,
                        modifiers: &[],
                    },
                )?,
            );
        }
        let mut authority = cold::authority(evidence.context.actor);
        authority.new_item = fresh_id;
        let mut request = InventoryPreparedRequest {
            context: evidence.context,
            request: evidence.request,
            authority,
            split: None,
            drop: None,
        };
        let mut fresh = None;
        if let Some(id) = fresh_id {
            let PackLookup::Record(record) = generation
                .lookup(PackKey {
                    namespace: 1,
                    id: u64::from(source.weenie_id),
                })
                .map_err(|e| e.to_string())?
            else {
                return Err("fresh accepted split template missing".into());
            };
            let template = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
            let (amount, place) = match evidence.request {
                InventoryRequest::SplitToWorld { amount, .. } => (amount, ItemPlace::World),
                InventoryRequest::SplitToContainer {
                    amount,
                    container,
                    placement,
                    ..
                } => (
                    amount,
                    ItemPlace::Contained {
                        container,
                        slot: u32::try_from(placement).map_err(|_| "negative split placement")?,
                        equipped: 0,
                    },
                ),
                _ => return Err("wield split requires prepared equipment transition".into()),
            };
            let prepared = prepare_split_stack(
                &template,
                generation.revision(),
                id,
                u32::try_from(amount).map_err(|_| "negative split amount")?,
                place,
                false,
            )?;
            request.split = Some(StackSplitPreparation {
                fresh: prepared.item.clone(),
                source_stackable: bace_loot::is_stackable(source.weenie_type),
                source_stuck: false,
                source_vendor: false,
                destination_corpse: evidence.target_corpse
                    && matches!(evidence.request, InventoryRequest::SplitToContainer { .. }),
            });
            fresh = Some(prepared);
        }
        let drop_shape = if matches!(
            evidence.request,
            InventoryRequest::Drop { .. } | InventoryRequest::SplitToWorld { .. }
        ) {
            Some(
                assets
                    .prepare_template(fresh.as_ref().map_or(source, |f| &f.frozen.entity.state))?
                    .shape,
            )
        } else {
            None
        };
        let target = evidence.target.and_then(|id| metadata.get(&id.0));
        let use_radius = target
            .and_then(|t| t.entity.state.properties.floats.iter().find(|p| p.id == 54))
            .map_or(0.6, |p| p.value as f32);
        let use_radius = if state.style == 0x80000049 {
            f32::max(0., use_radius - 0.2)
        } else {
            use_radius
        };
        let sources: Vec<_> = metadata
            .values()
            .map(|r| &r.entity.state)
            .chain(fresh.as_ref().map(|f| &f.frozen.entity.state))
            .collect();
        let appearance = assets.prepare_entry_appearance(&sources)?;
        let command = InventoryCommandKind::ProposeLive(Box::new(InventoryLivePrepared {
            evidence,
            request,
            motions,
            drop_shape,
            use_radius,
        }));
        Ok(cold::Prepared {
            request: Some(command),
            fresh,
            appearance: Some(appearance),
            equipment_dat: None,
            external: metadata
                .into_iter()
                .filter_map(|(id, row)| (!owned.contains(&id)).then_some(row))
                .collect(),
        })
    })
    .await
    .map_err(|e| format!("inventory physical cold worker: {e}"))?
}
