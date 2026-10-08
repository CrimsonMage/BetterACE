//! Cold accepted-region rendering closure, retaining native equipped sources.
use super::*;
use crate::{
    region_unload_saves::RegionItemSource,
    visibility_assets::{PreparedVisibilityObject, VisibilitySource, prepare_visibility_object},
};
use bace_types::EntityId;
impl VerifiedRegionAssets {
    pub fn prepare_visibility_sources(
        &mut self,
        rows: Vec<VisibilitySource<'_>>,
    ) -> Result<Vec<PreparedVisibilityObject>, String> {
        let sources: Vec<_> = rows
            .iter()
            .flat_map(|r| std::iter::once(r.source).chain(r.equipment.iter().map(|(_, s, _)| *s)))
            .collect();
        let appearance = self.prepare_entry_appearance(&sources)?;
        let chargen = bace_dat::CharGen::decode(
            &self
                .portal
                .read(bace_dat::CharGen::RECORD_ID)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let assets = appearance.borrowed(&chargen);
        rows.into_iter()
            .map(|row| prepare_visibility_object(row, &assets))
            .collect()
    }
    pub fn prepare_region_visibility(
        &mut self,
        prepared: &PreparedRegionActivation,
        encounters: &BTreeMap<u32, EntityId>,
        admitted: &std::collections::BTreeSet<EntityId>,
        items: &bace_simulation::PreparedWorldRegionItems,
        sources: &BTreeMap<u32, RegionItemSource>,
    ) -> Result<Vec<PreparedVisibilityObject>, String> {
        let mut rows = Vec::new();
        for (id, source) in prepared
            .content
            .instances
            .iter()
            .filter(|i| !i.source.is_link_child)
            .map(|i| (EntityId(i.source.guid), i.template.as_ref()))
            .chain(
                prepared
                    .content
                    .encounters
                    .iter()
                    .map(|e| (encounters[&e.source.id], e.template.as_ref())),
            )
        {
            if !admitted.contains(&id) {
                continue;
            }
            let equipment = sources
                .values()
                .filter_map(|item| match item.item.placement {
                    Some(bace_storage_codec::ItemPlacementV2::Contained {
                        container,
                        equipped,
                        ..
                    }) if container == id.0 && equipped != 0 => Some((
                        EntityId(item.item.entity.object_id),
                        &item.item.entity.state,
                        equipped,
                    )),
                    _ => None,
                })
                .collect();
            rows.push(VisibilitySource {
                entity: id,
                incarnation: prepared.fence.activation_epoch,
                revision: prepared.fence.content_generation.max(1),
                source,
                equipment,
                missile_combat: false,
            });
        }
        for root in &items.roots {
            let source = sources
                .get(&root.entity.0)
                .ok_or("restored visible root source absent")?;
            rows.push(VisibilitySource {
                entity: root.entity,
                incarnation: prepared.fence.activation_epoch,
                revision: u64::try_from(source.item.persisted_version.max(1))
                    .map_err(|_| "negative visible revision")?,
                source: &source.item.entity.state,
                equipment: vec![],
                missile_combat: false,
            });
        }
        self.prepare_visibility_sources(rows)
    }
}
