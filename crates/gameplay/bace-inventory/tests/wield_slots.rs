use bace_inventory::*;
use bace_types::EntityId;
fn item(kind: u32, id: u32, location: u32) -> WieldSlotItem {
    let mut w = WieldSlotItem {
        item: EntityId(id),
        revision: 1,
        location,
        valid: location,
        clothing: matches!(kind, 8 | 9),
        coverage: None,
        item_type: 1,
        style: Some(2),
        weapon_skill: None,
        ammo: None,
        combat_use: None,
    };
    match kind {
        1 => w.weapon_skill = Some(41),
        2 => w.style = Some(0x200),
        3 => {
            w.style = Some(16);
            w.ammo = Some(1);
        }
        4 => {
            w.style = Some(0x400);
            w.ammo = Some(2);
        }
        5 => {
            w.style = Some(0x80);
            w.ammo = Some(3);
        }
        6 => {
            w.item_type = 2;
            w.combat_use = Some(4);
        }
        7 => w.style = None,
        8 => {
            w.coverage = Some(3);
            w.item_type = 4;
        }
        9 => {
            w.coverage = Some(4);
            w.item_type = 2;
        }
        10 => w.ammo = Some(1),
        _ => {}
    }
    w
}
fn gear(kind: u32) -> Vec<WieldSlotItem> {
    let (k, l) = match kind {
        0 => return vec![],
        1 => (0, 0x100000),
        2 => (1, 0x2000000),
        3 => (2, 0x1000000),
        4 => (3, 0x400000),
        5 => (6, 0x200000),
        6 => (0, 0x200000),
        7 => (10, 0x800000),
        8 => (4, 0x800000),
        9 => (8, 3),
        10 => (9, 3),
        11 => return vec![item(0, 1, 0x100000), item(6, 2, 0x200000)],
        _ => panic!("fixture kind"),
    };
    vec![item(k, 1, l)]
}
#[test]
fn compiled_original_weapon_collision_and_slot_occupancy() {
    let mut count = 0;
    for line in include_str!("fixtures/wield_slots.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let v = line
            .split('|')
            .map(|v| v.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        let w = item(v[0], 99, v[2]);
        let gear = gear(v[1]);
        assert_eq!(
            check_weapon_collision(&w, v[2], &gear, v[3]),
            v[4] != 0,
            "{line}"
        );
        assert_eq!(wield_slot_available(&w, v[2], &gear), v[5] == 0, "{line}");
        count += 1;
    }
    assert_eq!(count, 2112);
}
#[test]
fn source_normalization_unlocks_and_no_auto_dequip() {
    let mut w = item(8, 99, 0);
    w.valid = 3;
    assert_eq!(check_wield_slots(&w, 1, &[], 1, 0), Ok(3));
    assert_eq!(
        check_wield_slots(&w, 1, &gear(9), 1, 0),
        Err(WieldSlotError::Conflict)
    );
    w = item(0, 99, 0);
    w.valid = 0x70000000;
    assert_eq!(
        check_wield_slots(&w, 0x20000000, &[], 1, 1),
        Err(WieldSlotError::Locked)
    );
    assert_eq!(check_wield_slots(&w, 0x20000000, &[], 1, 2), Ok(0x20000000));
    w.valid = 0x100000;
    assert_eq!(check_wield_slots(&w, 0x200000, &[], 1, 0), Ok(0x200000));
    assert_eq!(
        check_wield_slots(&w, 0x200000, &gear(2), 1, 0),
        Err(WieldSlotError::Conflict)
    );
    // Source includes the item itself in occupancy; an identical location is rejected.
    let w = item(0, 99, 0x100000);
    assert_eq!(
        check_wield_slots(&w, 0x100000, std::slice::from_ref(&w), 1, 0),
        Err(WieldSlotError::Conflict)
    );
    assert_eq!(
        check_wield_slots(&w, 0x200000, std::slice::from_ref(&w), 1, 0),
        Ok(0x200000)
    );
}
