use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer, InventoryProjection, SequenceKind, Sequences};
use bace_types::{AccountId, EntityId};
use bace_wire::{ObjectCodecLimits, PositionPack, WirePosition};
use std::collections::BTreeMap;
#[test]
fn drop_container_and_position_share_atomic_counter_budget() {
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    };
    let id = EntityId(0x80000001);
    let mut events = EventSequencer::new(binding, 10);
    let pack = PositionPack {
        position: WirePosition {
            cell: 1,
            origin: [0., 1.1, 0.],
            rotation: [1., 0., 0., 0.],
        },
        velocity: Some([0.; 3]),
        placement: Some(101),
        grounded: true,
        instance_sequence: 999,
        position_sequence: 999,
        teleport_sequence: 999,
        force_position_sequence: 999,
    };
    let steps = [
        InventoryProjection::Container { item: id, value: 0 },
        InventoryProjection::Position { item: id, pack },
    ];
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
    let mut sequences = BTreeMap::from([(id, Sequences::new(1).unwrap())]);
    assert!(
        events
            .project_inventory(binding, &steps, &mut sequences, objects, limits)
            .is_err()
    );
    assert_eq!(
        sequences[&id].current(SequenceKind::PropertyInstanceId, 2),
        255
    );
    assert_eq!(sequences[&id].current(SequenceKind::ObjectPosition, 0), 0);
    let mut sequences = BTreeMap::from([(id, Sequences::with_instance(16, 17).unwrap())]);
    assert!(
        events
            .project_inventory(
                binding,
                &steps,
                &mut sequences,
                objects,
                BatchLimits {
                    max_bytes: 1,
                    ..limits
                }
            )
            .is_err()
    );
    assert_eq!(
        sequences[&id].current(SequenceKind::PropertyInstanceId, 2),
        255
    );
    assert_eq!(sequences[&id].current(SequenceKind::ObjectPosition, 0), 0);
    let batch = events
        .project_inventory(binding, &steps, &mut sequences, objects, limits)
        .unwrap();
    assert_eq!(batch.messages.len(), 2);
    let position = bace_wire::PositionUpdate::decode(&batch.messages[1].bytes).unwrap();
    assert_eq!(position.pack.instance_sequence, 17);
    assert_eq!(position.pack.position_sequence, 1);
    assert_eq!(position.pack.teleport_sequence, 0);
    assert_eq!(position.pack.force_position_sequence, 0);
    assert_eq!(position.pack.position, pack.position);
    assert_eq!(
        sequences[&id].current(SequenceKind::PropertyInstanceId, 2),
        0
    );
    assert_eq!(events.next_sequence(), 10);
    let malformed = [InventoryProjection::Position {
        item: id,
        pack: PositionPack {
            velocity: Some([f32::NAN, 0., 0.]),
            ..pack
        },
    }];
    assert!(
        events
            .project_inventory(binding, &malformed, &mut sequences, objects, limits)
            .is_err()
    );
    assert_eq!(sequences[&id].current(SequenceKind::ObjectPosition, 0), 1);
}

#[test]
fn private_death_position_preflights_and_uses_actor_position_type_counter() {
    let binding = CharacterBinding {
        actor: EntityId(0x5000_0001),
        account: AccountId(1),
        session: SessionId(1),
    };
    let position = WirePosition {
        cell: 0x1234_0001,
        origin: [1., 2., 3.],
        rotation: [1., 0., 0., 0.],
    };
    let steps = [
        InventoryProjection::PrivatePosition {
            position_type: 14,
            position,
        },
        InventoryProjection::System {
            text: "Your corpse is located at (0.0N, 0.0E).",
            chat_type: 0,
        },
    ];
    let objects = ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 1024,
    };
    let limits = BatchLimits {
        max_messages: 2,
        max_bytes: 4096,
        max_message_bytes: 4096,
        max_string_bytes: 1024,
    };
    let mut actor = Sequences::new(16).unwrap();
    let mut events = EventSequencer::new(binding, 7);
    assert!(
        events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut BTreeMap::new(),
                Some(&mut actor),
                objects,
                BatchLimits {
                    max_bytes: 1,
                    ..limits
                },
            )
            .is_err()
    );
    assert_eq!(actor.current(SequenceKind::Position, 14), 255);
    assert_eq!(events.next_sequence(), 7);
    let batch = events
        .project_inventory_with_actor(
            binding,
            &steps,
            &mut BTreeMap::new(),
            Some(&mut actor),
            objects,
            limits,
        )
        .unwrap();
    assert_eq!(
        batch.messages.iter().map(|m| m.queue).collect::<Vec<_>>(),
        [9, 9]
    );
    assert_eq!(
        bace_wire::PrivatePositionUpdate::decode(&batch.messages[0].bytes).unwrap(),
        bace_wire::PrivatePositionUpdate {
            sequence: 0,
            position_type: 14,
            position,
        }
    );
    assert_eq!(actor.current(SequenceKind::Position, 14), 0);
    assert_eq!(events.next_sequence(), 7);
    let invalid = [InventoryProjection::PrivatePosition {
        position_type: 14,
        position: WirePosition {
            cell: 0,
            ..position
        },
    }];
    assert!(
        events
            .project_inventory_with_actor(
                binding,
                &invalid,
                &mut BTreeMap::new(),
                Some(&mut actor),
                objects,
                limits,
            )
            .is_err()
    );
    assert_eq!(actor.current(SequenceKind::Position, 14), 0);
}

#[test]
fn source_corpse_dequip_delete_uses_smartbox_and_current_instance_sequence() {
    // ACE GameMessageDeleteObject.cs writes Guid, current ObjectInstance u16,
    // and alignment on SmartboxQueue after ToCorpseOnDeath dequip.
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    };
    let item = EntityId(0x8000_0001);
    let mut events = EventSequencer::new(binding, 10);
    let mut items = BTreeMap::from([(item, Sequences::with_instance(16, 17).unwrap())]);
    let batch = events
        .project_inventory(
            binding,
            &[InventoryProjection::Delete(item)],
            &mut items,
            ObjectCodecLimits {
                max_message_bytes: 4096,
                max_model_entries: 255,
                max_children: 128,
                max_restrictions: 1024,
                max_motion_commands: 32,
                max_string_bytes: 1024,
            },
            BatchLimits {
                max_messages: 1,
                max_bytes: 4096,
                max_message_bytes: 4096,
                max_string_bytes: 1024,
            },
        )
        .unwrap();
    assert_eq!(batch.messages.len(), 1);
    assert_eq!(batch.messages[0].queue, 10);
    assert_eq!(
        batch.messages[0].bytes,
        [0x47, 0xf7, 0, 0, 1, 0, 0, 0x80, 17, 0, 0, 0]
    );
    assert_eq!(items[&item].current(SequenceKind::ObjectInstance, 0), 17);
}
