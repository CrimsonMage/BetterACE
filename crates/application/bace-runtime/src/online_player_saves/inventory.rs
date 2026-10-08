//! Preserve frozen item metadata while capturing accepted owned registry changes.
use super::*;
use bace_inventory::ItemPlace;
use bace_storage_codec::{ItemPlacementV2, ItemSaveV2, ItemSaveV5};
use std::collections::BTreeSet;
impl OnlinePlayerSaveService {
    pub fn register_loaded(
        &mut self,
        loaded: &crate::game_login::LoadedPlayer,
        online_lease: CharacterLease,
        now: Duration,
    ) -> Result<(), String> {
        self.register(
            loaded.binding,
            online_lease,
            loaded.player.clone(),
            loaded.persisted_version,
            now,
        )?;
        if let Err(error) = self.register_loaded_inventory(loaded) {
            self.dirty
                .forget_clean(loaded.binding.actor.0)
                .expect("new clean player");
            self.players.remove(&loaded.binding.actor.0);
            return Err(error);
        }
        Ok(())
    }
    /// Exact current-before views for a valuable operation's authenticated hold.
    /// This does not adopt a routine revision or claim that the rows are durable.
    pub fn operation_inventory_changes(
        &self,
        snapshot: &bace_simulation::PlayerReadSnapshot,
    ) -> Result<Vec<SaveSnapshot>, String> {
        let actor = snapshot.binding().actor.0;
        let player = self.players.get(&actor).ok_or("unknown operation player")?;
        if !player.reserved
            || snapshot.binding() != player.binding
            || snapshot.operation().is_none()
        {
            return Err("operation inventory snapshot without matching hold".into());
        }
        freeze_items(actor, &self.items, snapshot)
    }
    pub fn operation_inventory_baselines(
        &self,
        snapshot: &bace_simulation::PlayerReadSnapshot,
    ) -> Result<Vec<crate::game_inventory::FrozenInventoryItem>, String> {
        let actor = snapshot.binding().actor.0;
        let changed = self.operation_inventory_changes(snapshot)?;
        let mut all = self.inventory_baselines(actor);
        for change in changed {
            let after = ItemSaveV5::decode(&change.bytes).map_err(|e| e.to_string())?;
            let row = all
                .iter_mut()
                .find(|i| i.entity.object_id == change.object_id)
                .ok_or("operation item missing baseline")?;
            if row.source_destination != after.source_destination {
                return Err("operation item source destination changed".into());
            }
            row.source_destination = after.source_destination;
            row.entity = after.entity.clone();
            row.placement = Some(after.placement.clone());
            row.enchantments = after.previous.previous.enchantments;
            row.construction = after.previous.construction;
        }
        Ok(all)
    }
    /// Immutable pre-proposal views. These do not authorize persistence; the
    /// simulation must still validate every captured revision before reservation.
    pub fn captured_inventory_baselines(
        &self,
        snapshot: &bace_simulation::PlayerReadSnapshot,
    ) -> Result<Vec<crate::game_inventory::FrozenInventoryItem>, String> {
        let actor = snapshot.binding().actor.0;
        let player = self.players.get(&actor).ok_or("unknown captured player")?;
        if player.binding != snapshot.binding() || snapshot.operation().is_some() {
            return Err("pre-proposal capture identity".into());
        }
        let changed = freeze_items(actor, &self.items, snapshot)?;
        let mut all = self.inventory_baselines(actor);
        for change in changed {
            let after = ItemSaveV5::decode(&change.bytes).map_err(|e| e.to_string())?;
            let row = all
                .iter_mut()
                .find(|row| row.entity.object_id == change.object_id)
                .ok_or("captured item baseline missing")?;
            if row.source_destination != after.source_destination {
                return Err("captured item source destination changed".into());
            }
            row.source_destination = after.source_destination;
            row.entity = after.entity.clone();
            row.placement = Some(after.placement.clone());
            row.enchantments = after.previous.previous.enchantments;
            row.construction = after.previous.construction;
        }
        Ok(all)
    }
    /// Borrow accepted durable metadata without cloning every item/registry.
    pub fn inventory_baseline(&self, actor: u32, id: u32) -> Option<&ItemSaveV4> {
        self.items
            .get(&id)
            .filter(|item| item.owner == actor)
            .map(|item| &item.saved)
    }
    pub fn inventory_source_destination(&self, actor: u32, id: u32) -> Option<u8> {
        self.items
            .get(&id)
            .filter(|item| item.owner == actor)
            .and_then(|item| item.source_destination)
    }
    pub fn equipped_inventory_baselines(&self, actor: u32) -> Vec<&ItemSaveV4> {
        self.items.values().filter(|i|i.owner==actor&&matches!(i.saved.placement,bace_storage_codec::ItemPlacementV2::Contained{equipped,..}if equipped!=0)).map(|i|&i.saved).collect()
    }
    pub fn inventory_count(&self, actor: u32) -> usize {
        self.items
            .values()
            .filter(|item| item.owner == actor)
            .count()
    }
    pub fn inventory_baselines(
        &self,
        actor: u32,
    ) -> Vec<crate::game_inventory::FrozenInventoryItem> {
        self.items
            .values()
            .filter(|i| i.owner == actor)
            .map(|i| crate::game_inventory::FrozenInventoryItem {
                corpse: None,
                construction: i.saved.construction.clone(),
                source_destination: i.source_destination,
                enchantments: i.saved.enchantments.clone(),
                entity: i.saved.entity.clone(),
                placement: Some(i.saved.placement.clone()),
                persisted_version: i.version,
            })
            .collect()
    }
    /// Complete initial owned inventory, before routine scheduling begins.
    pub fn register_inventory(
        &mut self,
        actor: u32,
        items: Vec<(ItemSaveV4, i64)>,
    ) -> Result<(), String> {
        self.register_inventory_with_origins(
            actor,
            items
                .into_iter()
                .map(|(saved, version)| (saved, None, version))
                .collect(),
        )
    }
    pub fn register_inventory_v5(
        &mut self,
        actor: u32,
        items: Vec<(ItemSaveV5, i64)>,
    ) -> Result<(), String> {
        self.register_inventory_with_origins(
            actor,
            items
                .into_iter()
                .map(|(saved, version)| (saved.previous, saved.source_destination, version))
                .collect(),
        )
    }
    fn register_inventory_with_origins(
        &mut self,
        actor: u32,
        items: Vec<(ItemSaveV4, Option<u8>, i64)>,
    ) -> Result<(), String> {
        if self.players.get(&actor).is_none_or(|p| p.reserved)
            || self.items.values().any(|i| i.owner == actor)
            || self.capture.is_some()
            || self.writes.contains_key(&actor)
            || items.len() > 1023
        {
            return Err("inventory baseline registration state".into());
        }
        let mut rows = Vec::with_capacity(items.len());
        let mut seen = BTreeSet::new();
        for (saved, source_destination, version) in items {
            let id = saved.entity.object_id;
            if !seen.insert(id)
                || self.players.contains_key(&id)
                || self.items.contains_key(&id)
                || version <= 0
            {
                return Err("inventory baseline identity/version".into());
            }
            let bytes = ItemSaveV5 {
                previous: saved.clone(),
                source_destination,
            }
            .encode()
            .map_err(|e| e.to_string())?;
            rows.push((
                SaveSnapshot {
                    object_id: id,
                    mutation_revision: saved.entity.mutation_revision,
                    expected_version: version,
                    bytes,
                },
                Item {
                    owner: actor,
                    saved,
                    source_destination,
                    version,
                },
            ));
        }
        for (_, item) in &rows {
            validate_ancestry(
                actor,
                &item.saved,
                &rows
                    .iter()
                    .map(|(_, i)| (i.saved.entity.object_id, &i.saved))
                    .collect(),
            )?;
        }
        if rows.iter().map(|(s, _)| s.bytes.len()).sum::<usize>() + self.dirty.retained_bytes()
            > self.byte_limit
        {
            return Err("inventory baseline byte budget".into());
        }
        let mut installed = Vec::new();
        for (snapshot, item) in rows {
            let id = snapshot.object_id;
            if let Err(error) = self.dirty.register_clean(snapshot) {
                for id in installed {
                    self.dirty.forget_clean(id).expect("new clean baseline");
                    self.items.remove(&id);
                }
                return Err(error.to_string());
            }
            installed.push(id);
            self.items.insert(id, item);
        }
        Ok(())
    }
    pub fn register_loaded_inventory(
        &mut self,
        loaded: &crate::game_login::LoadedPlayer,
    ) -> Result<(), String> {
        self.register_inventory_with_origins(
            loaded.binding.actor.0,
            loaded
                .inventory
                .iter()
                .map(|i| {
                    (
                        bace_storage_codec::ItemSaveV4 {
                            previous: bace_storage_codec::ItemSaveV3 {
                                previous: ItemSaveV2 {
                                    entity: i.entity.clone(),
                                    placement: i.placement.clone(),
                                },
                                enchantments: i.enchantments.clone(),
                            },
                            construction: i.construction.clone(),
                        },
                        i.source_destination,
                        i.persisted_version,
                    )
                })
                .collect(),
        )
    }
    pub(super) fn finish_critical_rows(
        &mut self,
        committed: &[SaveSnapshot],
    ) -> Result<(), String> {
        let actors: Vec<_> = committed
            .iter()
            .filter(|s| self.players.contains_key(&s.object_id))
            .map(|s| s.object_id)
            .collect();
        self.finish_critical_owner_rows(&actors, committed)
    }
    pub(super) fn finish_critical_owner_rows(
        &mut self,
        actors: &[u32],
        committed: &[SaveSnapshot],
    ) -> Result<(), String> {
        let unique: BTreeSet<_> = actors.iter().copied().collect();
        if actors.is_empty()
            || unique.len() != actors.len()
            || actors.len() > 1024
            || actors
                .iter()
                .any(|id| self.players.get(id).is_none_or(|p| !p.reserved))
        {
            return Err("critical reserved player identities".into());
        }
        let actors = unique;
        let mut players = BTreeMap::new();
        let mut decoded = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for s in committed {
            if !seen.insert(s.object_id) {
                return Err("duplicate critical snapshot".into());
            }
            if let Some(p) = self.players.get(&s.object_id) {
                let next = PlayerSaveV6::decode(&s.bytes).map_err(|e| e.to_string())?;
                if !actors.contains(&s.object_id)
                    || !p.reserved
                    || next.player.entity.object_id != s.object_id
                    || next.player.account_id != p.binding.account.0
                    || next.player.entity.mutation_revision != s.mutation_revision
                {
                    return Err("critical player identity".into());
                }
                players.insert(s.object_id, next);
            } else {
                if self
                    .items
                    .get(&s.object_id)
                    .is_some_and(|i| !actors.contains(&i.owner))
                {
                    return Err("critical item outside reserved owners".into());
                }
                let (next, _) = crate::game_inventory::decode_inventory_item(&s.bytes, None)
                    .map_err(|e| e.to_string())?;
                if next.entity.object_id != s.object_id
                    || next.entity.mutation_revision != s.mutation_revision
                    || self
                        .items
                        .get(&s.object_id)
                        .is_some_and(|item| item.source_destination != next.source_destination)
                {
                    return Err("critical item identity".into());
                }
                decoded.insert(s.object_id, next);
            }
        }
        let reserved: Vec<_> = actors
            .iter()
            .copied()
            .chain(
                self.items
                    .iter()
                    .filter(|(_, i)| actors.contains(&i.owner))
                    .map(|(&id, _)| id),
            )
            .collect();
        let mut graph: BTreeMap<_, _> = self
            .items
            .iter()
            .filter(|(_, i)| actors.contains(&i.owner))
            .map(|(&id, i)| (id, &i.saved))
            .collect();
        graph.extend(decoded.iter().map(|(&id, s)| (id, &s.previous)));
        let mut owners = BTreeMap::new();
        let mut retired = Vec::new();
        for (&id, saved) in &graph {
            let owner = final_owner(saved, &graph, &actors)?;
            if let Some(owner) = owner {
                owners.insert(id, owner);
            } else if self.items.contains_key(&id) {
                retired.push(id);
            }
        }
        for actor in &actors {
            if owners.values().filter(|id| *id == actor).count() > 1023 {
                return Err("critical inventory capacity".into());
            }
        }
        let changed: Vec<_> = committed
            .iter()
            .filter(|s| {
                self.players.contains_key(&s.object_id) || self.items.contains_key(&s.object_id)
            })
            .cloned()
            .collect();
        let acquired: Vec<_> = committed
            .iter()
            .filter(|s| {
                !self.players.contains_key(&s.object_id)
                    && !self.items.contains_key(&s.object_id)
                    && owners.contains_key(&s.object_id)
            })
            .cloned()
            .collect();
        self.dirty
            .finish_reserved_hierarchy(&reserved, &changed, &acquired, &retired)
            .map_err(|e| e.to_string())?;
        for actor in &actors {
            self.players
                .get_mut(actor)
                .expect("validated reserved owner")
                .reserved = false;
        }
        for (id, next) in players {
            let s = committed
                .iter()
                .find(|s| s.object_id == id)
                .expect("decoded row");
            let p = self.players.get_mut(&id).expect("validated player");
            p.saved = next;
            p.version = s.expected_version;
            if p.notice == p.critical_notice {
                p.first_dirty = None;
            }
            p.reserved = false;
        }
        for id in retired {
            self.items.remove(&id);
        }
        for (id, owner) in owners {
            if let Some(next) = decoded.remove(&id) {
                let s = committed
                    .iter()
                    .find(|s| s.object_id == id)
                    .expect("decoded item");
                self.items.insert(
                    id,
                    Item {
                        owner,
                        saved: next.previous,
                        source_destination: next.source_destination,
                        version: s.expected_version,
                    },
                );
            } else {
                self.items
                    .get_mut(&id)
                    .expect("known unchanged descendant")
                    .owner = owner;
            }
        }
        Ok(())
    }
}
fn final_owner(
    item: &ItemSaveV4,
    items: &BTreeMap<u32, &ItemSaveV4>,
    actors: &BTreeSet<u32>,
) -> Result<Option<u32>, String> {
    let mut current = item;
    let mut seen = BTreeSet::new();
    for _ in 0..17 {
        if !seen.insert(current.entity.object_id) {
            return Err("critical inventory cycle".into());
        }
        let ItemPlacementV2::Contained { container, .. } = current.placement else {
            return Ok(None);
        };
        if actors.contains(&container) {
            return Ok(Some(container));
        }
        let Some(parent) = items.get(&container) else {
            return Ok(None);
        };
        current = parent;
    }
    Err("critical inventory depth".into())
}
fn validate_ancestry(
    actor: u32,
    item: &ItemSaveV4,
    items: &BTreeMap<u32, &ItemSaveV4>,
) -> Result<(), String> {
    let mut current = item;
    let mut seen = BTreeSet::new();
    for _ in 0..17 {
        if !seen.insert(current.entity.object_id) {
            break;
        }
        let ItemPlacementV2::Contained { container, .. } = current.placement else {
            break;
        };
        if container == actor {
            return Ok(());
        }
        current = items
            .get(&container)
            .ok_or("missing inventory baseline ancestor")?;
    }
    Err("inventory baseline ancestry".into())
}
pub(super) fn freeze_items(
    actor: u32,
    baselines: &BTreeMap<u32, Item>,
    snapshot: &bace_simulation::PlayerReadSnapshot,
) -> Result<Vec<SaveSnapshot>, String> {
    freeze_item_views_with_mana(
        actor,
        baselines,
        snapshot.items(),
        snapshot.item_enchantments(),
        snapshot.equipment_mana(),
    )
}
#[cfg(test)]
pub(super) fn freeze_item_views(
    actor: u32,
    baselines: &BTreeMap<u32, Item>,
    current_items: &[bace_inventory::InventoryItem],
    registries: &[(bace_types::EntityId, bace_magic::EnchantmentRegistry)],
) -> Result<Vec<SaveSnapshot>, String> {
    freeze_item_views_with_mana(actor, baselines, current_items, registries, None)
}
pub(super) fn freeze_item_views_with_mana(
    actor: u32,
    baselines: &BTreeMap<u32, Item>,
    current_items: &[bace_inventory::InventoryItem],
    registries: &[(bace_types::EntityId, bace_magic::EnchantmentRegistry)],
    mana: Option<&bace_simulation::EquipmentManaRecovery>,
) -> Result<Vec<SaveSnapshot>, String> {
    let expected: BTreeSet<_> = baselines
        .iter()
        .filter(|(_, i)| i.owner == actor)
        .map(|(&id, _)| id)
        .collect();
    let actual: BTreeSet<_> = current_items.iter().map(|i| i.id.0).collect();
    if expected != actual {
        return Err("inventory membership changed without durable baseline handoff".into());
    }
    let mut out = Vec::new();
    for current in current_items {
        let old = &baselines[&current.id.0];
        let ItemPlace::Contained {
            container,
            slot,
            equipped,
        } = current.place
        else {
            return Err("routine inventory is not owned".into());
        };
        let placement = ItemPlacementV2::Contained {
            container: container.0,
            slot,
            pack_slot: current.pack_slot,
            equipped,
        };
        if old.saved.placement != placement
            || old.saved.entity.state.weenie_id != current.template
            || current.revision < old.saved.entity.mutation_revision
        {
            return Err("routine item placement/revision changed without receipt".into());
        }
        let mut next = old.saved.clone();
        next.entity.mutation_revision = current.revision;
        if let Some(mana) =
            mana.and_then(|state| state.items.iter().find(|item| item.item == current.id))
        {
            crate::equipment_mana::overlay_equipment_mana(&mut next.entity.state, mana);
        }

        let registry = registries
            .iter()
            .find(|(id, _)| *id == current.id)
            .map(|(_, r)| r);
        if let Some(registry) = registry {
            next.enchantments = registry
                .entries()
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
        } else if !next.enchantments.is_empty() {
            return Err("missing owned item registry".into());
        }
        let ints = &mut next.entity.state.properties.ints;
        if current.maximum_stack > 1 || ints.iter().any(|p| p.id == 12) {
            set(
                ints,
                12,
                Some(i32::try_from(current.stack).map_err(|_| "item stack overflow")?),
            );
        }
        set(
            ints,
            92,
            current
                .structure
                .map(i32::try_from)
                .transpose()
                .map_err(|_| "item structure overflow")?,
        );
        let oldstack = old
            .saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .map_or(1, |p| p.value.max(0) as u32);
        if oldstack != current.stack && !current.is_container {
            set(
                ints,
                5,
                Some(
                    i32::try_from(u64::from(current.stack) * u64::from(current.unit_burden))
                        .map_err(|_| "item burden overflow")?,
                ),
            );
            set(
                ints,
                19,
                Some(
                    i32::try_from(u64::from(current.stack) * u64::from(current.unit_value))
                        .map_err(|_| "item value overflow")?,
                ),
            );
        }
        if next == old.saved {
            continue;
        }
        if next.entity.mutation_revision == old.saved.entity.mutation_revision {
            return Err("changed item without dirty revision".into());
        }
        out.push(SaveSnapshot {
            object_id: current.id.0,
            mutation_revision: current.revision,
            expected_version: old.version,
            bytes: ItemSaveV5 {
                previous: next,
                source_destination: old.source_destination,
            }
            .encode()
            .map_err(|e| e.to_string())?,
        });
    }
    Ok(out)
}
fn set(values: &mut Vec<bace_content::Property<i32>>, id: u32, value: Option<i32>) {
    if let Some(value) = value {
        if let Some(p) = values.iter_mut().find(|p| p.id == id) {
            p.value = value;
        } else {
            values.push(bace_content::Property { id, value });
            values.sort_by_key(|p| p.id);
        }
    } else {
        values.retain(|p| p.id != id);
    }
}
