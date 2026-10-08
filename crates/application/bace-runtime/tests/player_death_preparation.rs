//! Original C# CreateCorpse appearance block output; this is not death protocol
//! qualification. Physics and durable ownership have separate regression suites.
use bace_content::{Property, WeenieV1};
use bace_runtime::player_death_preparation::{CorpseAppearanceInput, prepare_corpse_source};
use bace_types::EntityId;
fn source(id: u32, kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("corpse_fixture_{id}"),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    }
}
#[test]
fn original_ace_corpse_appearance_identity_vectors() {
    let fixture = include_str!("fixtures/player_corpse.trace");
    let mut count = 0;
    for line in fixture
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let n: usize = line.split('|').next().unwrap().parse().unwrap();
        let mut template = source(100, 14);
        template.properties.data_ids = [(1, 101), (2, 102), (3, 103), (22, 122)]
            .map(|(id, value)| Property { id, value })
            .to_vec();
        let mut player = source(1, 10);
        player.properties.strings.push(Property {
            id: 1,
            value: "Alice".into(),
        });
        player.properties.data_ids = [(1, 11), (2, 12), (3, 13), (6, 6), (7, 7), (22, 22)]
            .map(|(id, value)| Property { id, value })
            .to_vec();
        if !n.is_multiple_of(2) {
            player.properties.floats = vec![
                Property { id: 39, value: 1.5 },
                Property {
                    id: 12,
                    value: 0.25,
                },
            ];
            player.properties.ints.push(Property { id: 3, value: 9 });
        }
        player.properties.floats.push(Property {
            id: 11,
            value: 0.75,
        }); // source translucency does not carry
        if n >= 6 {
            player.properties.bools.push(Property {
                id: 120,
                value: true,
            });
        }
        if n % 6 == 5 {
            player.properties.instance_ids.push(Property {
                id: 6,
                value: 0x50000006,
            });
        }
        let killer = match n % 6 {
            0 => None,
            1 => Some((EntityId(0x50000002), "++Killer")),
            2 => Some((EntityId(0x50000001), "Alice")),
            3 => Some((EntityId(0x50000003), "  ")),
            4 => Some((EntityId(0x50000005), "Pet")),
            _ => Some((EntityId(0x50000006), "Generator")),
        };
        let model = bace_wire::ObjectModel {
            palette_id: Some(6),
            parts: vec![bace_wire::ModelPart {
                part_index: 0,
                animation_id: 17,
            }],
            palettes: vec![bace_wire::ModelPalette {
                palette_id: 18,
                offset: 0,
                length: 1,
            }],
            textures: vec![bace_wire::ModelTexture {
                part_index: 0,
                old_texture: 19,
                new_texture: 20,
            }],
        };
        let result = prepare_corpse_source(CorpseAppearanceInput {
            template: &template,
            player: &player,
            actor: EntityId(0x50000001),
            position: bace_content::Position {
                obj_cell_id: 0x01010001,
                rotation_w: 1.,
                ..Default::default()
            },
            model: &model,
            killer,
        })
        .unwrap();
        let did = |id| {
            result
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value.to_string())
                .unwrap_or("-".into())
        };
        let int = |id| {
            result
                .properties
                .ints
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value.to_string())
                .unwrap_or("-".into())
        };
        let float = |id| {
            result
                .properties
                .floats
                .iter()
                .find(|p| p.id == id)
                .map(|p| (p.value as f32).to_string())
                .unwrap_or("-".into())
        };
        let iid = |id| {
            result
                .properties
                .instance_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value.to_string())
                .unwrap_or("-".into())
        };
        let text = |id| {
            result
                .properties
                .strings
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .value
                .clone()
        };
        let actual = vec![
            n.to_string(),
            text(1),
            text(16),
            did(1),
            did(2),
            did(3),
            did(6),
            did(7),
            did(22),
            float(39),
            int(3),
            float(12),
            iid(18),
            iid(19),
            result.properties.animation_parts.len().to_string(),
            result.properties.palettes.len().to_string(),
            result.properties.texture_maps.len().to_string(),
        ]
        .join("|");
        assert_eq!(actual, line, "original source case {n}");
        assert_eq!(float(11), "-");
        assert_eq!(int(6), "120");
        assert_eq!(int(7), "10");
        count += 1;
    }
    assert_eq!(count, 12);
}
#[test]
fn no_corpse_cannot_silently_use_the_corpse_checkpoint() {
    let template = source(1, 14);
    let mut player = source(2, 10);
    player.properties.strings.push(Property {
        id: 1,
        value: "Alice".into(),
    });
    player.properties.bools.push(Property {
        id: 29,
        value: true,
    });
    let result = prepare_corpse_source(CorpseAppearanceInput {
        template: &template,
        player: &player,
        actor: EntityId(0x50000001),
        position: Default::default(),
        model: &Default::default(),
        killer: None,
    });
    assert!(result.unwrap_err().contains("NoCorpse"));
}

#[test]
fn original_ace_corpse_decay_and_final_metadata_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/player_corpse_metadata.trace")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let columns: Vec<_> = line.split('|').collect();
        let level: u32 = columns[0].parse().unwrap();
        let empty = columns[1] == "True";
        let pk = columns[2] == "True";
        let mut corpse = source(100, 14);
        corpse.properties.ints.push(Property { id: 19, value: 100 });
        let kind = if pk {
            bace_interactions::PlayerDeathKind::Pk
        } else {
            bace_interactions::PlayerDeathKind::Ordinary
        };
        bace_runtime::player_death_preparation::freeze_corpse_metadata(
            &mut corpse,
            level,
            bace_interactions::player_corpse_decay_seconds(level, empty),
            1728000000,
            kind,
        )
        .unwrap();
        let int = |id, default| {
            corpse
                .properties
                .ints
                .iter()
                .find(|p| p.id == id)
                .map_or(default, |p| p.value)
        };
        let decay = corpse
            .properties
            .floats
            .iter()
            .find(|p| p.id == 44)
            .unwrap()
            .value;
        assert_eq!(
            format!(
                "{}|{}|{}|{}|{}|{}|{}|-",
                level,
                columns[1],
                columns[2],
                int(25, 0),
                decay,
                int(98, 0),
                int(99, 0)
            ),
            line
        );
        assert!(!corpse.properties.ints.iter().any(|p| p.id == 19));
        count += 1;
    }
    assert_eq!(count, 20);
    let mut corpse = source(100, 14);
    let before = corpse.clone();
    assert!(
        bace_runtime::player_death_preparation::freeze_corpse_metadata(
            &mut corpse,
            1,
            15,
            i64::from(i32::MAX) + 1,
            bace_interactions::PlayerDeathKind::Ordinary
        )
        .is_err()
    );
    assert_eq!(corpse, before);
}

#[test]
fn last_outside_death_is_saved_by_its_single_metadata_owner() {
    use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
    let mut player = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state: source(1, 10),
        },
        account_id: 1,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    let mut state = bace_runtime::player_death_state::restore_player_death_state(&player).unwrap();
    let position = bace_interactions::PortalPosition {
        cell: 0x12340001,
        origin: [1., 2., 3.],
        rotation: [1., 0., 0., 0.],
    };
    state.last_outside_death = Some(position);
    bace_runtime::player_death_state::freeze_player_death_state(&mut player, &state).unwrap();
    assert_eq!(
        bace_runtime::player_death_state::restore_player_death_state(&player).unwrap(),
        state
    );
    assert_eq!(
        player
            .player
            .entity
            .state
            .properties
            .positions
            .iter()
            .filter(|p| p.id == 14)
            .count(),
        1
    );
    let before = player.clone();
    state.last_outside_death.as_mut().unwrap().rotation = [0.; 4];
    assert!(
        bace_runtime::player_death_state::freeze_player_death_state(&mut player, &state).is_err()
    );
    assert_eq!(player, before);
}
