use bace_content::{Attribute, Property, SecondaryAttribute, WeenieV1};
use bace_runtime::player_entry::*;
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, PlayerSaveV1, PlayerSaveV6};
use bace_wire::{FriendsUpdate, FriendsUpdateKind, PhysicsSequences, WirePosition};
use std::collections::BTreeMap;
mod treasure_table_support;
const ACTOR: u32 = 0x50000001;
fn source(kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "entry".into(),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    }
}
fn player() -> PlayerSaveV6 {
    let mut state = source(10);
    state.properties.attributes = (1..=6)
        .map(|id| Property {
            id,
            value: Attribute {
                init_level: 10,
                ..Default::default()
            },
        })
        .collect();
    state.properties.secondary_attributes = [1, 3, 5]
        .map(|id| Property {
            id,
            value: SecondaryAttribute {
                current_level: 10,
                ..Default::default()
            },
        })
        .to_vec();
    state.properties.data_ids.push(Property {
        id: 1,
        value: 0x02000001,
    });
    state.properties.ints.push(Property { id: 65, value: 999 });
    state.properties.strings.push(Property {
        id: 1,
        value: "Rune".into(),
    });
    let mut p = PlayerSaveV6::decode_or_migrate(
        &PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: ACTOR,
                template_revision: 1,
                mutation_revision: 1,
                state,
            },
            account_id: 1,
            name: "Rune".into(),
            metadata: Default::default(),
            quests: vec![],
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    p.ui.spellbook_filters = 0;
    p
}
fn sequences() -> PhysicsSequences {
    PhysicsSequences {
        position: 1,
        movement: 2,
        state: 3,
        vector: 4,
        teleport: 5,
        server_control: 6,
        force_position: 7,
        visual_description: 8,
        instance: 9,
    }
}
fn live() -> EntryObjectState {
    EntryObjectState {
        is_player: true,
        is_creature: true,
        physics_state: 0x404410,
        position: Some(WirePosition {
            cell: 0x12340001,
            origin: [1., 2., 3.],
            rotation: [1., 0., 0., 0.],
        }),
        movement: None,
        parent: None,
        children: vec![],
        velocity: [0.; 3],
        acceleration: [0.; 3],
        omega: [0.; 3],
        sequences: sequences(),
        admin_vision: false,
        change_no_draw: false,
        cloak_status: 1,
    }
}
fn entity(id: u32, kind: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: source(kind),
    }
}
#[test]
fn complete_nested_and_equipped_plan_uses_accepted_locations_and_atomic_sequence_owner() {
    treasure_table_support::install_tables();
    let saved = player();
    let mut entities = vec![
        entity(2, 21),
        entity(3, 1),
        entity(4, 21),
        entity(5, 1),
        entity(6, 6),
    ];
    // Deliberately stale saved relationships must be replaced by accepted placements.
    entities[4]
        .state
        .properties
        .instance_ids
        .push(Property { id: 2, value: 999 });
    entities[4].state.properties.ints.push(Property {
        id: 10,
        value: 0x400000,
    });
    entities[4].state.properties.ints.push(Property {
        id: 9,
        value: 0x100000,
    });
    let placements = [
        ItemPlacementV2::Contained {
            container: ACTOR,
            slot: 0,
            pack_slot: true,
            equipped: 0,
        },
        ItemPlacementV2::Contained {
            container: 2,
            slot: 1,
            pack_slot: false,
            equipped: 0,
        },
        ItemPlacementV2::Contained {
            container: 2,
            slot: 0,
            pack_slot: true,
            equipped: 0,
        },
        ItemPlacementV2::Contained {
            container: 4,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        },
        ItemPlacementV2::Contained {
            container: ACTOR,
            slot: 0,
            pack_slot: false,
            equipped: 0x100000,
        },
    ];
    let items = entities
        .iter()
        .zip(&placements)
        .map(|(entity, placement)| EntryInventoryItem { entity, placement })
        .collect::<Vec<_>>();
    let counters = entities
        .iter()
        .map(|v| (v.object_id, sequences()))
        .collect();
    let chargen = bace_dat::CharGen {
        reserved: 0,
        starter_areas: vec![],
        heritage_marker: 0,
        heritage_groups: BTreeMap::new(),
    };
    let setups = BTreeMap::from([(
        0x02000001,
        bace_dat::ModelSetup {
            id: 0x02000001,
            flags: 0,
            parts: vec![0x01000001],
            parents: vec![],
            scales: vec![],
            placements: BTreeMap::new(),
            default_animation: 0,
            default_motion: 0,
        },
    )]);
    let clothing = BTreeMap::new();
    let palettes = BTreeMap::new();
    let assets = EntryAppearanceAssets {
        chargen: &chargen,
        setups: &setups,
        clothing: &clothing,
        palettes: &palettes,
    };
    let build = |counters| {
        prepare_player_entry(PlayerEntryInput {
            saved: &saved,
            items: &items,
            enchantments: &[],
            friends: FriendsUpdate {
                kind: FriendsUpdateKind::Full,
                friends: vec![],
            },
            actor_state: live(),
            item_sequences: counters,
            missile_combat: false,
            plussed: false,
            assets: &assets,
        })
    };
    let plan = build(&counters).unwrap();
    assert!(plan.self_object.physics.options.movement.is_none());
    assert_eq!(plan.self_object.physics.options.children[0].object_id, 6);
    let order = plan
        .possessions
        .iter()
        .map(|v| match v {
            PreparedEntryPossession::Create(v) => format!("create{}", v.object_id),
            PreparedEntryPossession::Contents { container_id, .. } => {
                format!("contents{container_id}")
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        vec![
            "create2",
            "contents2",
            "create3",
            "create4",
            "contents4",
            "create5",
            "create6"
        ]
    );
    let PreparedEntryPossession::Contents { items, .. } = &plan.possessions[1] else {
        panic!()
    };
    assert_eq!(
        items.iter().map(|i| i.object_id).collect::<Vec<_>>(),
        vec![4, 3]
    );
    assert_eq!(items[0].container_type, 1);
    let PreparedEntryPossession::Create(weapon) = plan.possessions.last().unwrap() else {
        panic!()
    };
    assert_eq!(weapon.game.options.wielder, Some(ACTOR));
    assert_eq!(weapon.game.options.container, None);
    assert_eq!(weapon.game.options.wielded_location, Some(0x100000));
    assert_eq!(weapon.game.options.valid_locations, Some(0x100000));
    assert_eq!(weapon.physics.options.parent.unwrap().location, 1);
    assert_eq!(weapon.physics.options.position, live().position);
    let binding = bace_gameplay_api::CharacterBinding {
        actor: bace_types::EntityId(ACTOR),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
    };
    let mut events = bace_replication::EventSequencer::new(binding, 22);
    let limits = bace_replication::LoginProjectionLimits {
        batch: bace_replication::BatchLimits {
            max_messages: 32,
            max_bytes: 65536,
            max_message_bytes: 16384,
            max_string_bytes: 1024,
        },
        description: bace_wire::PlayerDescriptionLimits {
            max_table_entries: 4096,
            max_string_bytes: 1024,
            max_gameplay_options_bytes: 8192,
            max_message_bytes: 16384,
        },
        objects: bace_wire::ObjectCodecLimits {
            max_message_bytes: 16384,
            max_model_entries: 765,
            max_children: 128,
            max_restrictions: 128,
            max_motion_commands: 128,
            max_string_bytes: 1024,
        },
        social: bace_wire::SocialCodecLimits {
            max_filters: 256,
            max_entries: 4096,
            max_string_bytes: 1024,
            max_message_bytes: 16384,
        },
        max_titles: 4096,
        max_container_items: 1024,
    };
    let mut small = limits;
    small.batch.max_messages = 5;
    assert!(plan.project(&mut events, binding, small).is_err());
    assert_eq!(events.next_sequence(), 22);
    let batch = plan.project(&mut events, binding, limits).unwrap();
    assert_eq!(batch.messages.len(), 12);
    assert_eq!(events.next_sequence(), 27);
    let mut shallow_entities = entities.clone();
    shallow_entities[2].state.weenie_type = 1;
    let mut shallow_places = placements.clone();
    shallow_places[2] = ItemPlacementV2::Contained {
        container: 2,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    shallow_places[3] = ItemPlacementV2::Contained {
        container: ACTOR,
        slot: 1,
        pack_slot: false,
        equipped: 0,
    };
    let shallow = [0, 2, 1, 3, 4].map(|i| EntryInventoryItem {
        entity: &shallow_entities[i],
        placement: &shallow_places[i],
    });
    let source_plan = prepare_player_entry(PlayerEntryInput {
        saved: &saved,
        items: &shallow,
        enchantments: &[],
        friends: FriendsUpdate {
            kind: FriendsUpdateKind::Full,
            friends: vec![],
        },
        actor_state: live(),
        item_sequences: &counters,
        missile_combat: false,
        plussed: false,
        assets: &assets,
    })
    .unwrap();
    let source_order = source_plan
        .possessions
        .iter()
        .map(|v| match v {
            PreparedEntryPossession::Create(v) => format!("create{}", v.object_id),
            PreparedEntryPossession::Contents { container_id, .. } => {
                format!("contents{container_id}")
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_order,
        include_str!("fixtures/entry_order.txt")
            .lines()
            .filter(|v| !v.starts_with('#'))
            .collect::<Vec<_>>()
    );
    let mut missing = counters.clone();
    missing.remove(&6);
    assert!(build(&missing).is_err());
}
