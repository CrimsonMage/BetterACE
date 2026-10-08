use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer};
use bace_types::{AccountId, EntityId};
use bace_wire::{GameEventEnvelope, Reader, SalvageWireResult};
fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(1),
        account: AccountId(2),
        actor: EntityId(0x50000001),
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 300,
        max_bytes: 100000,
        max_message_bytes: 4096,
        max_string_bytes: 4096,
    }
}
fn golden(name: &str) -> Vec<u8> {
    let line = include_str!("../../bace-wire/tests/fixtures/crafting.tsv")
        .lines()
        .find(|l| l.starts_with(&format!("{name}\t")))
        .unwrap();
    line.split('\t')
        .nth(1)
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn single_bag_and_empty_ack_match_pinned_ace_serializer() {
    let b = binding();
    let mut sequence = EventSequencer::new(b, 9);
    let bag = SalvageWireResult {
        material: 61,
        workmanship: 9.5,
        units: 100,
    };
    let batch = sequence
        .project_salvage_results(b, 40, &[], &[bag], 100, limits())
        .unwrap();
    assert_eq!(batch.messages[0].bytes, golden("salvage-100"));
    assert_eq!(batch.messages[0].queue, 9);
    let mut sequence = EventSequencer::new(b, 9);
    let batch = sequence
        .project_salvage_results(b, 40, &[], &[], 100, limits())
        .unwrap();
    assert_eq!(batch.messages[0].bytes, golden("salvage-empty"));
}
#[test]
fn retail_cardinality_unsuitable_once_and_sequence_admission_are_atomic() {
    let b = binding();
    let mut sequence = EventSequencer::new(b, u32::MAX);
    let bags = [
        SalvageWireResult {
            material: 61,
            workmanship: 9.5,
            units: 100,
        },
        SalvageWireResult {
            material: 61,
            workmanship: 9.5,
            units: 1,
        },
    ];
    let mut small = limits();
    small.max_bytes = 55;
    assert!(
        sequence
            .project_salvage_results(b, 40, &[99], &bags, 100, small)
            .is_err()
    );
    assert_eq!(sequence.next_sequence(), u32::MAX);
    let batch = sequence
        .project_salvage_results(b, 40, &[99], &bags, 100, limits())
        .unwrap();
    assert_eq!(batch.messages.len(), 2);
    assert_eq!(sequence.next_sequence(), 1);
    for (index, message) in batch.messages.iter().enumerate() {
        let envelope = GameEventEnvelope::decode(&message.bytes, 4096).unwrap();
        assert_eq!(envelope.event.0, 0x2b4);
        assert_eq!(envelope.sequence, u32::MAX.wrapping_add(index as u32));
        let mut reader = Reader::new(envelope.payload);
        assert_eq!(reader.u32().unwrap(), 40);
        assert_eq!(reader.u32().unwrap(), u32::from(index == 0));
        if index == 0 {
            assert_eq!(reader.u32().unwrap(), 99);
        }
        assert_eq!(reader.u32().unwrap(), 1);
        assert_eq!(reader.u32().unwrap(), 61);
        assert_eq!(reader.f64().unwrap(), 9.5);
        assert_eq!(reader.u32().unwrap(), bags[index].units);
        assert_eq!(reader.u32().unwrap(), 100);
        assert_eq!(reader.remaining(), 0);
    }
    let wrong = CharacterBinding {
        session: SessionId(2),
        ..b
    };
    assert!(
        sequence
            .project_salvage_results(wrong, 40, &[], &bags, 100, limits())
            .is_err()
    );
    assert_eq!(sequence.next_sequence(), 1);
}

#[test]
fn confirmation_and_use_done_share_atomic_session_counter() {
    use bace_replication::InventoryProjection;
    use bace_wire::{CraftingEvent, ObjectCodecLimits, SimpleGameEvent};
    let binding = binding();
    let mut events = EventSequencer::new(binding, 9);
    let mut items = std::collections::BTreeMap::new();
    let objects = ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 4096,
    };
    let text = "You determine that you have a 38 percent chance to succeed.\n5 percent is due to your augmentation.";
    let steps = [
        InventoryProjection::Crafting(CraftingEvent::ConfirmationRequest {
            confirmation_type: 5,
            context: 7,
            text,
        }),
        InventoryProjection::Simple(SimpleGameEvent::UseDone(0)),
    ];
    let mut small = limits();
    small.max_messages = 1;
    assert!(
        events
            .project_inventory(binding, &steps, &mut items, objects, small)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 9);
    let batch = events
        .project_inventory(binding, &steps, &mut items, objects, limits())
        .unwrap();
    assert_eq!(batch.messages.len(), 2);
    assert_eq!(events.next_sequence(), 11);
    assert_eq!(
        GameEventEnvelope::decode(&batch.messages[0].bytes, 4096)
            .unwrap()
            .sequence,
        9
    );
    assert_eq!(
        GameEventEnvelope::decode(&batch.messages[1].bytes, 4096)
            .unwrap()
            .sequence,
        10
    );
}

#[test]
fn recipe_intermediate_properties_share_exact_item_and_actor_counters_atomically() {
    use bace_replication::{InventoryProjection, SequenceKind, Sequences};
    use bace_wire::{ObjectCodecLimits, PropertyValue};
    let b = binding();
    let id = EntityId(0x80000001);
    let mut events = EventSequencer::new(b, 4);
    let mut actor = Sequences::new(8).unwrap();
    let mut items = std::collections::BTreeMap::from([(id, Sequences::new(8).unwrap())]);
    let steps = [
        InventoryProjection::Property {
            item: id,
            property: 28,
            value: PropertyValue::Int(10),
        },
        InventoryProjection::Property {
            item: id,
            property: 28,
            value: PropertyValue::Int(20),
        },
        InventoryProjection::Property {
            item: b.actor,
            property: 205,
            value: PropertyValue::Int(1),
        },
    ];
    let objects = ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 4096,
    };
    let mut small = limits();
    small.max_bytes = 20;
    assert!(
        events
            .project_inventory_with_actor(b, &steps, &mut items, Some(&mut actor), objects, small)
            .is_err()
    );
    assert_eq!(items[&id].current(SequenceKind::PropertyInt, 28), 255);
    assert_eq!(actor.current(SequenceKind::PropertyInt, 205), 255);
    let result = events
        .project_inventory_with_actor(b, &steps, &mut items, Some(&mut actor), objects, limits())
        .unwrap();
    assert_eq!(result.messages[0].bytes[4], 0);
    assert_eq!(result.messages[1].bytes[4], 1);
    assert_eq!(result.messages[2].bytes[4], 0);
    assert_eq!(items[&id].current(SequenceKind::PropertyInt, 28), 1);
    assert_eq!(actor.current(SequenceKind::PropertyInt, 205), 0);
    assert_eq!(events.next_sequence(), 4);
}

#[test]
fn proficiency_spend_precedes_usedone_and_queued_grant_with_atomic_counters() {
    use bace_gameplay_api::{
        ProgressionProjection, ProgressionTarget, SkillAdvancement, TraitDetails,
        experience::{ExperienceEvent, ExperienceState},
    };
    use bace_replication::{
        CraftingProficiencyProjection, CraftingSkillSpend, SequenceKind, Sequences,
        project_crafting_commit,
    };
    let b = binding();
    let mut events = EventSequencer::new(b, 9);
    let mut actor = Sequences::new(32).unwrap();
    let mut items = std::collections::BTreeMap::new();
    let experience = ExperienceEvent {
        actor: b.actor,
        before: ExperienceState {
            total: 0,
            available: 95,
            level: 1,
            available_skill_credits: 0,
        },
        after: ExperienceState {
            total: 6,
            available: 101,
            level: 1,
            available_skill_credits: 0,
        },
        maximum_level: 275,
        update_properties: true,
        quest_amount: None,
        next_credit_level: None,
        vitals: vec![],
    };
    let input = || {
        Some(CraftingProficiencyProjection {
            skill: Some(CraftingSkillSpend {
                after: ProgressionProjection {
                    target: ProgressionTarget::Skill(18),
                    experience_spent: 5,
                    ranks: 0,
                    advancement: SkillAdvancement::Trained,
                    details: Some(TraitDetails::Skill {
                        initial_level: 0,
                        resistance_at_last_check: 5,
                        last_used_time: 10000.,
                    }),
                },
                available: 95,
                rank_changed: false,
                base: None,
                maximum: false,
            }),
            experience: Some(&experience),
        })
    };
    let objects = bace_wire::ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 256,
        max_children: 256,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 4096,
    };
    let mut small = limits();
    small.max_bytes = 70;
    assert!(
        project_crafting_commit(
            &mut events,
            b,
            &[],
            &mut items,
            &mut actor,
            input(),
            true,
            objects,
            small
        )
        .is_err()
    );
    assert_eq!(events.next_sequence(), 9);
    assert_eq!(actor.current(SequenceKind::PropertyInt64, 2), 255);
    let batch = project_crafting_commit(
        &mut events,
        b,
        &[],
        &mut items,
        &mut actor,
        input(),
        true,
        objects,
        limits(),
    )
    .unwrap();
    assert_eq!(batch.owner.messages.len(), 5);
    let bytes = &batch.owner.messages[0].bytes;
    assert_eq!(
        i64::from_le_bytes(bytes[bytes.len() - 8..].try_into().unwrap()),
        95
    );
    assert_eq!(
        u32::from_le_bytes(batch.owner.messages[1].bytes[..4].try_into().unwrap()),
        0x2dd
    );
    assert_eq!(
        GameEventEnvelope::decode(&batch.owner.messages[2].bytes, 4096)
            .unwrap()
            .event
            .0,
        0x1c7
    );
    assert_eq!(actor.current(SequenceKind::PropertyInt64, 2), 1);
    assert_eq!(actor.current(SequenceKind::Skill, 18), 0);
    assert_eq!(events.next_sequence(), 10);
}
