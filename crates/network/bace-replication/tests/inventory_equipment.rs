//! Source vectors from ACE GameMessageParentEvent/PickupEvent and
//! GameMessagePrivateUpdateAttribute2ndLevel at 47edade3bd3f6044b676d4eb877c4965c7eda62b.
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{
    BatchLimits, EventSequencer, InventoryProjection as P, SequenceKind, Sequences,
};
use bace_types::{AccountId, EntityId};
use bace_wire::{ObjectCodecLimits, ObjectModel};
use std::collections::BTreeMap;
fn limits() -> (ObjectCodecLimits, BatchLimits) {
    (
        ObjectCodecLimits {
            max_message_bytes: 4096,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 1024,
        },
        BatchLimits {
            max_messages: 32,
            max_bytes: 16384,
            max_message_bytes: 4096,
            max_string_bytes: 1024,
        },
    )
}
#[test]
fn parent_uses_wearer_instance_child_position_and_whole_batch_is_atomic() {
    let b = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    };
    let item = EntityId(0x80000001);
    let mut event = EventSequencer::new(b, 17);
    let mut actor = Sequences::with_instance(16, 0x1234).unwrap();
    let mut items = BTreeMap::from([(item, Sequences::with_instance(16, 0x6789).unwrap())]);
    let model = ObjectModel::default();
    let steps = [
        P::Parent {
            parent: b.actor,
            item,
            location: 1,
            placement: 1,
        },
        P::Appearance {
            item: b.actor,
            model: &model,
        },
        P::Vital {
            vital: 2,
            current: 37,
        },
        P::Magic(bace_wire::MagicEvent::Remove {
            spell: 100,
            layer: 2,
        }),
    ];
    let (objects, good) = limits();
    assert!(
        event
            .project_inventory_with_actor(
                b,
                &steps,
                &mut items,
                Some(&mut actor),
                objects,
                BatchLimits {
                    max_bytes: 1,
                    ..good
                }
            )
            .is_err()
    );
    assert_eq!(items[&item].current(SequenceKind::ObjectPosition, 0), 0);
    assert_eq!(actor.current(SequenceKind::ObjectVisualDesc, 0), 0);
    assert_eq!(event.next_sequence(), 17);
    let batch = event
        .project_inventory_with_actor(b, &steps, &mut items, Some(&mut actor), objects, good)
        .unwrap();
    // Original ParentEvent: opcode, wearer, child, parent location, placement,
    // wearer's current instance, child's next position (little endian).
    assert_eq!(
        batch.messages[0].bytes,
        vec![
            0x49, 0xf7, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0x80, 1, 0, 0, 0, 1, 0, 0, 0, 0x34, 0x12, 1, 0
        ]
    );
    assert_eq!(batch.messages[0].queue, 10);
    assert_eq!(items[&item].current(SequenceKind::ObjectPosition, 0), 1);
    assert_eq!(actor.current(SequenceKind::ObjectVisualDesc, 0), 1);
    assert_eq!(event.next_sequence(), 18);
    let next = event
        .project_inventory(b, &[P::Pickup(item)], &mut items, objects, good)
        .unwrap();
    assert_eq!(
        next.messages[0].bytes,
        vec![0x4a, 0xf7, 0, 0, 1, 0, 0, 0x80, 0x89, 0x67, 2, 0]
    );
}
#[test]
fn equipment_event_failure_does_not_advance_private_properties_or_event_sequence() {
    let b = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    };
    let item = EntityId(2);
    let mut e = EventSequencer::new(b, 4);
    let mut items = BTreeMap::from([(item, Sequences::new(8).unwrap())]);
    let (objects, limits) = limits();
    let steps = [
        P::Property {
            item,
            property: 3,
            value: bace_wire::PropertyValue::InstanceId(0),
        },
        P::Social(bace_wire::SocialEvent::Transient(&"x".repeat(2048))),
    ];
    assert!(
        e.project_inventory(b, &steps, &mut items, objects, limits)
            .is_err()
    );
    assert_eq!(e.next_sequence(), 4);
    assert_eq!(
        items[&item].current(SequenceKind::PropertyInstanceId, 3),
        255
    );
}
