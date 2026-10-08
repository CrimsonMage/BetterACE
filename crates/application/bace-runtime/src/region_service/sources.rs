//! Atomic source-cache admission, prepared on a bounded cold worker.
use super::*;
use crate::region_unload_saves::RegionItemSource;
pub struct PreparedRegionSources {
    sources: Vec<RegionItemSource>,
    sizes: Vec<usize>,
}

fn corpse_open_contents_in(
    sources: &BTreeMap<u32, RegionItemSource>,
    corpse: EntityId,
) -> Result<(Vec<RegionItemSource>, Vec<RegionItemSource>), String> {
    let direct = container_children_in(sources, corpse)?;
    let mut nested = Vec::new();
    for child in &direct {
        let descendants = container_children_in(sources, EntityId(child.item.entity.object_id))?;
        if !descendants.is_empty()
            && !matches!(
                child.item.entity.state.weenie_type,
                14 | 20 | 21 | 56 | 57 | 58
            )
        {
            return Err("corpse non-container has contents".into());
        }
        nested.extend(descendants);
        if direct.len() + nested.len() > 1024 {
            return Err("corpse open content capacity".into());
        }
    }
    Ok((direct, nested))
}

fn container_children_in(
    sources: &BTreeMap<u32, RegionItemSource>,
    container_id: EntityId,
) -> Result<Vec<RegionItemSource>, String> {
    let mut children: Vec<_> = sources
        .values()
        .filter_map(|source| match source.item.placement {
            Some(bace_storage_codec::ItemPlacementV2::Contained {
                container,
                slot,
                pack_slot,
                ..
            }) if container == container_id.0 => Some((pack_slot, slot, source.clone())),
            _ => None,
        })
        .collect();
    if children.len() > 1024 {
        return Err("corpse child source capacity".into());
    }
    children
        .sort_by_key(|(pack_slot, slot, source)| (*pack_slot, *slot, source.item.entity.object_id));
    Ok(children.into_iter().map(|(_, _, source)| source).collect())
}

impl PreparedRegionSources {
    pub fn prepare(sources: Vec<RegionItemSource>) -> Result<Self, String> {
        if sources.len() > 1024 {
            return Err("region source batch count".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut sizes = Vec::new();
        let mut total = 0usize;
        for source in &sources {
            if source.item.entity.object_id == 0
                || !ids.insert(source.item.entity.object_id)
                || source.item.persisted_version != 0
                || source.corpse.is_some()
            {
                return Err("invalid transient source batch".into());
            }
            let bytes = if source.item.placement.is_some() {
                world_items::source_size(source)?
            } else {
                // A newly generated world root has no accepted pose yet. Size a
                // transient envelope only; do not invent or install a location.
                // Removed encodes one tag; 64 extra bytes upper-bound World pose.
                bace_storage_codec::ItemSaveV5 {
                    previous: bace_storage_codec::ItemSaveV4 {
                        previous: bace_storage_codec::ItemSaveV3 {
                            previous: bace_storage_codec::ItemSaveV2 {
                                entity: source.item.entity.clone(),
                                placement: bace_storage_codec::ItemPlacementV2::Removed,
                            },
                            enchantments: source.item.enchantments.clone(),
                        },
                        construction: source.item.construction.clone(),
                    },
                    source_destination: source.item.source_destination,
                }
                .encode()
                .map_err(|e| e.to_string())?
                .len()
                .checked_add(64)
                .ok_or("source size overflow")?
            };
            total = total.checked_add(bytes).ok_or("source byte overflow")?;
            if total > 64 * 1024 * 1024 {
                return Err("region source batch byte capacity".into());
            }
            sizes.push(bytes);
        }
        Ok(Self { sources, sizes })
    }
    pub fn sources(&self) -> &[RegionItemSource] {
        &self.sources
    }
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.sources.iter().map(|s| s.item.entity.object_id)
    }
}
impl RegionService {
    /// Admit committed NoCorpse world roots and their retained descendants as
    /// one cache update after the exact placement receipt.
    pub fn record_committed_world_item_sources(
        &mut self,
        landblock: u16,
        sources: Vec<RegionItemSource>,
    ) -> Result<(), String> {
        if sources.len() > 1024
            || self
                .loading
                .as_ref()
                .is_some_and(|l| l.phase == Phase::Submitted)
        {
            return Err("committed world item source batch bounds".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut sizes = Vec::with_capacity(sources.len());
        let mut bytes = self.source_bytes.values().sum::<usize>();
        let mut count = self.sources.len();
        for source in &sources {
            let id = source.item.entity.object_id;
            if id == 0
                || !ids.insert(id)
                || source.corpse.is_some()
                || source.item.corpse.is_some()
                || source.item.persisted_version <= 0
                || self
                    .source_regions
                    .get(&id)
                    .is_some_and(|region| *region != landblock)
            {
                return Err("committed world item source identity".into());
            }
            match source.item.placement.as_ref() {
                Some(bace_storage_codec::ItemPlacementV2::World(position))
                    if (position.obj_cell_id >> 16) as u16 == landblock => {}
                Some(bace_storage_codec::ItemPlacementV2::Contained {
                    container,
                    equipped: 0,
                    ..
                }) if ids.contains(container) && *container != id => {}
                _ => return Err("committed world item source ancestry".into()),
            }
            let size = world_items::source_size(source)?;
            if let Some(old) = self.sources.get(&id) {
                if old.item.persisted_version > source.item.persisted_version
                    || old.item.persisted_version == source.item.persisted_version
                        && (old.item.entity != source.item.entity
                            || old.item.placement != source.item.placement
                            || old.item.enchantments != source.item.enchantments
                            || old.item.construction != source.item.construction
                            || old.corpse.is_some())
                {
                    return Err("committed world item source version conflict".into());
                }
                bytes -= self
                    .source_bytes
                    .get(&id)
                    .copied()
                    .ok_or("source byte index missing")?;
            } else {
                count += 1;
            }
            bytes = bytes
                .checked_add(size)
                .ok_or("committed world item source byte overflow")?;
            sizes.push(size);
        }
        if count > 4096 || bytes > 64 * 1024 * 1024 {
            return Err("committed world item source cache capacity".into());
        }
        for (source, size) in sources.into_iter().zip(sizes) {
            let id = source.item.entity.object_id;
            self.source_regions.insert(id, landblock);
            self.source_bytes.insert(id, size);
            self.sources.insert(id, source);
        }
        Ok(())
    }

    /// Install an exact committed corpse forest together. Failed admission
    /// leaves the prior cache intact so the death delivery can be retried.
    pub fn record_committed_corpse_sources(
        &mut self,
        landblock: u16,
        sources: Vec<RegionItemSource>,
    ) -> Result<(), String> {
        if sources.is_empty()
            || sources.len() > 1024
            || self
                .loading
                .as_ref()
                .is_some_and(|l| l.phase == Phase::Submitted)
        {
            return Err("committed corpse source batch bounds".into());
        }
        let root = &sources[0];
        let corpse = root.corpse.as_ref().ok_or("committed corpse root absent")?;
        let root_id = root.item.entity.object_id;
        if root.item.persisted_version <= 0
            || root.item.entity != corpse.corpse.entity
            || root.item.placement.as_ref() != Some(&corpse.placement)
            || !matches!(&corpse.placement, bace_storage_codec::ItemPlacementV2::World(p) if (p.obj_cell_id >> 16) as u16 == landblock)
        {
            return Err("committed corpse root identity".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        ids.insert(root_id);
        let mut sizes = Vec::with_capacity(sources.len());
        let mut bytes = self.source_bytes.values().sum::<usize>();
        let mut count = self.sources.len();
        for (index, source) in sources.iter().enumerate() {
            let id = source.item.entity.object_id;
            if source.item.persisted_version <= 0
                || (index != 0 && source.corpse.is_some())
                || (index != 0 && !ids.insert(id))
                || self
                    .source_regions
                    .get(&id)
                    .is_some_and(|region| *region != landblock)
            {
                return Err("committed corpse source identity".into());
            }
            if index != 0 {
                let Some(bace_storage_codec::ItemPlacementV2::Contained { container, .. }) =
                    &source.item.placement
                else {
                    return Err("committed corpse descendant placement".into());
                };
                if !ids.contains(container) || *container == id {
                    return Err("committed corpse descendant ancestry".into());
                }
            }
            let size = world_items::source_size(source)?;
            let old = self.sources.get(&id);
            if old.is_some_and(|old| old.item.persisted_version > source.item.persisted_version) {
                return Err("committed corpse source stale version".into());
            }
            if let Some(old) = old {
                bytes -= self
                    .source_bytes
                    .get(&id)
                    .copied()
                    .ok_or("source byte index missing")?;
                if old.item.persisted_version == source.item.persisted_version
                    && (old.item.entity != source.item.entity
                        || old.item.placement != source.item.placement
                        || old.item.enchantments != source.item.enchantments
                        || old.item.construction != source.item.construction
                        || old.corpse != source.corpse)
                {
                    return Err("committed corpse same-version source conflict".into());
                }
            } else {
                count += 1;
            }
            bytes = bytes
                .checked_add(size)
                .ok_or("committed corpse source bytes overflow")?;
            sizes.push(size);
        }
        if count > 4096 || bytes > 64 * 1024 * 1024 {
            return Err("committed corpse source cache capacity".into());
        }
        for (mut source, size) in sources.into_iter().zip(sizes) {
            source.item.corpse = source.corpse.clone().map(Box::new);
            let id = source.item.entity.object_id;
            self.source_regions.insert(id, landblock);
            self.source_bytes.insert(id, size);
            self.sources.insert(id, source);
        }
        Ok(())
    }

    pub fn record_sources(
        &mut self,
        landblock: u16,
        batch: PreparedRegionSources,
    ) -> Result<(), Box<PreparedRegionSources>> {
        if !self.active.contains_key(&landblock)
            || self
                .loading
                .as_ref()
                .is_some_and(|l| l.phase == Phase::Submitted)
            || self
                .unload
                .as_ref()
                .is_some_and(|u| u.landblock() == landblock)
        {
            return Err(Box::new(batch));
        }
        let mut count = self.sources.len();
        let mut bytes = self.source_bytes.values().sum::<usize>();
        for (source, size) in batch.sources.iter().zip(&batch.sizes) {
            let id = source.item.entity.object_id;
            if let Some(old) = self.sources.get(&id) {
                if old.item.persisted_version != 0
                    || old.item.entity.state.weenie_id != source.item.entity.state.weenie_id
                    || self.source_regions.get(&id) != Some(&landblock)
                {
                    return Err(Box::new(batch));
                }
                bytes -= self.source_bytes.get(&id).copied().unwrap_or(0);
            } else {
                count += 1;
            }
            bytes += size;
        }
        if count > 4096 || bytes > 64 * 1024 * 1024 {
            return Err(Box::new(batch));
        }
        for (source, size) in batch.sources.into_iter().zip(batch.sizes) {
            let id = source.item.entity.object_id;
            self.source_regions.insert(id, landblock);
            self.source_bytes.insert(id, size);
            self.sources.insert(id, source);
        }
        Ok(())
    }
    pub fn transient_source(&self, id: EntityId) -> Option<&RegionItemSource> {
        self.sources
            .get(&id.0)
            .filter(|s| s.item.persisted_version == 0)
    }
    /// The accepted source cache identifies durable and transient corpse roots
    /// for authenticated Use dispatch; permission remains with simulation.
    pub fn corpse_source(&self, id: EntityId) -> Option<&RegionItemSource> {
        self.sources
            .get(&id.0)
            .filter(|source| source.corpse.is_some())
    }
    pub fn world_item_source(&self, id: EntityId) -> Option<&RegionItemSource> {
        self.sources.get(&id.0).filter(|source| {
            source.corpse.is_none()
                && matches!(
                    source.item.placement,
                    Some(bace_storage_codec::ItemPlacementV2::World(_))
                )
        })
    }
    /// Read direct accepted children in container slot order for canonical Open.
    pub fn corpse_children(&self, corpse: EntityId) -> Result<Vec<RegionItemSource>, String> {
        if self.corpse_source(corpse).is_none() {
            return Err("corpse source missing".into());
        }
        self.container_children(corpse)
    }

    /// ACE Container.SendInventory publishes the corpse's direct contents and
    /// one level under each direct subcontainer in one Open. Each subcontainer
    /// also receives its own ViewContents before object descriptions.
    pub fn corpse_open_contents(
        &self,
        corpse: EntityId,
    ) -> Result<(Vec<RegionItemSource>, Vec<RegionItemSource>), String> {
        if self.corpse_source(corpse).is_none() {
            return Err("corpse source missing".into());
        }
        corpse_open_contents_in(&self.sources, corpse)
    }

    fn container_children(&self, container_id: EntityId) -> Result<Vec<RegionItemSource>, String> {
        container_children_in(&self.sources, container_id)
    }
    /// Adopt the exact V5 checkpoint only after its confirmed durable receipt.
    /// Unload and expiry subsequently freeze this newer source and CAS version.
    pub fn replace_corpse_source(
        &mut self,
        id: EntityId,
        expected_version: i64,
        saved: bace_storage_codec::CorpseSaveV5,
        new_version: i64,
    ) -> Result<(), String> {
        let current = self.corpse_source(id).ok_or("corpse source missing")?;
        if current.item.persisted_version != expected_version
            || new_version <= expected_version
            || saved.corpse.entity.object_id != id.0
            || saved.operation != current.corpse.as_ref().and_then(|c| c.operation)
        {
            return Err("corpse source receipt identity/version mismatch".into());
        }
        let mut updated = current.clone();
        updated.item.entity = saved.corpse.entity.clone();
        updated.item.placement = Some(saved.placement.clone());
        updated.item.enchantments = saved.enchantments.clone();
        updated.item.persisted_version = new_version;
        updated.item.corpse = Some(Box::new(saved.clone()));
        updated.corpse = Some(saved);
        let bytes = world_items::source_size(&updated)?;
        let previous = self
            .source_bytes
            .get(&id.0)
            .copied()
            .ok_or("corpse source size missing")?;
        let total = self
            .source_bytes
            .values()
            .sum::<usize>()
            .checked_sub(previous)
            .and_then(|size| size.checked_add(bytes))
            .ok_or("corpse source byte overflow")?;
        if total > 64 * 1024 * 1024 {
            return Err("corpse source byte capacity".into());
        }
        self.source_bytes.insert(id.0, bytes);
        self.sources.insert(id.0, updated);
        Ok(())
    }
    /// Called only after a definitive rejected root tree or an accepted lifecycle
    /// retirement/detachment. Source metadata never owns the world item itself.
    pub fn forget_transient_sources(&mut self, ids: &[EntityId]) {
        for id in ids {
            if self
                .sources
                .get(&id.0)
                .is_some_and(|s| s.item.persisted_version == 0)
            {
                self.sources.remove(&id.0);
                self.source_bytes.remove(&id.0);
                self.source_regions.remove(&id.0);
            }
        }
    }
    /// Exact committed acquisition/removal may retire durable as well as
    /// transient source metadata. This never removes a world/inventory actor.
    pub fn forget_inventory_sources(&mut self, ids: &[EntityId]) -> Result<(), String> {
        if ids.len() > 1024 {
            return Err("inventory source retirement capacity".into());
        }
        for id in ids {
            self.sources.remove(&id.0);
            self.source_bytes.remove(&id.0);
            self.source_regions.remove(&id.0);
        }
        Ok(())
    }
    pub fn forget_transient_tree(&mut self, root: EntityId) {
        let mut ids = std::collections::BTreeSet::from([root.0]);
        for _ in 0..64 {
            let before = ids.len();
            for (&id, s) in &self.sources {
                if let Some(bace_storage_codec::ItemPlacementV2::Contained { container, .. }) =
                    s.item.placement.as_ref()
                    && ids.contains(container)
                {
                    ids.insert(id);
                }
            }
            if before == ids.len() {
                break;
            }
        }
        self.forget_transient_sources(&ids.into_iter().map(EntityId).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod open_tests {
    use super::*;
    use bace_content::WeenieV1;
    use bace_storage_codec::{EntitySaveV1, ItemPlacementV2};

    fn source(id: u32, parent: u32, slot: u32, weenie_type: u32) -> RegionItemSource {
        RegionItemSource {
            item: crate::game_inventory::FrozenInventoryItem {
                corpse: None,
                construction: None,
                source_destination: None,
                enchantments: vec![],
                entity: EntitySaveV1 {
                    object_id: id,
                    template_revision: 1,
                    mutation_revision: 1,
                    state: WeenieV1 {
                        schema_version: 1,
                        weenie_id: id,
                        class_name: format!("corpse_item_{id}"),
                        weenie_type,
                        last_modified: None,
                        properties: Default::default(),
                    },
                },
                placement: Some(ItemPlacementV2::Contained {
                    container: parent,
                    slot,
                    pack_slot: false,
                    equipped: 0,
                }),
                persisted_version: 1,
            },
            corpse: None,
        }
    }

    #[test]
    fn open_includes_one_nested_level_in_source_slot_order() {
        let sources = BTreeMap::from([
            (2, source(2, 1, 1, 20)),
            (3, source(3, 1, 0, 51)),
            (4, source(4, 2, 2, 51)),
            (5, source(5, 2, 0, 21)),
            (6, source(6, 5, 0, 51)),
        ]);
        let (direct, nested) = corpse_open_contents_in(&sources, EntityId(1)).unwrap();
        assert_eq!(
            direct
                .iter()
                .map(|s| s.item.entity.object_id)
                .collect::<Vec<_>>(),
            [3, 2]
        );
        assert_eq!(
            nested
                .iter()
                .map(|s| s.item.entity.object_id)
                .collect::<Vec<_>>(),
            [5, 4]
        );
        assert!(sources.contains_key(&6));
        let mut invalid = sources;
        invalid.insert(7, source(7, 3, 0, 51));
        assert!(corpse_open_contents_in(&invalid, EntityId(1)).is_err());
    }
}
