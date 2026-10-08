use bace_content::{Property, SecondaryAttribute, WeenieV1};
use bace_entity::EntityVital;
use bace_inventory::{InventoryItem, InventoryProposal, ItemChange, ItemPlace};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::{
    game_inventory::{FrozenInventoryItem, InventoryFreezeInput},
    magic_saves::*,
};
use bace_simulation::{InventoryTicket, MagicResourceCommit};
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV5, PlayerSaveV1, PlayerSaveV6};
use bace_types::EntityId;
use std::collections::BTreeMap;
const PLAYER: u32 = 0x50000001;
const ITEM: u32 = 0x80000001;
fn state(template: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: template,
        class_name: format!("fixture_{template}"),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    }
}
fn player() -> PlayerSaveV6 {
    let mut state = state(1);
    state.properties.secondary_attributes.push(Property {
        id: 5,
        value: SecondaryAttribute {
            init_level: 100,
            level_from_cp: 2,
            cp_spent: 20,
            current_level: 100,
        },
    });
    state.properties.strings.push(Property {
        id: 999,
        value: "preserved".into(),
    });
    PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: PLAYER,
            template_revision: 1,
            mutation_revision: 4,
            state,
        },
        account_id: 1,
        name: "Mage".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}
fn item() -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(ITEM),
        revision: 7,
        template: 100,
        stack_key: 10,
        place: ItemPlace::Contained {
            container: EntityId(PLAYER),
            slot: 0,
            equipped: 0,
        },
        stack: 2,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
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
fn ticket() -> InventoryTicket {
    let before = item();
    let mut after = before.clone();
    after.stack = 1;
    after.revision = 8;
    InventoryTicket {
        operation: 9,
        actor: EntityId(PLAYER),
        proposal: InventoryProposal {
            changes: vec![ItemChange {
                before: Some(before),
                after,
            }],
            participants: vec![(EntityId(PLAYER), 4), (EntityId(ITEM), 7)],
            actor_burden: 1,
            requires_pickup_motion: false,
        },
    }
}
fn frozen() -> FrozenInventoryItem {
    let mut state = state(100);
    state.properties.ints = vec![
        Property { id: 12, value: 2 },
        Property {
            id: 9999,
            value: 42,
        },
    ];
    FrozenInventoryItem {
        corpse: None,
        construction: None,
        source_destination: Some(2),
        entity: EntitySaveV1 {
            object_id: ITEM,
            template_revision: 2,
            mutation_revision: 7,
            state,
        },
        enchantments: vec![],
        placement: Some(ItemPlacementV2::Contained {
            container: PLAYER,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }),
        persisted_version: 2,
    }
}
fn resources() -> MagicResourceCommit {
    MagicResourceCommit {
        operation: 9,
        actor: EntityId(PLAYER),
        cast: 12,
        mana: bace_entity::VitalMutation {
            actor: EntityId(PLAYER),
            vital: EntityVital::Mana,
            before: 100,
            after: 90,
        },
        recovery: Default::default(),
        prepared_at: 1.0,
        before_revision: 4,
        after_revision: 5,
    }
}
fn freeze<'a>(
    ticket: &'a InventoryTicket,
    resources: &'a MagicResourceCommit,
    player: &'a PlayerSaveV6,
    items: &'a [FrozenInventoryItem],
) -> Result<PendingMagicSave, MagicSaveError> {
    let leases = [CharacterLease {
        character_id: PLAYER,
        epoch: 3,
        state: OwnershipState::Online,
    }];
    let positions = BTreeMap::new();
    freeze_magic_inventory(MagicSaveInput {
        id: MagicOperationId::new([7; 16]).unwrap(),
        ticket,
        resources,
        player,
        player_version: 6,
        lease: leases[0],
        captured_unix_millis: 11_000,
        inventory: InventoryFreezeInput {
            operation_id: "replaced-by-stable-magic-id",
            proposal: &ticket.proposal,
            items,
            other_snapshots: &[],
            leases: &leases,
            storage_views: &[],
            admitted_positions: &positions,
        },
    })
}
#[test]
fn frozen_component_transaction_contains_exact_mana_item_and_pre_effect_recovery() {
    let saved = player();
    let ticket = ticket();
    let resource = resources();
    let item = frozen();
    let pending = freeze(&ticket, &resource, &saved, &[item]).unwrap();
    let operation = pending.operation();
    assert_eq!(operation.snapshots.len(), 2);
    assert_eq!(operation.changes.len(), 1);
    assert_eq!(
        operation.operation_id,
        "magic:07070707070707070707070707070707"
    );
    let player = PlayerSaveV6::decode(
        &operation
            .snapshots
            .iter()
            .find(|s| s.object_id == PLAYER)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(player.player.entity.mutation_revision, 5);
    assert_eq!(
        player.player.entity.state.properties.secondary_attributes[0]
            .value
            .current_level,
        90
    );
    assert_eq!(
        player.player.entity.state.properties.secondary_attributes[0]
            .value
            .cp_spent,
        20
    );
    assert_eq!(
        player.player.entity.state.properties.strings,
        saved.player.entity.state.properties.strings
    );
    assert_eq!(player.combat_recovery.unwrap().state.last_success_school, 0);
    assert_eq!(player.combat_recovery.unwrap().captured_unix_millis, 11_000);
    let item = ItemSaveV5::decode(
        &operation
            .snapshots
            .iter()
            .find(|s| s.object_id == ITEM)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(item.source_destination, Some(2));
    assert_eq!(item.entity.mutation_revision, 8);
    assert_eq!(
        item.entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .unwrap()
            .value,
        1
    );
    assert_eq!(
        item.entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 9999)
            .unwrap()
            .value,
        42
    );
    assert_eq!(
        saved.player.entity.state.properties.secondary_attributes[0]
            .value
            .current_level,
        100
    );
}
#[test]
fn stale_mana_revision_and_cross_operation_resources_cannot_produce_a_write() {
    let saved = player();
    let ticket = ticket();
    let items = [frozen()];
    let mut resource = resources();
    resource.operation += 1;
    assert!(freeze(&ticket, &resource, &saved, &items).is_err());
    let mut resource = resources();
    resource.before_revision -= 1;
    assert!(freeze(&ticket, &resource, &saved, &items).is_err());
    let mut resource = resources();
    resource.mana.before = 99;
    assert!(freeze(&ticket, &resource, &saved, &items).is_err());
    let mut resource = resources();
    resource.mana.vital = EntityVital::Health;
    assert!(freeze(&ticket, &resource, &saved, &items).is_err());
    let mut empty = ticket;
    empty.proposal.changes.clear();
    assert!(freeze(&empty, &resources(), &saved, &items).is_err());
}
