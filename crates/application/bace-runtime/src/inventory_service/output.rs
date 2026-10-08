//! Projection starts only after exact database and simulation adoption. New
//! instances require their full prepared description; no unknown-object fallback.
use super::*;
use bace_gameplay_api::InventoryRequest;
use bace_inventory::ItemPlace;
use bace_replication::{
    BatchLimits, EventSequencer, InventoryProjection, Sequences, SessionBatch,
    SessionProjectionError,
};
use bace_types::EntityId;
use bace_wire::{InventoryEvent, ObjectCodecLimits, ObjectDescription};
use std::collections::BTreeMap;
pub fn project_inventory_completion(
    completion: &InventoryCompletion,
    sequencer: &mut EventSequencer,
    item_sequences: &mut BTreeMap<EntityId, Sequences>,
    created: &BTreeMap<EntityId, ObjectDescription>,
    objects: ObjectCodecLimits,
    limits: BatchLimits,
) -> Result<Option<SessionBatch>, SessionProjectionError> {
    if !completion.committed {
        return Ok(None);
    }
    let p = &completion.work.operation;
    let acquisition = match p.request {
        InventoryRequest::Move { item, .. } => p
            .ticket
            .proposal
            .changes
            .iter()
            .find(|c| {
                c.after.id == item
                    && c.after.is_container
                    && matches!(c.after.place, ItemPlace::Contained { .. })
                    && c.before.as_ref().is_some_and(|before| {
                        before.place == ItemPlace::World
                            || completion
                                .work
                                .external
                                .iter()
                                .any(|source| source.entity.object_id == item.0)
                    })
            })
            .map(|c| c.after.id),
        _ => None,
    };
    let mut contents = BTreeMap::new();
    let mut descendants = std::collections::BTreeSet::new();
    let mut order = Vec::new();
    if let Some(root) = acquisition {
        order.push(root);
        let mut index = 0;
        while index < order.len() {
            let parent = order[index];
            index += 1;
            let mut children = p
                .ticket
                .proposal
                .changes
                .iter()
                .filter_map(|c| match c.after.place {
                    ItemPlace::Contained {
                        container,
                        slot,
                        equipped: 0,
                    } if container == parent => Some((c.after.pack_slot, slot, &c.after)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            children.sort_by_key(|(pack, slot, _)| (*pack, *slot));
            let mut entries = Vec::new();
            for (_, _, child) in children {
                if !descendants.insert(child.id) || child.id == root || descendants.len() > 1023 {
                    return Err(SessionProjectionError::InvalidProjection);
                }
                entries.push(bace_wire::ContainerEntry {
                    object_id: child.id.0,
                    container_type: container_type(completion, child.id)?,
                });
                if child.is_container {
                    order.push(child.id);
                }
            }
            contents.insert(parent, entries);
        }
    }
    let mut steps = Vec::new();
    // ACE DoHandleActionStackableSplitToContainer: create, contain, then remainder.
    for change in p
        .ticket
        .proposal
        .changes
        .iter()
        .filter(|c| c.before.is_none() || created.contains_key(&c.after.id))
    {
        if descendants.contains(&change.after.id) {
            continue;
        }
        let object = created
            .get(&change.after.id)
            .ok_or(SessionProjectionError::InvalidProjection)?;
        if object.object_id != change.after.id.0
            || !completion
                .snapshots
                .iter()
                .any(|s| s.object_id == object.object_id)
        {
            return Err(SessionProjectionError::InvalidProjection);
        }
        steps.push(InventoryProjection::Create(object));
        if change.before.is_none() {
            steps.push(InventoryProjection::Event(placement(
                &change.after,
                container_type(completion, change.after.id)?,
            )?));
        }
    }
    if let Some(position) = p.world_placement
        && p.ticket
            .proposal
            .changes
            .iter()
            .any(|c| c.after.id == position.item && c.before.is_some())
    {
        steps.push(InventoryProjection::Container {
            item: position.item,
            value: 0,
        });
    }
    let focus = match p.request {
        InventoryRequest::Move { item, .. }
        | InventoryRequest::Drop { item }
        | InventoryRequest::Equip { item, .. } => Some(item),
        _ => None,
    };
    if let Some(id) = focus {
        let change = p
            .ticket
            .proposal
            .changes
            .iter()
            .find(|c| c.after.id == id)
            .ok_or(SessionProjectionError::InvalidProjection)?;
        steps.push(InventoryProjection::Event(placement(
            &change.after,
            container_type(completion, change.after.id)?,
        )?));
    }
    // ACE pickup publishes the container placement before ViewContents and
    // each direct child's full description. Walk nested authored bags in order.
    for parent in order {
        steps.push(InventoryProjection::Event(InventoryEvent::ViewContents {
            container_id: parent.0,
            items: &contents[&parent],
        }));
        for child in &contents[&parent] {
            if let Some(object) = created.get(&EntityId(child.object_id)) {
                steps.push(InventoryProjection::Create(object));
            }
        }
    }
    if let Some(position) = p.world_placement {
        let h = position.heading * 0.5;
        steps.push(InventoryProjection::Position {
            item: position.item,
            pack: bace_wire::PositionPack {
                position: bace_wire::WirePosition {
                    cell: position.cell.0,
                    origin: [
                        position.position.x,
                        position.position.y,
                        position.position.z,
                    ],
                    rotation: [h.cos(), 0., 0., h.sin()],
                },
                velocity: Some([
                    position.velocity.x,
                    position.velocity.y,
                    position.velocity.z,
                ]),
                placement: Some(101),
                grounded: position.grounded,
                instance_sequence: 0,
                position_sequence: 0,
                teleport_sequence: 0,
                force_position_sequence: 0,
            },
        });
    }
    for change in p
        .ticket
        .proposal
        .changes
        .iter()
        .filter(|c| c.before.is_some())
    {
        let before = change.before.as_ref().expect("filtered");
        if change.after.place == ItemPlace::Removed {
            steps.push(InventoryProjection::Remove(change.after.id));
        } else if before.stack != change.after.stack {
            let value = change
                .after
                .stack
                .checked_mul(change.after.unit_value)
                .ok_or(SessionProjectionError::InvalidProjection)?;
            steps.push(InventoryProjection::Stack {
                item: change.after.id,
                quantity: change.after.stack,
                value,
            });
        }
    }
    sequencer
        .project_inventory(
            completion.work.binding,
            &steps,
            item_sequences,
            objects,
            limits,
        )
        .map(Some)
}
fn placement(
    item: &bace_inventory::InventoryItem,
    container_type: u32,
) -> Result<InventoryEvent<'static>, SessionProjectionError> {
    Ok(match item.place {
        ItemPlace::World => InventoryEvent::PutInWorld {
            object_id: item.id.0,
        },
        ItemPlace::Contained { equipped, .. } if equipped != 0 => InventoryEvent::Wield {
            object_id: item.id.0,
            location: equipped,
        },
        ItemPlace::Contained {
            container, slot, ..
        } => InventoryEvent::PutInContainer {
            object_id: item.id.0,
            container_id: container.0,
            placement: i32::try_from(slot)
                .map_err(|_| SessionProjectionError::InvalidProjection)?,
            container_type,
        },
        ItemPlace::Removed => return Err(SessionProjectionError::InvalidProjection),
    })
}

fn container_type(
    completion: &InventoryCompletion,
    id: EntityId,
) -> Result<u32, SessionProjectionError> {
    let row = completion
        .snapshots
        .iter()
        .find(|s| s.object_id == id.0)
        .ok_or(SessionProjectionError::InvalidProjection)?;
    let item = bace_storage_codec::ItemSaveV5::decode(&row.bytes)
        .map_err(|_| SessionProjectionError::InvalidProjection)?;
    Ok(if item.entity.state.weenie_type == 21 {
        1
    } else if item
        .entity
        .state
        .properties
        .bools
        .iter()
        .any(|p| p.id == 81 && p.value)
    {
        2
    } else {
        0
    })
}
