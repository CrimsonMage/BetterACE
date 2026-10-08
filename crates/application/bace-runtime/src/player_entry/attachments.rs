//! ACE Creature_Equipment.IsInChildLocation/GetPlacementLocation/TrySetChild.
use super::object::Properties;
use bace_content::WeenieV1;
use bace_wire::{PhysicsChild, PhysicsParent};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryItemAttachment {
    pub item: u32,
    pub parent: Option<PhysicsParent>,
    pub placement: u32,
}
/// The equipment list and combat mode must be the accepted owner state, in its
/// retained order. Returned placement replaces stale saved attachment fields.
pub fn prepare_entry_attachments(
    actor: u32,
    equipment: &[(u32, &WeenieV1, u32)],
    missile_combat: bool,
) -> Result<(Vec<PhysicsChild>, Vec<EntryItemAttachment>), String> {
    if actor == 0 || equipment.len() > 128 {
        return Err("entry attachment identity/capacity".into());
    }
    let missile_launcher = equipment
        .iter()
        .any(|(_, item, loc)| *loc == 0x00400000 && item.weenie_type == 3);
    let mut children = vec![];
    let mut items = vec![];
    for (id, item, loc) in equipment {
        if *id == 0 || *id == actor || items.iter().any(|v: &EntryItemAttachment| v.item == *id) {
            return Err("duplicate entry attachment identity".into());
        }
        let child =
            loc & 0x03700000 != 0 || (loc & 0x00800000 != 0 && missile_combat && missile_launcher);
        let (placement, location) = if child {
            match *loc {
                0x00100000 | 0x01000000 | 0x02000000 => (1, 1),
                0x00200000 => {
                    if Properties(item).int("ItemType") == Some(2) {
                        (6, 3)
                    } else {
                        (2, 8)
                    }
                }
                0x00400000 => {
                    if matches!(Properties(item).int("DefaultCombatStyle"), Some(16 | 32)) {
                        (3, 2)
                    } else {
                        (1, 1)
                    }
                }
                _ => (0, 0),
            }
        } else {
            (101, 0)
        };
        let parent = child.then_some(PhysicsParent {
            object_id: actor,
            location,
        });
        if child {
            children.push(PhysicsChild {
                object_id: *id,
                location,
            });
        }
        items.push(EntryItemAttachment {
            item: *id,
            parent,
            placement,
        });
    }
    Ok((children, items))
}
