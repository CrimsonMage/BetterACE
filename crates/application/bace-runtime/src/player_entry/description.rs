//! Source: ACE GameEventPlayerDescription.WriteEventBody and SendOnLoginProperties
//! at 47edade3. Property visibility is verified by original-source reflection.
use super::visibility;
use bace_gameplay_api::EnchantmentProjection;
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, PlayerSaveV6};
use bace_wire::{
    CharacterTitle, ContainerEntry, LoginAttribute, LoginEquipment, LoginSkill, LoginVital,
    PlayerDescription, WirePosition,
};
use std::collections::BTreeSet;

pub struct EntryInventoryItem<'a> {
    pub entity: &'a EntitySaveV1,
    pub placement: &'a ItemPlacementV2,
}
/// `plussed` is supplied by the authoritative staff/cloak owner, never inferred
/// from a name or client flags. Equipment order is the accepted caller order.
pub fn prepare_player_description(
    saved: &PlayerSaveV6,
    items: &[EntryInventoryItem<'_>],
    enchantments: &[EnchantmentProjection],
    plussed: bool,
) -> Result<PlayerDescription, String> {
    saved.validate().map_err(|e| e.to_string())?;
    if items.len() > 1023 {
        return Err("entry inventory capacity".into());
    }
    let p = &saved.player.entity.state.properties;
    let mut result = PlayerDescription {
        weenie_type: saved.player.entity.state.weenie_type,
        ..Default::default()
    };
    result.integers = select(&p.ints, visibility::INT);
    result.integers64 = select(&p.int64s, visibility::INT64);
    result.booleans = select(&p.bools, visibility::BOOL);
    result.doubles = select(&p.floats, visibility::FLOAT);
    result.strings = select(&p.strings, visibility::STRING);
    result.data_ids = select(&p.data_ids, visibility::DATAID);
    result.instance_ids = select(&p.instance_ids, visibility::INSTANCEID);
    if plussed {
        for (id, name) in &mut result.strings {
            if *id == 1 {
                *name = format!("+{name}");
            }
        }
    }
    result.last_outside_death = p
        .positions
        .iter()
        .find(|p| p.id == 14)
        .map(|p| WirePosition {
            cell: p.value.obj_cell_id,
            origin: [p.value.position_x, p.value.position_y, p.value.position_z],
            rotation: [
                p.value.rotation_w,
                p.value.rotation_x,
                p.value.rotation_y,
                p.value.rotation_z,
            ],
        });
    for (slot, id) in [1, 2, 4, 3, 5, 6].into_iter().enumerate() {
        let a = &p
            .attributes
            .iter()
            .find(|a| a.id == id)
            .ok_or("entry attribute missing")?
            .value;
        result.attributes[slot] = LoginAttribute {
            ranks: a.level_from_cp,
            starting: a.init_level,
            experience: a.cp_spent,
        };
    }
    for (slot, id) in [1, 3, 5].into_iter().enumerate() {
        let v = &p
            .secondary_attributes
            .iter()
            .find(|a| a.id == id)
            .ok_or("entry vital missing")?
            .value;
        result.vitals[slot] = LoginVital {
            attribute: LoginAttribute {
                ranks: v.level_from_cp,
                starting: v.init_level,
                experience: v.cp_spent,
            },
            current: v.current_level,
        };
    }
    result.skills = p
        .skills
        .iter()
        .map(|s| {
            Ok(LoginSkill {
                id: u32::try_from(s.id).map_err(|_| "invalid entry skill")?,
                ranks: s.value.level_from_pp,
                advancement: s.value.sac,
                experience: s.value.pp,
                initial_level: s.value.init_level,
            })
        })
        .collect::<Result<_, String>>()?;
    result.known_spells = p.spell_book.iter().map(|s| s.id).collect();
    if !enchantments.is_empty() {
        result.enchantments = Some(
            bace_replication::project_enchantments(enchantments, 4096)
                .map_err(|e| format!("entry registry: {e:?}"))?,
        );
        result.has_enchantments = true;
    }
    bace_replication::project_ui(
        &crate::ui_saves::restore_ui(saved).map_err(|e| e.to_string())?,
        &mut result,
    );
    let actor = saved.player.entity.object_id;
    let mut ids = BTreeSet::new();
    let mut slots = BTreeSet::new();
    let mut top = Vec::new();
    for item in items {
        item.entity.validate().map_err(|e| e.to_string())?;
        if !ids.insert(item.entity.object_id) || item.entity.object_id == actor {
            return Err("entry duplicate inventory identity".into());
        }
        item.placement
            .validate(item.entity.object_id)
            .map_err(|e| e.to_string())?;
        let ItemPlacementV2::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        } = item.placement
        else {
            return Err("entry item is not contained".into());
        };
        if *equipped != 0 {
            if *container != actor {
                return Err("entry nested equipment".into());
            }
            let priority = item
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|p| p.id == 4)
                .map_or(0, |p| p.value as u32);
            result.equipment.push(LoginEquipment {
                object_id: item.entity.object_id,
                location: *equipped,
                priority,
            });
        } else {
            if !slots.insert((*container, *pack_slot, *slot)) {
                return Err("entry duplicate inventory slot".into());
            }
            if *container == actor {
                top.push((
                    *pack_slot,
                    *slot,
                    ContainerEntry {
                        object_id: item.entity.object_id,
                        container_type: if !pack_slot {
                            0
                        } else if item.entity.state.weenie_type == 21 {
                            1
                        } else {
                            2
                        },
                    },
                ));
            }
        }
    }
    // Every nested object must have a bounded ancestry terminating at this player.
    for item in items {
        let mut parent = item.placement;
        let mut reached = false;
        for _ in 0..=64 {
            let ItemPlacementV2::Contained { container, .. } = parent else {
                break;
            };
            if *container == actor {
                reached = true;
                break;
            }
            parent = items
                .iter()
                .find(|i| i.entity.object_id == *container)
                .ok_or("entry missing inventory parent")?
                .placement;
        }
        if !reached {
            return Err("entry inventory cycle/depth".into());
        }
    }
    top.sort_by_key(|(pack, slot, _)| (*pack, *slot));
    result.inventory = top.into_iter().map(|(_, _, item)| item).collect();
    Ok(result)
}
pub fn prepare_titles(saved: &PlayerSaveV6) -> Result<CharacterTitle, String> {
    saved.validate().map_err(|e| e.to_string())?;
    let current = saved
        .player
        .entity
        .state
        .properties
        .ints
        .iter()
        .find(|p| p.id == 261)
        .map_or(0, |p| p.value as u32);
    Ok(CharacterTitle {
        current,
        titles: saved.player.metadata.titles.clone(),
    })
}

fn select<T: Clone>(properties: &[bace_content::Property<T>], visible: &[u32]) -> Vec<(u16, T)> {
    properties
        .iter()
        .filter(|p| visible.binary_search(&p.id).is_ok())
        .map(|p| (p.id as u16, p.value.clone()))
        .collect()
}
