use super::*;
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
fn item(id: u32, parent: u32, container: bool) -> InventoryItem {
    InventoryItem {
        id: EntityId(id),
        revision: 1,
        template: id,
        stack_key: u64::from(id),
        place: ItemPlace::Contained {
            container: EntityId(parent),
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        structure: None,
        pack_slot: container,
        is_container: container,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
#[test]
fn detach_is_atomic_and_transfers_the_exact_complete_owned_hierarchy() {
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(2),
        session: bace_gameplay_api::SessionId(9),
    };
    let (mut kernel, prepared) = fixture::fixture(binding);
    assert!(kernel.admit_player(prepared).is_ok());
    kernel
        .register_inventory_container(InventoryContainer {
            id: EntityId(2),
            revision: 1,
            root_owner: Some(binding.actor),
            slots: 8,
            pack_slots: 0,
            burden_limit: 100,
            accessible: true,
            open: true,
            generation: 1,
        })
        .unwrap();
    kernel.register_inventory_item(item(2, 1, true)).unwrap();
    kernel.register_inventory_item(item(3, 2, false)).unwrap();
    let snapshot = kernel.read_player_snapshot(binding).unwrap();
    let mut request = bace_simulation::PlayerDetachRequest {
        correlation: 1,
        binding,
        expected_revision: snapshot.character().progression().revision(),
        expected_items: vec![(EntityId(2), 1)],
        capture_final: true,
    };
    assert!(
        kernel.detach_player(&request).is_err(),
        "missing nested item cannot detach partial graph"
    );
    assert!(kernel.character(binding.actor).is_some());
    assert!(kernel.inventory_item(EntityId(3)).is_some());
    request.expected_items.push((EntityId(3), 1));
    request.expected_revision += 1;
    assert!(
        kernel.detach_player(&request).is_err(),
        "future durable revision cannot be accepted"
    );
    assert!(kernel.character(binding.actor).is_some());
    request.expected_revision -= 1;
    let detached = kernel
        .detach_player(&request)
        .unwrap_or_else(|e| panic!("detach: {e:?}"));
    assert_eq!(detached.actor.id, binding.actor);
    assert_eq!(
        detached.items.iter().map(|i| i.id).collect::<Vec<_>>(),
        [EntityId(2), EntityId(3)]
    );
    assert_eq!(
        detached.containers.iter().map(|c| c.id).collect::<Vec<_>>(),
        [EntityId(1), EntityId(2)]
    );
    assert!(kernel.character(binding.actor).is_none());
    assert!(kernel.inventory_item(EntityId(2)).is_none());
    assert!(kernel.inventory_item(EntityId(3)).is_none());
    assert!(kernel.inventory_container(binding.actor).is_none());
    assert!(kernel.world().actor_state(binding.actor).is_err());
}
#[test]
fn entry_state_uses_accepted_motion_and_durable_instance() {
    // The production cold-start owner installs this pinned enum catalog before
    // player entry. This standalone projection test must establish the same input.
    if bace_loot::ace_tables::active_id().is_none() {
        let source =
            include_str!("../../../../../gameplay/bace-loot/data/ace-treasure-tables.toml");
        let tables = bace_content_tools::parse_treasure_table_set(source).unwrap();
        if let Err(error) = bace_loot::ace_tables::install(tables) {
            assert_eq!(bace_loot::ace_tables::active_id(), Some(1), "{error}");
        }
    }
    assert_eq!(bace_loot::ace_tables::active_id(), Some(1));
    let binding = CharacterBinding {
        actor: EntityId(0x50000001),
        account: bace_types::AccountId(2),
        session: bace_gameplay_api::SessionId(9),
    };
    let (mut kernel, prepared) = fixture::fixture(binding);
    assert!(kernel.admit_player(prepared).is_ok());
    for _ in 0..3 {
        kernel.step().unwrap();
    }
    let snapshot = kernel.read_player_snapshot(binding).unwrap();
    let mut properties = bace_content::SparseProperties::default();
    properties.data_ids.push(bace_content::Property {
        id: 1,
        value: 0x02000001,
    });
    let saved = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "entry_motion".into(),
                weenie_type: 1,
                last_modified: None,
                properties,
            },
        },
        account_id: 2,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    let receipt = bace_persistence::OnlineLoginReceipt {
        lease: bace_persistence::CharacterLease {
            character_id: 0x50000001,
            epoch: 1,
            state: OwnershipState::Online,
        },
        total_logins: 65537,
    };
    let setup = bace_dat::ModelSetup {
        id: 0x02000001,
        flags: 0,
        parts: vec![],
        parents: vec![],
        scales: vec![],
        placements: Default::default(),
        default_animation: 0,
        default_motion: 0,
    };
    assert!(
        crate::player_entry::prepare_player_entry_state(
            &saved,
            &snapshot,
            receipt,
            &Sequences::new(256).unwrap(),
            &setup
        )
        .is_err()
    );
    let owner = Sequences::with_instance(256, 1).unwrap();
    let projection =
        crate::player_entry::prepare_player_entry_state(&saved, &snapshot, receipt, &owner, &setup)
            .unwrap_or_else(|e| {
                panic!(
                    "{e}: {:?} {:?}",
                    snapshot.entry_physics(),
                    snapshot.entry_motion()
                )
            });
    assert_eq!(projection.sequences.instance, 1);
    assert_eq!(projection.position.unwrap().cell, snapshot.world().cell.0);
    assert!(matches!(
        projection.movement,
        Some(bace_wire::PhysicsMovement::Motion(_))
    ));
}
