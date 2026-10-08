use bace_wire::{
    AppraisalCreature, AppraisalLimits, AppraisalProfile, AppraisalWeapon, IdentifyObjectRequest,
};

#[test]
fn pinned_identify_action_guid_is_exact_little_endian_dword() {
    // ACE GameActionIdentifyObject reads exactly one UInt32, then Player.cs
    // treats zero as deselection rather than an appraisal request.
    assert_eq!(
        IdentifyObjectRequest::decode(&[0x21, 0x00, 0x00, 0x80]).unwrap(),
        IdentifyObjectRequest {
            target: 0x8000_0021
        }
    );
    assert_eq!(IdentifyObjectRequest::decode(&[0; 4]).unwrap().target, 0);
    assert!(IdentifyObjectRequest::decode(&[1, 2, 3]).is_err());
    assert!(IdentifyObjectRequest::decode(&[1, 2, 3, 4, 5]).is_err());
}
fn limits() -> AppraisalLimits {
    AppraisalLimits {
        table_entries: 4096,
        string_bytes: 4096,
        message_bytes: 1 << 20,
    }
}
fn fixture(flags: u32, success: bool, creature_flags: u32) -> AppraisalProfile {
    let mut p = AppraisalProfile {
        success,
        ..Default::default()
    };
    for id in [17u16, 8, 1, 16] {
        if flags & 1 != 0 {
            p.integers.push((id, -i32::from(id)));
        }
        if flags & 0x2000 != 0 {
            p.integers64.push((id, -(i64::from(id) << 33)));
        }
        if flags & 2 != 0 {
            p.booleans.push((id, id % 2 == 0));
        }
        if flags & 4 != 0 {
            p.doubles.push((id, f64::from(id) / 8.));
        }
        if flags & 8 != 0 {
            p.strings.push((id, format!("item{id}")));
        }
        if flags & 0x1000 != 0 {
            p.data_ids.push((id, 0xf0000000 + u32::from(id)));
        }
    }
    if flags & 0x10 != 0 {
        p.spells = vec![0x80001234, 99];
    }
    if flags & 0x80 != 0 {
        p.armor = Some([1., 2., 3., 4., 5., 6., 7., 8.]);
    }
    if flags & 0x100 != 0 {
        p.creature = Some(AppraisalCreature {
            health: 7,
            maximum_health: 100,
            attributes: (creature_flags & 8 != 0).then_some([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
            attribute_mask: (creature_flags & 1 != 0).then_some((3, 1)),
        });
    }
    if flags & 0x20 != 0 {
        p.weapon = Some(AppraisalWeapon {
            damage_type: 8,
            time: 25,
            skill: 44,
            damage: 90,
            variance: 0.25,
            damage_modifier: 1.5,
            length: 2.5,
            maximum_velocity: 30.,
            offense: 1.25,
            velocity_estimated: 1,
        });
    }
    if flags & 0x40 != 0 {
        p.hook = Some([3, 0x100000, 2]);
    }
    if flags & 0x200 != 0 {
        p.armor_mask = Some((3, 1));
    }
    if flags & 0x800 != 0 {
        p.weapon_mask = Some((5, 4));
    }
    if flags & 0x400 != 0 {
        p.resist_mask = Some((9, 8));
    }
    if flags & 0x4000 != 0 {
        p.armor_levels = Some([1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
    p
}
#[test]
fn original_appraiseinfo_and_all_profile_writers() {
    let mut count = 0;
    for row in include_str!("fixtures/appraisal.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = row.split('|').collect();
        let p = fixture(f[0].parse().unwrap(), f[1] == "True", f[2].parse().unwrap());
        let bytes = p.encode(1, 2, 3, limits()).unwrap();
        let actual: String = bytes[20..].iter().map(|b| format!("{b:02X}")).collect();
        assert_eq!(actual, f[3], "{row}");
        count += 1;
    }
    assert_eq!(count, 136);
}
#[test]
fn rejects_duplicate_ids_and_bounded_sections_without_partial_packet() {
    let mut p = fixture(0x7fff, true, 9);
    assert!(
        p.encode(
            1,
            2,
            3,
            AppraisalLimits {
                message_bytes: 30,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(
        p.encode(
            1,
            2,
            3,
            AppraisalLimits {
                table_entries: 3,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(
        p.encode(
            1,
            2,
            3,
            AppraisalLimits {
                string_bytes: 2,
                ..limits()
            }
        )
        .is_err()
    );
    p.integers.push(p.integers[0]);
    assert!(p.encode(1, 2, 3, limits()).is_err());
}
