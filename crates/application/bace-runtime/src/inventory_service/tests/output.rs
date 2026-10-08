use super::*;
use bace_replication::{BatchLimits, EventSequencer, SequenceKind, Sequences};
use bace_wire::*;
use std::collections::BTreeMap;
pub(super) fn assert_complete_output(completion: &InventoryCompletion) {
    let binding = completion.work.binding;
    let object = ObjectDescription {
        object_id: FRESH.0,
        model: ObjectModel::default(),
        physics: PhysicsDescription {
            state: 0x400,
            options: PhysicsOptions::default(),
            sequences: PhysicsSequences {
                position: 0,
                movement: 0,
                state: 0,
                vector: 0,
                teleport: 0,
                server_control: 0,
                force_position: 0,
                visual_description: 0,
                instance: 0,
            },
        },
        game: ObjectGameData {
            name: "Known fresh stack".into(),
            class_id: 10,
            icon_id: 0x6000001,
            item_type: 128,
            description_flags: 0,
            options: ObjectGameOptions {
                stack_size: Some(2),
                max_stack_size: Some(100),
                container: Some(binding.actor.0),
                ..Default::default()
            },
        },
    };
    let objects = ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 1024,
    };
    let limits = BatchLimits {
        max_messages: 16,
        max_bytes: 16384,
        max_message_bytes: 4096,
        max_string_bytes: 1024,
    };
    let mut events = EventSequencer::new(binding, 7);
    let mut counters = BTreeMap::from([(SOURCE, Sequences::with_instance(16, 1).unwrap())]);
    assert!(
        project_inventory_completion(
            completion,
            &mut events,
            &mut counters,
            &BTreeMap::new(),
            objects,
            limits
        )
        .is_err()
    );
    assert_eq!(events.next_sequence(), 7);
    assert_eq!(
        counters[&SOURCE].current(SequenceKind::PropertyInt, 12),
        255
    );
    let created = BTreeMap::from([(FRESH, object)]);
    let tiny = BatchLimits {
        max_bytes: 1,
        ..limits
    };
    assert!(
        project_inventory_completion(
            completion,
            &mut events,
            &mut counters,
            &created,
            objects,
            tiny
        )
        .is_err()
    );
    assert_eq!(events.next_sequence(), 7);
    assert_eq!(
        counters[&SOURCE].current(SequenceKind::PropertyInt, 12),
        255
    );
    let batch = project_inventory_completion(
        completion,
        &mut events,
        &mut counters,
        &created,
        objects,
        limits,
    )
    .unwrap()
    .unwrap();
    let order: Vec<_> = batch
        .messages
        .iter()
        .map(
            |m| match u32::from_le_bytes(m.bytes[..4].try_into().unwrap()) {
                0xf745 => "create",
                0xf7b0 => "contain",
                0x197 => "stack",
                opcode => panic!("unexpected {opcode:x}"),
            },
        )
        .collect();
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/inventory_split_order.json"
    ))
    .unwrap();
    assert_eq!(serde_json::json!(order), oracle["cases"][0]["messages"]);
    assert_eq!(events.next_sequence(), 8);
    assert_eq!(counters[&SOURCE].current(SequenceKind::PropertyInt, 12), 0);
    assert_eq!(batch.messages[2].bytes[9..13], 3u32.to_le_bytes());
}

#[test]
fn durable_bag_snapshot_children_publish_after_placement_and_view() {
    let (_, _, mut work) = fixture();
    let fresh = work.fresh.take().unwrap();
    let mut root = work
        .operation
        .ticket
        .proposal
        .changes
        .iter()
        .find(|c| c.after.id == SOURCE)
        .unwrap()
        .after
        .clone();
    root.is_container = true;
    root.stack = 1;
    let mut before = root.clone();
    before.place = bace_inventory::ItemPlace::World;
    before.revision -= 1;
    let mut child = fresh.item;
    child.place = bace_inventory::ItemPlace::Contained {
        container: SOURCE,
        slot: 0,
        equipped: 0,
    };
    work.operation.request = bace_gameplay_api::InventoryRequest::Move {
        item: SOURCE,
        container: work.binding.actor,
        placement: 0,
    };
    // Deliberately put the child first: network order follows containment, not IDs/proposal order.
    work.operation.ticket.proposal.changes = vec![
        bace_inventory::ItemChange {
            before: Some(child.clone()),
            after: child.clone(),
        },
        bace_inventory::ItemChange {
            before: Some(before),
            after: root.clone(),
        },
    ];
    let mut snapshots = Vec::new();
    let mut created = BTreeMap::new();
    for item in [&root, &child] {
        let mut entity = fresh.frozen.entity.clone();
        entity.object_id = item.id.0;
        entity.mutation_revision = item.revision;
        let bace_inventory::ItemPlace::Contained {
            container,
            slot,
            equipped,
        } = item.place
        else {
            unreachable!()
        };
        let saved = bace_storage_codec::ItemSaveV5::migrate_v4(
            bace_storage_codec::ItemSaveV4::migrate_v2(bace_storage_codec::ItemSaveV2 {
                entity,
                placement: bace_storage_codec::ItemPlacementV2::Contained {
                    container: container.0,
                    slot,
                    pack_slot: item.pack_slot,
                    equipped,
                },
            })
            .unwrap(),
        )
        .unwrap();
        snapshots.push(SaveSnapshot {
            object_id: item.id.0,
            mutation_revision: item.revision,
            expected_version: 1,
            bytes: saved.encode().unwrap(),
        });
        created.insert(
            item.id,
            ObjectDescription {
                object_id: item.id.0,
                model: ObjectModel::default(),
                physics: PhysicsDescription {
                    state: 0,
                    options: PhysicsOptions::default(),
                    sequences: PhysicsSequences {
                        position: 0,
                        movement: 0,
                        state: 0,
                        vector: 0,
                        teleport: 0,
                        server_control: 0,
                        force_position: 0,
                        visual_description: 0,
                        instance: 0,
                    },
                },
                game: ObjectGameData {
                    name: format!("known {}", item.id.0),
                    class_id: 10,
                    icon_id: 0x06000001,
                    item_type: 128,
                    description_flags: 0,
                    options: ObjectGameOptions::default(),
                },
            },
        );
    }
    let completion = InventoryCompletion {
        work,
        committed: true,
        snapshots,
    };
    let mut events = EventSequencer::new(completion.work.binding, 1);
    let mut counters = BTreeMap::from([
        (SOURCE, Sequences::new(16).unwrap()),
        (FRESH, Sequences::new(16).unwrap()),
    ]);
    let batch = project_inventory_completion(
        &completion,
        &mut events,
        &mut counters,
        &created,
        ObjectCodecLimits {
            max_message_bytes: 4096,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 1024,
        },
        BatchLimits {
            max_messages: 16,
            max_bytes: 16384,
            max_message_bytes: 4096,
            max_string_bytes: 1024,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(batch.messages.len(), 4);
    assert_eq!(
        u32::from_le_bytes(batch.messages[0].bytes[0..4].try_into().unwrap()),
        0xf745
    );
    assert_eq!(
        u32::from_le_bytes(batch.messages[0].bytes[4..8].try_into().unwrap()),
        SOURCE.0
    );
    let event =
        |index: usize| u32::from_le_bytes(batch.messages[index].bytes[12..16].try_into().unwrap());
    assert_eq!(event(1), 0x22);
    assert_eq!(event(2), 0x196);
    assert_eq!(
        u32::from_le_bytes(batch.messages[3].bytes[0..4].try_into().unwrap()),
        0xf745
    );
    assert_eq!(
        u32::from_le_bytes(batch.messages[3].bytes[4..8].try_into().unwrap()),
        FRESH.0
    );
}
