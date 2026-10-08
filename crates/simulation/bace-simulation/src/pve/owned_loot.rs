//! Freeze accepted creature inventory at the death boundary, retaining the
//! immutable source properties while overlaying the inventory owner's values.
use super::*;
use bace_content::{Property, WeenieV1};
use bace_inventory::ItemPlace;
impl Population {
    pub(crate) fn refresh_owned_loot(
        &mut self,
        actor: EntityId,
        inventory: &crate::inventory::Inventory,
    ) -> Result<(), PveError> {
        let Some(context) = self.native.aces.get_mut(&actor) else {
            return Ok(());
        };
        if self.pending.values().any(|p| p.proposal.victim == actor) {
            return Ok(());
        }
        let original_ids: std::collections::BTreeSet<_> =
            context.originals.iter().map(|(id, _)| *id).collect();
        let mut selected = Vec::new();
        for (order, (id, source)) in context.originals.iter().enumerate() {
            let Some(item) = inventory.item(*id) else {
                continue;
            };
            let ItemPlace::Contained {
                container: parent, ..
            } = item.place
            else {
                continue;
            };
            let mut current = parent;
            let mut depth = 0;
            let mut retained = true;
            while current != actor {
                if depth >= 64 || current == *id {
                    return Err(PveError::InvalidProfile);
                }
                if !original_ids.contains(&current) {
                    retained = false;
                    break;
                }
                let Some(ancestor) = inventory.item(current) else {
                    retained = false;
                    break;
                };
                let ItemPlace::Contained { container, .. } = ancestor.place else {
                    retained = false;
                    break;
                };
                current = container;
                depth += 1;
            }
            if !retained {
                continue;
            }
            let mut source = source.clone();
            if source.properties.ints.iter().any(|p| p.id == 12) {
                set(&mut source, 12, item.stack)?;
            }
            if let Some(structure) = item.structure {
                set(&mut source, 92, structure)?;
            } else {
                source.properties.ints.retain(|p| p.id != 92);
            }
            for (property, value) in [(13, item.unit_burden), (15, item.unit_value)] {
                if source.properties.ints.iter().any(|p| p.id == property) {
                    set(&mut source, property, value)?;
                }
            }
            if source.properties.ints.iter().any(|p| p.id == 5) || item.unit_burden != 0 {
                set(
                    &mut source,
                    5,
                    item.unit_burden
                        .checked_mul(item.stack)
                        .ok_or(PveError::Overflow)?,
                )?;
            }
            if source.properties.ints.iter().any(|p| p.id == 19) || item.unit_value != 0 {
                set(
                    &mut source,
                    19,
                    item.unit_value
                        .checked_mul(item.stack)
                        .ok_or(PveError::Overflow)?,
                )?;
            }
            selected.push((depth, order, *id, parent, source));
        }
        selected.sort_by_key(|(depth, order, _, _, _)| (*depth, *order));
        let indexes: BTreeMap<_, _> = selected
            .iter()
            .enumerate()
            .map(|(index, (_, _, id, _, _))| (*id, index))
            .collect();
        let mut items = Vec::with_capacity(selected.len());
        let mut ids = Vec::with_capacity(selected.len());
        let mut parents = Vec::with_capacity(selected.len());
        for (_, _, id, parent, source) in selected {
            let parent = if parent == actor {
                None
            } else {
                Some(*indexes.get(&parent).ok_or(PveError::InvalidProfile)?)
            };
            ids.push(id);
            parents.push(parent);
            items.push(source);
        }
        context.policy.initial_ids = ids;
        context.policy.initial_items = items;
        context.policy.initial_parents = parents;
        Ok(())
    }
}
fn set(source: &mut WeenieV1, id: u32, value: u32) -> Result<(), PveError> {
    let value = i32::try_from(value).map_err(|_| PveError::Overflow)?;
    if let Some(p) = source.properties.ints.iter_mut().find(|p| p.id == id) {
        p.value = value;
    } else {
        source.properties.ints.push(Property { id, value });
        source.properties.ints.sort_by_key(|p| p.id);
    }
    Ok(())
}

impl Population {
    /// Read-only source death-drop membership for an exact NPC continuation.
    pub(crate) fn npc_loot_initial_ids(&self, actor: EntityId) -> Option<&[EntityId]> {
        self.native
            .aces
            .get(&actor)
            .map(|context| context.policy.initial_ids.as_slice())
    }
}
