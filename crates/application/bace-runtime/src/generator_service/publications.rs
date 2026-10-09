//! A generator's exact cold row/model survives admission and reliable creation.
//! The current catalog is never consulted after that birth has been accepted.
use super::*;
use crate::{
    npc_sources::PreparedNpcRegistration,
    visibility_assets::{PreparedVisibilityObject, VisibilitySource},
};
use bace_content::WeenieV1;
use bace_inventory::{InventoryItem, ItemPlace};
#[derive(Clone)]
pub struct PreparedGeneratorPublication {
    pub key: GeneratorSpawnKey,
    pub entity: EntityId,
    pub visibility: PreparedVisibilityObject,
    pub npc: Option<PreparedNpcRegistration>,
    pub retained_bytes: usize,
}
#[cfg(test)]
impl<S: GeneratorRepository> GeneratorService<S> {
    pub(crate) fn retain_test_publication(&mut self, publication: PreparedGeneratorPublication) {
        self.publication_bytes += publication.retained_bytes;
        self.publication_count += 1;
        assert!(
            self.publications
                .insert(publication.entity, publication)
                .is_none()
        );
    }
}
pub(super) fn prepare(
    assets: Result<&mut crate::region_activation::VerifiedRegionAssets, String>,
    region: &PreparedRegionActivation,
    request: &GeneratorHostRequest,
    raw: &materialization::Materialized,
    ready: &mut materialization::Ready,
) -> Result<Vec<PreparedGeneratorPublication>, String> {
    let action = &ready.action;
    let roots: Vec<_> = match action {
        GeneratorAction::AdmitCreature { .. } => {
            request.entities.first().copied().into_iter().collect()
        }
        GeneratorAction::AdmitMixedTrees { roots, .. } => {
            roots.iter().map(|r| r.entity()).collect()
        }
        GeneratorAction::AdmitItemTrees {
            roots,
            shapes: Some(_),
            ..
        } => roots.clone(),
        GeneratorAction::AdmitItems {
            items,
            shapes: Some(_),
            ..
        } => items.iter().map(|i| i.id).collect(),
        _ => return Ok(vec![]),
    };
    if roots.len() > 1024 {
        return Err("generator publication root capacity".into());
    }
    let assets = assets?;
    let mut sources: BTreeMap<EntityId, Arc<WeenieV1>> = ready
        .sources
        .iter()
        .map(|s| {
            (
                EntityId(s.item.entity.object_id),
                Arc::new(s.item.entity.state.clone()),
            )
        })
        .collect();
    add_creatures(raw, &request.entities, &mut sources)?;
    let mut items: Vec<&InventoryItem> = Vec::new();
    match action {
        GeneratorAction::AdmitCreature { loadout, .. } => items.extend(loadout.items.iter()),
        GeneratorAction::AdmitMixedTrees { roots, .. } => {
            for root in roots {
                match root {
                    bace_simulation::PreparedMixedGeneratorRoot::Creature { loadout, .. } => {
                        items.extend(loadout.items.iter())
                    }
                    bace_simulation::PreparedMixedGeneratorRoot::Item { items: rows, .. } => {
                        items.extend(rows.iter())
                    }
                }
            }
        }
        GeneratorAction::AdmitItems { items: rows, .. }
        | GeneratorAction::AdmitItemTrees { items: rows, .. } => items.extend(rows.iter()),
        _ => {}
    }
    let mut inputs = Vec::new();
    let mut npcs = BTreeMap::new();
    for &entity in &roots {
        let source = sources
            .get(&entity)
            .ok_or("generator publication exact source missing")?;
        let equipment = items
            .iter()
            .filter_map(|item| match item.place {
                ItemPlace::Contained {
                    container,
                    equipped,
                    ..
                } if container == entity && equipped != 0 => Some((item.id, equipped)),
                _ => None,
            })
            .map(|(id, location)| {
                Ok((
                    id,
                    sources
                        .get(&id)
                        .ok_or("generator public attachment source missing")?
                        .as_ref(),
                    location,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        inputs.push(VisibilitySource {
            entity,
            incarnation: request.intent.key.generator.incarnation,
            revision: 1,
            source,
            equipment,
            missile_combat: false,
        });
        if let Some(npc) = crate::npc_sources::prepare_generated_registration(
            entity,
            request.intent.key.generator.incarnation,
            request.landblock,
            region.generation.clone(),
            region.source_manifest,
            source,
        )? {
            npcs.insert(entity, npc);
        }
    }
    let visuals = assets.prepare_visibility_sources(inputs)?;
    let mut publications = Vec::new();
    let limits = bace_wire::ObjectCodecLimits {
        max_message_bytes: 1024 * 1024,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 4096,
    };
    let mut total = 0usize;
    for visibility in visuals {
        let entity = EntityId(visibility.description.object_id);
        let npc = npcs.remove(&entity);
        let mut bytes = visibility
            .description
            .encode_create(limits)
            .map_err(|e| e.to_string())?
            .len();
        for child in &visibility.children {
            bytes = bytes
                .checked_add(
                    child
                        .encode_create(limits)
                        .map_err(|e| e.to_string())?
                        .len(),
                )
                .ok_or("generator public byte overflow")?;
        }
        bytes = bytes
            .checked_add(npc.as_ref().map_or(0, |n| n.source.retained_bytes))
            .ok_or("generator source byte overflow")?;
        total = total
            .checked_add(bytes)
            .filter(|n| *n <= MAX_BYTES)
            .ok_or("generator publication byte capacity")?;
        publications.push(PreparedGeneratorPublication {
            key: request.intent.key,
            entity,
            visibility,
            npc,
            retained_bytes: bytes,
        });
    }
    let mut scripts: Vec<_> = publications
        .iter()
        .filter_map(|p| p.npc.as_ref().map(|n| (p.entity, n.script_source())))
        .collect();
    scripts.sort_by_key(|(id, _)| *id);
    if !scripts.is_empty() {
        let placeholder = GeneratorAction::SetDay(false);
        let action = std::mem::replace(&mut ready.action, placeholder);
        ready.action = GeneratorAction::AdmitWithScripts {
            sources: scripts,
            action: Box::new(action),
        };
    }
    Ok(publications)
}
fn add_creatures(
    raw: &materialization::Materialized,
    ids: &[EntityId],
    out: &mut BTreeMap<EntityId, Arc<WeenieV1>>,
) -> Result<(), String> {
    if raw.count() != ids.len() {
        return Err("generator raw/public identity count".into());
    }
    match raw {
        materialization::Materialized::Creature { source, .. } => {
            out.insert(ids[0], source.clone());
        }
        materialization::Materialized::Items(_)
        | materialization::Materialized::NestedItems { .. } => {}
        materialization::Materialized::Mixed(trees) => {
            let mut at = 0;
            for tree in trees {
                if matches!(tree, materialization::Materialized::Mixed(_)) {
                    return Err("nested publication forest".into());
                }
                let end = at + tree.count();
                add_creatures(tree, &ids[at..end], out)?;
                at = end;
            }
        }
    }
    Ok(())
}
