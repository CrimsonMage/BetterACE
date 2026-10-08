use bace_content::WeenieV1;
use bace_gameplay_api::{ActionContext, NpcContext, SessionId};
use bace_inventory::{InventoryItem, InventoryProposal, ItemChange, ItemPlace};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::{
    game_inventory::{FrozenInventoryItem, InventoryFreezeInput},
    npc_persistence::{NpcCheckpointBinding, NpcHandInStageInput, freeze_handin_stage},
};
use bace_simulation::{
    InventoryTicket, NpcHandInRequest, NpcHandInTicket, NpcInvocationCheckpoint,
    NpcSourceCheckpoint,
};
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV5};
use bace_types::{AccountId, EntityId};
const PLAYER: u32 = 0x50000001;
const SOURCE: u32 = 0x80000002;
const ITEM: u32 = 0x80000010;
fn fixture() -> (NpcHandInTicket, FrozenInventoryItem) {
    let item = InventoryItem {
        structure: None,
        id: EntityId(ITEM),
        revision: 1,
        template: 100,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(PLAYER),
            slot: 0,
            equipped: 0,
        },
        stack: 10,
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
    };
    let mut after = item.clone();
    after.stack = 7;
    after.revision = 2;
    let context = NpcContext {
        source: EntityId(SOURCE),
        target: Some(EntityId(PLAYER)),
        operation: 77,
    };
    let ticket = NpcHandInTicket {
        category: 6,
        character_revision: 1,
        accepted_count: 3,
        request: NpcHandInRequest {
            context: ActionContext {
                actor: EntityId(PLAYER),
                account: AccountId(1),
                session: SessionId(1),
                sequence: 1,
            },
            source: EntityId(SOURCE),
            item: EntityId(ITEM),
            count: 3,
            event: [55; 16],
            operation: 77,
        },
        inventory: InventoryTicket {
            operation: 4,
            actor: EntityId(PLAYER),
            proposal: InventoryProposal {
                changes: vec![ItemChange {
                    before: Some(item),
                    after,
                }],
                participants: vec![(EntityId(PLAYER), 0), (EntityId(ITEM), 1)],
                actor_burden: 7,
                requires_pickup_motion: false,
            },
        },
        checkpoint: NpcSourceCheckpoint {
            inventory: None,
            location: None,
            properties: None,
            source_quests: None,
            archive: None,
            source: EntityId(SOURCE),
            active_operation: 77,
            invocations: vec![NpcInvocationCheckpoint {
                operation: 77,
                event_id: [55; 16],
                key_version: 1,
                random_position: 1,
            }],
            logical_now: 1.0,
            event_id: [55; 16],
            key_version: 1,
            random_position: 1,
            vm: bace_emotes::NativeCheckpoint {
                order: 1,
                remaining: 100,
                work: vec![bace_emotes::NativeScheduledRow {
                    inline: true,
                    depth: 0,
                    set: 0,
                    action: 0,
                    due: 1.0,
                    order: 1,
                    context,
                }],
                pending: vec![],
                detached: vec![],
            },
            pending: vec![],
        },
    };
    let saved = FrozenInventoryItem {
        source_destination: None,
        corpse: None,
        construction: None,
        enchantments: vec![],
        entity: EntitySaveV1 {
            object_id: ITEM,
            template_revision: 1,
            mutation_revision: 1,
            state: WeenieV1 {
                schema_version: 1,
                weenie_id: 100,
                class_name: "test-item".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        placement: Some(ItemPlacementV2::Contained {
            container: PLAYER,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        }),
        persisted_version: 5,
    };
    (ticket, saved)
}
#[test]
fn consumed_input_and_unexecuted_give_checkpoint_are_one_exact_operation() {
    let (ticket, saved) = fixture();
    let items = [saved];
    let positions = std::collections::BTreeMap::new();
    let leases = [CharacterLease {
        character_id: PLAYER,
        epoch: 3,
        state: OwnershipState::Online,
    }];
    let pending = freeze_handin_stage(NpcHandInStageInput {
        binding: NpcCheckpointBinding {
            source_version: 0,
            source_template: 100,
            invocation: [55; 16],
            source: SOURCE,
            program_hash: [2; 32],
            content_generation: [3; 32],
        },
        world_epoch: 7,
        ticket: &ticket,
        inventory: InventoryFreezeInput {
            operation_id: "ignored-in-favor-of-workflow-id",
            proposal: &ticket.inventory.proposal,
            items: &items,
            other_snapshots: &[],
            leases: &leases,
            storage_views: &[],
            admitted_positions: &positions,
        },
    })
    .unwrap();
    let op = pending.operation();
    assert_eq!(op.workflow.expected_version, 0);
    assert_eq!(op.inventory.snapshots.len(), 1);
    let restored = ItemSaveV5::decode_or_migrate(&op.inventory.snapshots[0].bytes, None).unwrap();
    assert_eq!(
        restored
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .unwrap()
            .value,
        7
    );
    let workflow = bace_storage_codec::NpcWorkflowSaveV3::decode(&op.workflow.checkpoint).unwrap();
    assert_eq!(workflow.scheduled.len(), 1);
    assert!(workflow.effects.is_empty());
    assert!(!workflow.completed);
    assert_eq!(workflow.event_id, [55; 16]);
    assert!(op.inventory.participants.contains(&SOURCE));
    assert_eq!(
        items[0].entity.mutation_revision, 1,
        "freezing never adopts accepted inventory"
    );
}
