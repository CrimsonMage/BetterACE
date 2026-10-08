//! Source order from pinned ACE AttributeTransferDevice.ActOnUse: two private
//! updates, success WeenieError 0x04E1, then device inventory consumption.
use bace_gameplay_api::{
    AttributeId, CharacterBinding, ProgressionProjection, ProgressionTarget, SessionId,
    SkillAdvancement, TraitDetails,
};
use bace_replication::{
    BatchLimits, EventSequencer, InventoryProjection, SequenceKind, Sequences,
    attribute_transfer::project_attribute_transfer,
};
use bace_types::{AccountId, EntityId};
use bace_wire::ObjectCodecLimits;
use std::collections::BTreeMap;

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn projection(id: AttributeId, starting_value: u32) -> ProgressionProjection {
    ProgressionProjection {
        target: ProgressionTarget::Attribute(id),
        experience_spent: 0,
        ranks: 0,
        advancement: SkillAdvancement::Inactive,
        details: Some(TraitDetails::Attribute { starting_value }),
    }
}
#[test]
fn pinned_yes_order_and_rejected_batch_keep_counters() {
    let binding = CharacterBinding {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(0x5000_0001),
    };
    let mut events = EventSequencer::new(binding, 7);
    let mut actor = Sequences::new(64).unwrap();
    let mut items = BTreeMap::new();
    let item = EntityId(0x5000_0002);
    items.insert(item, Sequences::new(64).unwrap());
    let objects = ObjectCodecLimits {
        max_message_bytes: 1024,
        max_model_entries: 32,
        max_children: 16,
        max_restrictions: 16,
        max_motion_commands: 16,
        max_string_bytes: 512,
    };
    let limits = BatchLimits {
        max_messages: 4,
        max_bytes: 1024,
        max_message_bytes: 1024,
        max_string_bytes: 512,
    };
    let project = |events: &mut EventSequencer,
                   actor: &mut Sequences,
                   items: &mut BTreeMap<EntityId, Sequences>,
                   limits| {
        project_attribute_transfer(
            events,
            binding,
            projection(AttributeId::Strength, 47),
            projection(AttributeId::Endurance, 100),
            vec![InventoryProjection::Delete(item)],
            items,
            actor,
            objects,
            limits,
        )
    };
    assert!(
        project(
            &mut events,
            &mut actor,
            &mut items,
            BatchLimits {
                max_messages: 3,
                ..limits
            }
        )
        .is_err()
    );
    assert_eq!(actor.current(SequenceKind::Attribute, 1), 255);
    let batch = project(&mut events, &mut actor, &mut items, limits).unwrap();
    assert_eq!(batch.messages.len(), 4);
    assert_eq!(
        batch.messages.iter().map(|m| m.queue).collect::<Vec<_>>(),
        [9, 9, 9, 10]
    );
    assert_eq!(word(&batch.messages[0].bytes, 0), 0x02e3);
    assert_eq!(word(&batch.messages[0].bytes, 5), 1);
    assert_eq!(word(&batch.messages[0].bytes, 13), 47);
    assert_eq!(word(&batch.messages[1].bytes, 0), 0x02e3);
    assert_eq!(word(&batch.messages[1].bytes, 5), 2);
    assert_eq!(word(&batch.messages[1].bytes, 13), 100);
    assert_eq!(word(&batch.messages[2].bytes, 0), 0xf7b0);
    assert_eq!(word(&batch.messages[2].bytes, 12), 0x028a);
    assert_eq!(word(&batch.messages[2].bytes, 16), 0x04e1);
    assert_eq!(word(&batch.messages[3].bytes, 0), 0xf747);
    assert_eq!(actor.current(SequenceKind::Attribute, 1), 0);
    assert_eq!(actor.current(SequenceKind::Attribute, 2), 0);
}
