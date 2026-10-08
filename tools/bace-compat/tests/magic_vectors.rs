use bace_wire::{Enchantment, EnchantmentRegistry, MagicEvent};
use serde_json::Value;
fn fixture() -> Value {
    let f: Value = serde_json::from_str(include_str!("../fixtures/magic.json")).unwrap();
    assert_eq!(f["commit"], bace_compat::ACE_COMMIT);
    f["vectors"].clone()
}
fn bytes(v: &Value) -> Vec<u8> {
    v.as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
fn entry(spell: u16, layer: u16, set: u16) -> Enchantment {
    Enchantment {
        spell_id: spell,
        layer,
        category: 37,
        has_spell_set_id: set,
        power: 250,
        start_time: -15.0,
        duration: 120.5,
        caster_id: 0x50000001,
        degrade_modifier: 0.25,
        degrade_limit: -666.0,
        last_time_degraded: 0.0,
        stat_type: 0x02004008,
        stat_key: 64,
        stat_value: 1.125,
        spell_set_id: (set != 0).then_some(17),
    }
}
#[test]
fn official_enchantment_fields_optional_set_and_registry_category_order() {
    let f = fixture();
    let entries = [
        entry(100, 4, 1),
        entry(666, 99, 1),
        entry(101, 1, 0),
        entry(102, 2, 2),
    ];
    for (actual, expected) in entries.iter().zip(f["enchantments"].as_array().unwrap()) {
        assert_eq!(actual.encode().unwrap(), bytes(expected));
    }
    for row in f["registries"].as_array().unwrap() {
        let mask = row["mask"].as_u64().unwrap();
        let r = EnchantmentRegistry {
            multiplicative: if mask & 1 != 0 {
                vec![entries[0]]
            } else {
                vec![]
            },
            additive: if mask & 2 != 0 {
                vec![entries[2]]
            } else {
                vec![]
            },
            cooldown: if mask & 8 != 0 {
                vec![entries[3]]
            } else {
                vec![]
            },
            vitae: (mask & 4 != 0).then_some(entries[1]),
        };
        assert_eq!(r.encode(32, 4096).unwrap(), bytes(&row["bytes"]));
    }
    let layered = [(100, 1), (65000, 65535)];
    let events = [
        MagicEvent::UpdateSpell {
            spell: 100,
            layer: 2,
        },
        MagicEvent::UpdateEnchantment(&entries[0]),
        MagicEvent::Remove {
            spell: 100,
            layer: 2,
        },
        MagicEvent::UpdateMultiple(&entries),
        MagicEvent::RemoveMultiple(&layered),
        MagicEvent::Purge,
        MagicEvent::Dispel {
            spell: 65000,
            layer: 65535,
        },
        MagicEvent::DispelMultiple(&layered),
        MagicEvent::PurgeBad,
    ];
    for (event, expected) in events.iter().zip(f["events"].as_array().unwrap()) {
        assert_eq!(
            event.encode(0x50000001, 17, 32, 4096).unwrap(),
            bytes(&expected["bytes"]),
            "{}",
            expected["name"]
        );
    }
}
#[test]
fn official_magic_chance_and_mana_conversion_preserve_random_consumption_and_precision() {
    let f = fixture();
    for row in f["mana"].as_array().unwrap() {
        let skill = row["skill"].as_u64().unwrap() as u32;
        let difficulty = row["difficulty"].as_u64().unwrap() as u32;
        let cost = row["cost"].as_u64().unwrap() as u32;
        let draw = row["draw"].as_f64().unwrap() as f32;
        let actual = bace_magic::mana_cost(difficulty, cost, skill, &[draw, 0.25, 0.75]).unwrap();
        assert_eq!(
            actual,
            (
                row["value"].as_u64().unwrap() as u32,
                row["consumed"].as_u64().unwrap() as usize
            ),
            "{row}"
        );
    }
    for row in f["chance"].as_array().unwrap() {
        let actual = bace_magic::cast_chance(
            row["skill"].as_u64().unwrap() as u32,
            row["difficulty"].as_u64().unwrap() as u32,
        )
        .unwrap();
        assert!(
            (actual - row["value"].as_f64().unwrap()).abs() < 1e-14,
            "{row}"
        );
    }
}
#[test]
fn player_description_inserts_independent_registry_vector_at_the_source_position() {
    use bace_wire::{LoginAttribute, LoginVital, PlayerDescription, PlayerDescriptionLimits};
    let old: Value = serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap();
    let mut expected = bytes(&old["vectors"]["player_description"]["fresh"]["bytes"]);
    // These offsets are independently witnessed by the pinned fresh description,
    // not inferred from the Rust serializer under test.
    assert_eq!(&expected[24..28], &3_u32.to_le_bytes());
    assert_eq!(&expected[160..164], &0x460_u32.to_le_bytes());
    expected[24..28].copy_from_slice(&0x203_u32.to_le_bytes());
    let registry_bytes = bytes(&fixture()["registries"][1]["bytes"]);
    expected.splice(160..160, registry_bytes);
    let a = LoginAttribute {
        ranks: 3,
        starting: 4,
        experience: 5,
    };
    let mut p = PlayerDescription {
        weenie_type: 10,
        attributes: [a; 6],
        vitals: [LoginVital {
            attribute: a,
            current: 6,
        }; 3],
        has_enchantments: true,
        enchantments: Some(EnchantmentRegistry {
            multiplicative: vec![entry(100, 4, 1)],
            ..Default::default()
        }),
        ..Default::default()
    };
    for (i, a) in p.attributes.iter_mut().enumerate() {
        a.starting = (i as u32 + 1) * 10;
    }
    for (i, v) in p.vitals.iter_mut().enumerate() {
        v.current = (i as u32 + 7) * 10;
    }
    assert_eq!(
        p.encode(
            0x50000001,
            42,
            PlayerDescriptionLimits {
                max_table_entries: 64,
                max_string_bytes: 128,
                max_gameplay_options_bytes: 128,
                max_message_bytes: 8192
            }
        )
        .unwrap(),
        expected
    );
}
#[test]
fn official_component_burn_selects_occurrences_without_gdle_fizzle_multiplier() {
    for row in fixture()["burns"].as_array().unwrap() {
        let power = row["power"].as_u64().unwrap() as u32;
        let skill = row["skill"].as_u64().unwrap() as u32;
        let loss = row["loss"].as_f64().unwrap() as f32;
        let mut consumed = Vec::new();
        for (component, modifier, draw) in [(1, 0.5, 0.1), (63, 2.0, 0.5), (1, 0.5, 0.9)] {
            if draw < bace_magic::component_burn_rate(loss, modifier, power, skill).unwrap() {
                consumed.push(component);
            }
        }
        assert_eq!(
            consumed,
            row["consumed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>(),
            "{row}"
        );
    }
}
