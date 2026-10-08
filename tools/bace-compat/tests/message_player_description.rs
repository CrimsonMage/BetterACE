//! Original ACE PlayerDescription with synthetic properties, traits and possessions.
use bace_wire::*;
use serde_json::Value;
fn fixtures() -> Value {
    serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap()
}
fn bytes(v: &Value) -> Vec<u8> {
    let s = v["bytes"].as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
fn description(name: &str) -> PlayerDescription {
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
        ..Default::default()
    };
    for (i, a) in p.attributes.iter_mut().enumerate() {
        a.starting = (i as u32 + 1) * 10;
    }
    for (i, v) in p.vitals.iter_mut().enumerate() {
        v.current = (i as u32 + 7) * 10;
    }
    if matches!(name, "properties" | "all" | "plussed" | "cloaked") {
        p.integers = vec![(65, -7), (1, 42), (64, 0)];
        p.integers64 = vec![(1, -1234567890123)];
        p.booleans = vec![(33, true), (1, false)];
        p.doubles = vec![(2, 0.35)];
        p.strings = vec![(
            1,
            if name == "plussed" {
                "+Élodie"
            } else {
                "Élodie"
            }
            .into(),
        )];
        p.data_ids = vec![(1, 0x02000001)];
        p.instance_ids = vec![(2, 0x50000002)];
    }
    if matches!(name, "position" | "all") {
        p.last_outside_death = Some(WirePosition {
            cell: 0x12340001,
            origin: [1.25, -2.5, 3.75],
            rotation: [0.1, 0.2, 0.3, 0.4],
        });
    }
    if matches!(name, "skills_spells" | "all") {
        p.skills = [33, 1, 32]
            .map(|id| LoginSkill {
                id,
                ranks: (id + 1) as u16,
                advancement: 2,
                experience: 900,
                initial_level: 5,
            })
            .into();
        p.known_spells = vec![65, 1, 64];
    }
    if matches!(name, "options" | "all") {
        p.options1 = 0x12345678;
        p.options2 = 0x87654321;
        p.spellbook_filters = 0x11223344;
        p.shortcuts = vec![LoginShortcut {
            index: 2,
            object_id: 0x80000001,
            spell_id: 7,
            layer: 3,
        }];
        p.spell_bars[0] = vec![9];
        p.spell_bars[7] = vec![10];
        p.desired_components = vec![(257, 258), (1, 2), (256, 257)];
        p.gameplay_options = vec![1, 2, 3, 4, 5, 6, 7, 8];
    }
    if matches!(name, "inventory" | "all") {
        p.inventory = vec![
            ContainerEntry {
                object_id: 0x80000002,
                container_type: 0,
            },
            ContainerEntry {
                object_id: 0x80000001,
                container_type: 0,
            },
            ContainerEntry {
                object_id: 0x80000004,
                container_type: 2,
            },
            ContainerEntry {
                object_id: 0x80000003,
                container_type: 1,
            },
        ];
        p.equipment = vec![LoginEquipment {
            object_id: 0x80000005,
            location: 4,
            priority: 8,
        }];
    }
    p
}
#[test]
fn complete_fresh_player_and_each_optional_family_match_official_serializer() {
    let f = fixtures();
    for (name, v) in f["vectors"]["player_description"].as_object().unwrap() {
        if name == "titles" {
            assert_eq!(
                CharacterTitle {
                    current: 7,
                    titles: vec![7, 9]
                }
                .encode(0x50000001, 42, 10, 128)
                .unwrap(),
                bytes(v)
            );
            continue;
        }
        let actual = description(name)
            .encode(
                0x50000001,
                42,
                PlayerDescriptionLimits {
                    max_table_entries: 256,
                    max_string_bytes: 256,
                    max_gameplay_options_bytes: 1024,
                    max_message_bytes: 16384,
                },
            )
            .unwrap();
        assert_eq!(actual, bytes(v), "{name}");
        assert_eq!(v["group"], 9);
    }
}
#[test]
fn character_lifecycle_prefixes_preserve_utf16_lengths_and_ignored_suffixes() {
    for v in fixtures()["vectors"]["character_lifecycle"]
        .as_array()
        .unwrap()
    {
        let data = bytes(v);
        let request = CharacterLifecycleRequest::decode(&data, 256, 64).unwrap();
        let captured = match request.action {
            CharacterLifecycleAction::EnterWorld {
                character_id,
                account,
            } => serde_json::json!({"character_id":character_id,"account":account}),
            CharacterLifecycleAction::Delete { account, slot } => {
                serde_json::json!({"account":account,"slot":slot})
            }
            CharacterLifecycleAction::Restore { character_id } => {
                serde_json::json!({"character_id":character_id})
            }
            _ => panic!("unexpected lifecycle fixture"),
        };
        assert_eq!(captured, v["captured"]);
        assert_eq!(
            request.trailing_bytes,
            v["trailing_bytes"].as_u64().unwrap() as usize
        );
        for len in 0..data.len() - request.trailing_bytes {
            assert!(CharacterLifecycleRequest::decode(&data[..len], 256, 64).is_err());
        }
    }
}
#[test]
fn world_control_readers_match_original_handlers_without_granting_entry() {
    let f = fixtures();
    let vectors = &f["vectors"]["world_control"];
    for (name, action, sequence) in [
        (
            "login_complete",
            WorldControlAction::LoginComplete,
            Some(42),
        ),
        (
            "force_description",
            WorldControlAction::ForceObjectDescription(0x80000001),
            None,
        ),
    ] {
        let v = &vectors[name];
        let data = bytes(v);
        let request = WorldControlRequest::decode(&data, 256).unwrap();
        assert_eq!(request.action, action);
        assert_eq!(request.action_sequence, sequence);
        assert_eq!(
            request.trailing_bytes,
            v["trailing_bytes"].as_u64().unwrap() as usize
        );
        for n in 0..data.len() - request.trailing_bytes {
            assert!(WorldControlRequest::decode(&data[..n], 256).is_err());
        }
    }
}
