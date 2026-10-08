//! Freeze one retained native PVE proposal into exact item snapshots and an
//! immutable simulation admission. Asset and SQL work stays off the owner tick.
use super::*;
use bace_persistence::{DurableItemPlace, LocatedSnapshot, StoredAggregate};
use bace_storage_codec::{EntitySaveV1, PackKey, PackLookup};

pub(super) struct Prepared {
    pub operation: bace_persistence::PlacementOperation,
    pub allegiance: Option<bace_persistence::AllegianceOperation>,
    pub forest: bace_simulation::PreparedWorldRegionItems,
    pub sources: Vec<crate::region_unload_saves::RegionItemSource>,
    pub visibility: Vec<crate::visibility_assets::PreparedVisibilityObject>,
}
pub(super) struct Completion {
    pub ids: Vec<EntityId>,
    pub result: Result<Prepared, String>,
}
pub(super) struct Input {
    pub proposal: bace_simulation::DeathProposal,
    pub ids: Vec<EntityId>,
    pub generation: Arc<bace_storage_codec::PackGeneration>,
    pub region: Arc<crate::region_activation::PreparedRegionActivation>,
    pub manifest: crate::region_activation::RegionAssetManifest,
    pub epoch: u64,
    pub unix_seconds: i64,
    pub tick: u64,
    pub players: Vec<crate::death_saves::DeathPlayer>,
    pub item_baselines: BTreeMap<EntityId, Vec<crate::game_inventory::FrozenInventoryItem>>,
    pub leases: Vec<bace_persistence::CharacterLease>,
    pub social_before: Option<(
        Vec<bace_persistence::StoredAllegiance>,
        Vec<bace_persistence::StoredAllegiance>,
    )>,
}
pub(super) async fn prepare(store: bace_db_postgres::PgStore, mut input: Input) -> Completion {
    if let Some(ticket) = &input.proposal.social {
        let participants = crate::allegiance_service::AllegianceService::participants(ticket);
        match store.character_leases(&participants).await {
            Ok(leases)
                if leases
                    .iter()
                    .map(|lease| lease.character_id)
                    .collect::<Vec<_>>()
                    == participants =>
            {
                input.leases = leases;
            }
            Ok(_) => {
                return Completion {
                    ids: input.ids,
                    result: Err("PVE allegiance participant lease mismatch".into()),
                };
            }
            Err(error) => {
                return Completion {
                    ids: input.ids,
                    result: Err(error.to_string()),
                };
            }
        }
    }
    // One additional identity belongs to the durable death receipt, never to
    // the world forest. This also gives a zero-drop kill a concrete CAS row.
    let count = input.proposal.drops.len() + usize::from(!input.proposal.no_corpse) + 1;
    if input.ids.is_empty() {
        let count = match u16::try_from(count) {
            Ok(count) => count,
            Err(_) => {
                return Completion {
                    ids: input.ids,
                    result: Err("PVE identity count".into()),
                };
            }
        };
        if count != 0 {
            match store.allocate_dynamic_ids(count).await {
                Ok(ids) if ids.len() == usize::from(count) => {
                    input.ids = ids.into_iter().map(EntityId).collect();
                }
                Ok(_) => {
                    return Completion {
                        ids: input.ids,
                        result: Err("PVE identity allocation count".into()),
                    };
                }
                Err(error) => {
                    return Completion {
                        ids: input.ids,
                        result: Err(error.to_string()),
                    };
                }
            }
        }
    }
    let ids = input.ids.clone();
    let result = tokio::task::spawn_blocking(move || prepare_blocking(input))
        .await
        .map_err(|e| e.to_string())
        .and_then(|result| result);
    Completion { ids, result }
}
fn prepare_blocking(input: Input) -> Result<Prepared, String> {
    let proposal = &input.proposal;
    let native = proposal
        .native
        .as_ref()
        .ok_or("non-native PVE death unsupported")?;
    let position = proposal
        .position
        .as_ref()
        .ok_or("PVE accepted death pose missing")?;
    if input.ids.len() != proposal.drops.len() + usize::from(!proposal.no_corpse) + 1
        || input.ids.iter().any(|id| id.0 < 0x8000_0000)
        || input
            .ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != input.ids.len()
    {
        return Err("PVE frozen identity partition".into());
    }
    let lookup = |id| -> Result<bace_content::WeenieV1, String> {
        let PackLookup::Record(record) = input
            .generation
            .lookup(PackKey {
                namespace: 1,
                id: u64::from(id),
            })
            .map_err(|e| e.to_string())?
        else {
            return Err(format!("accepted PVE template {id} missing"));
        };
        let source = bace_content_tools::decode(record.bytes()).map_err(|e| e.to_string())?;
        if source.weenie_id != id {
            return Err("accepted PVE template identity".into());
        }
        Ok(source)
    };
    let mut at = 0;
    let corpse = if proposal.no_corpse {
        None
    } else {
        let id = input.ids[at];
        at += 1;
        Some(EntitySaveV1 {
            object_id: id.0,
            template_revision: input.generation.revision(),
            mutation_revision: 1,
            state: lookup(proposal.corpse_template)?,
        })
    };
    let mut items = Vec::with_capacity(proposal.drops.len());
    for (index, drop) in proposal.drops.iter().enumerate() {
        let source = if let Some(rows) = &native.source_items {
            rows.get(index)
                .filter(|row| row.weenie_id == drop.template)
                .ok_or("PVE exact selected source item missing")?
                .clone()
        } else {
            lookup(drop.template)?
        };
        items.push(EntitySaveV1 {
            object_id: input.ids[at].0,
            template_revision: input.generation.revision(),
            mutation_revision: 1,
            state: source,
        });
        at += 1;
    }
    let expires_at = input
        .unix_seconds
        .checked_add(
            i64::try_from(proposal.corpse_decay_ticks.div_ceil(30))
                .map_err(|_| "PVE decay overflow")?,
        )
        .ok_or("PVE expiry overflow")?;
    let mut operation = if let Some(corpse) = corpse {
        crate::death_saves::freeze_native_death(crate::death_saves::DeathFreezeInput {
            proposal,
            corpse,
            position: position.clone(),
            expires_at,
            items: &items,
            players: &input.players,
            leases: &input.leases,
        })
    } else {
        crate::death_saves::freeze_native_no_corpse_death(crate::death_saves::NoCorpseFreezeInput {
            proposal,
            position: position.clone(),
            items: &items,
            players: &input.players,
            leases: &input.leases,
            olthoi_killer: proposal.olthoi_killer,
        })
    }
    .map_err(|e| e.to_string())?;
    let allegiance = if let Some(ticket) = &proposal.social {
        let (nodes, metadata) = input
            .social_before
            .as_ref()
            .ok_or("PVE allegiance before-image missing")?;
        let mut item_operations = Vec::new();
        for (reward, inventory) in &ticket.item_experience {
            let Some(inventory) = inventory else { continue };
            let items = input
                .item_baselines
                .get(&reward.actor)
                .ok_or("PVE item reward baseline absent")?;
            let vitae = ticket
                .vitae
                .iter()
                .find(|patch| patch.actor == reward.actor)
                .map(|patch| &patch.registry);
            let frozen = crate::item_experience::freeze_item_experience_with_vitae(
                crate::game_inventory::InventoryFreezeInput {
                    operation_id: &operation.operation_id,
                    proposal: &inventory.proposal,
                    items,
                    other_snapshots: &operation.snapshots,
                    leases: &input.leases,
                    storage_views: &[],
                    admitted_positions: &BTreeMap::new(),
                },
                reward,
                inventory,
                vitae,
            )
            .map_err(|e| e.to_string())?;
            item_operations.push((inventory.operation, frozen));
        }
        operation = crate::item_reward_join::join_allegiance_item_operations(
            operation,
            ticket,
            &item_operations,
        )
        .map_err(|e| e.to_string())?;
        let mut allegiance = crate::allegiance_saves::freeze_allegiance(
            crate::allegiance_saves::AllegianceFreezeInput {
                world_epoch: input.epoch,
                operation: ticket.operation,
                patch: &ticket.patch,
                stored_nodes: nodes,
                stored_metadata: metadata,
                players: &[],
                leases: &input.leases,
            },
        )
        .map_err(|e| e.to_string())?;
        allegiance.operation_id = operation.operation_id.clone();
        operation
            .participants
            .extend(allegiance.nodes.iter().map(|row| row.character));
        operation
            .participants
            .extend(allegiance.metadata.iter().map(|row| row.character));
        operation.participants.sort_unstable();
        operation.participants.dedup();
        Some(allegiance)
    } else {
        None
    };
    let marker = input.ids[at];
    if at + 1 != input.ids.len() {
        return Err("PVE marker identity partition".into());
    }
    operation.snapshots.push(bace_persistence::SaveSnapshot {
        object_id: marker.0,
        mutation_revision: 1,
        expected_version: 0,
        bytes: bace_storage_codec::PveDeathReceiptV1 {
            marker_object_id: marker.0,
            event_id: native.event_id,
            victim_object_id: proposal.victim.0,
            position: position.clone(),
            world_epoch: input.epoch,
            killed: true,
        }
        .encode()
        .map_err(|e| e.to_string())?,
    });
    operation.participants.push(marker.0);
    operation.participants.sort_unstable();
    operation.participants.dedup();
    if operation.snapshots.len() > 1024 || operation.participants.len() > 1024 {
        return Err("PVE death receipt participant bound".into());
    }
    let mut snapshots = Vec::new();
    for change in &operation.changes {
        if !input.ids[..input.ids.len() - 1].contains(&EntityId(change.item)) {
            // Shared item-XP placements belong to the player's inventory owner,
            // not the new corpse or NoCorpse world forest.
            continue;
        }
        let snapshot = operation
            .snapshots
            .iter()
            .find(|snapshot| snapshot.object_id == change.item)
            .ok_or("PVE placement snapshot missing")?;
        let depth = match change.destination {
            DurableItemPlace::World { .. } => 0,
            DurableItemPlace::Contained { .. } => 1,
            DurableItemPlace::Removed => return Err("removed PVE item".into()),
        };
        snapshots.push(LocatedSnapshot {
            aggregate: StoredAggregate {
                object_id: snapshot.object_id,
                persisted_version: snapshot.expected_version + 1,
                bytes: snapshot.bytes.clone(),
            },
            placement: change.destination,
            depth,
        });
    }
    let mut assets = crate::region_activation::VerifiedRegionAssets::open(&input.manifest)?;
    let spells = assets.prepare_world_spell_table()?;
    let (forest, indexed) = crate::region_service::world_items::prepare_live_death(
        &snapshots,
        &input.region,
        &mut assets,
        &spells,
        crate::region_service::world_items::WorldItemRestoreClock {
            epoch: input.epoch,
            unix_seconds: input.unix_seconds,
            tick: input.tick,
        },
        proposal.no_corpse.then_some(proposal.victim),
    )?;
    let sources = snapshots
        .iter()
        .map(|snapshot| {
            indexed
                .get(&snapshot.aggregate.object_id)
                .cloned()
                .ok_or("PVE prepared source missing".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let visibility = if forest.roots.is_empty() {
        Vec::new()
    } else {
        let rows = forest
            .roots
            .iter()
            .map(|root| {
                let source = indexed
                    .get(&root.entity.0)
                    .ok_or("PVE visible root source absent")?;
                Ok(crate::visibility_assets::VisibilitySource {
                    entity: root.entity,
                    incarnation: proposal.operation,
                    revision: u64::try_from(source.item.persisted_version)
                        .map_err(|_| "PVE visible source version")?,
                    source: &source.item.entity.state,
                    equipment: Vec::new(),
                    missile_combat: false,
                })
            })
            .collect::<Result<Vec<_>, &str>>()?;
        assets.prepare_visibility_sources(rows)?
    };
    Ok(Prepared {
        operation,
        allegiance,
        forest,
        sources,
        visibility,
    })
}
