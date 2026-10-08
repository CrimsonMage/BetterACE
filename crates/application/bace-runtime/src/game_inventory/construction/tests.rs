use super::*;
use bace_storage_codec::{
    EntitySaveV1, FrozenConstructedChildV1, FrozenCreatureConstructionV1,
    FrozenGeneratorConstructionOriginV1,
};

fn companion(
    kind: u32,
    death_roster: Vec<FrozenConstructedChildV1>,
    gear: Vec<u32>,
) -> FrozenCreatureConstructionV1 {
    FrozenCreatureConstructionV1 {
        weenie_type: kind,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 0x8000_2600,
            incarnation: 1,
            content_revision: 1,
            profile: 1,
            occurrence: 1,
            random_identity: [4; 16],
            random_key_version: 1,
        },
        equipment_order: gear,
        death_roster,
    }
}

fn item(
    id: u32,
    kind: u32,
    parent: u32,
    equipped: u32,
    construction: Option<FrozenCreatureConstructionV1>,
) -> FrozenInventoryItem {
    FrozenInventoryItem {
        corpse: None,
        construction,
        source_destination: Some(2),
        enchantments: vec![],
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 100,
                class_name: "nested_constructed".into(),
                weenie_type: kind,
                last_modified: None,
                properties: Default::default(),
            },
        },
        placement: Some(ItemPlacementV2::Contained {
            container: parent,
            slot: 0,
            pack_slot: false,
            equipped,
        }),
        persisted_version: 0,
    }
}

#[test]
fn nested_creature_root_is_a_source_death_child_but_its_gear_has_an_exclusive_owner() {
    let outer = 0x8000_2601;
    let inner = 0x8000_2602;
    let gear = 0x8000_2603;
    let outer_companion = companion(
        10,
        vec![FrozenConstructedChildV1 {
            entity: inner,
            parent: None,
        }],
        vec![],
    );
    let items = vec![
        item(outer, 10, 0x8000_2600, 0, Some(outer_companion.clone())),
        item(
            inner,
            15,
            outer,
            0,
            Some(companion(
                15,
                vec![FrozenConstructedChildV1 {
                    entity: gear,
                    parent: None,
                }],
                vec![gear],
            )),
        ),
        item(gear, 1, inner, 1, None),
    ];
    let proposal = InventoryProposal {
        changes: vec![],
        participants: vec![],
        actor_burden: 0,
        requires_pickup_motion: false,
    };
    validate(&items, &proposal).unwrap();

    let mut wrong = items.clone();
    wrong[0]
        .construction
        .as_mut()
        .unwrap()
        .death_roster
        .push(FrozenConstructedChildV1 {
            entity: gear,
            parent: Some(inner),
        });
    assert!(matches!(
        validate(&wrong, &proposal),
        Err(InventoryFreezeError::Identity)
    ));

    let mut wrong = items;
    wrong[0].construction = Some(outer_companion);
    wrong[0]
        .construction
        .as_mut()
        .unwrap()
        .equipment_order
        .push(gear);
    assert!(matches!(
        validate(&wrong, &proposal),
        Err(InventoryFreezeError::Identity)
    ));
}
