use bace_gameplay_api::*;
use bace_replication::{skill_devices::*, *};
use bace_types::{AccountId, EntityId};
use bace_wire::ObjectCodecLimits;
use std::collections::BTreeMap;
fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(0x50000001),
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 32,
        max_bytes: 4096,
        max_message_bytes: 1024,
        max_string_bytes: 512,
    }
}
fn objects() -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_message_bytes: 1024,
        max_model_entries: 32,
        max_children: 16,
        max_restrictions: 16,
        max_motion_commands: 16,
        max_string_bytes: 512,
    }
}
fn view(mode: u32, skill: u32) -> SkillDeviceView<'static> {
    use SkillAdvancement::*;
    let before = ProgressionProjection {
        target: ProgressionTarget::Skill(skill),
        experience_spent: 50,
        ranks: 4,
        advancement: if matches!(mode, 1 | 2) {
            Specialized
        } else {
            Trained
        },
        details: Some(TraitDetails::Skill {
            initial_level: 0,
            resistance_at_last_check: 0,
            last_used_time: 0.,
        }),
    };
    let after = ProgressionProjection {
        advancement: match mode {
            0 | 2 | 5.. => Specialized,
            3 => Untrained,
            _ => Trained,
        },
        ..before
    };
    SkillDeviceView {
        kind: match mode {
            0 => SkillDeviceKind::Specialize,
            1..=4 => SkillDeviceKind::Lower,
            _ => SkillDeviceKind::Augment,
        },
        change: SkillTrainingChange {
            before,
            after,
            available_skill_credits: match mode {
                0 => 10,
                1 => 14,
                3 => 16,
                _ => 12,
            },
            revision: 2,
        },
        available_experience: match mode {
            0 => 100,
            1..=4 => 150,
            _ => 75,
        },
        actor_name: "Alice",
        device_name: "Test Gem",
    }
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn string_at(bytes: &[u8], offset: usize) -> String {
    let n = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize;
    String::from_utf8(bytes[offset + 2..offset + 2 + n].to_vec()).unwrap()
}
fn trace(message: &ReplicationMessage) -> String {
    let b = &message.bytes;
    match u32_at(b, 0) {
        0x2dd => format!("skill/{}", u32_at(b, 5)),
        0x2cd => format!("int/{}/{}", u32_at(b, 5), u32_at(b, 9)),
        0x2cf => format!(
            "int64/{}/{}",
            u32_at(b, 5),
            i64::from_le_bytes(b[9..17].try_into().unwrap())
        ),
        0xf7b0 => {
            assert_eq!(u32_at(b, 12), 0x28b);
            format!("notice/{}/{}", u32_at(b, 16), string_at(b, 20))
        }
        0x24 => "consume".into(),
        0xf755 => {
            assert_eq!(message.queue, 10);
            format!("script/{}", u32_at(b, 8))
        }
        0xf7e0 => format!("chat/{}", string_at(b, 4)),
        op => panic!("unexpected opcode {op:x}"),
    }
}
#[test]
fn committed_composite_matches_original_source_output_order_and_notices() {
    let mut count = 0;
    for line in include_str!("fixtures/skill_devices.trace")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let parts: Vec<_> = line.split('|').collect();
        let mode = parts[0].parse().unwrap();
        let skill = parts[1].parse().unwrap();
        let mut events = EventSequencer::new(binding(), 7);
        let mut actor = Sequences::new(32).unwrap();
        let mut items = BTreeMap::new();
        let batch = project_skill_device(
            &mut events,
            binding(),
            view(mode, skill),
            vec![InventoryProjection::Remove(EntityId(20))],
            &mut items,
            &mut actor,
            objects(),
            limits(),
        )
        .unwrap();
        assert_eq!(
            batch
                .owner
                .messages
                .iter()
                .map(trace)
                .collect::<Vec<_>>()
                .join(";"),
            parts[2],
            "{line}"
        );
        assert_eq!(events.next_sequence(), 8);
        assert_eq!(actor.current(SequenceKind::Skill, skill), 0);
        assert_eq!(batch.observers.len(), if mode >= 5 { 2 } else { 0 });
        if mode >= 5 {
            assert_eq!(
                batch.observers,
                batch.owner.messages[batch.owner.messages.len() - 2..]
            );
        }
        count += 1;
    }
    assert_eq!(count, 10);
}
#[test]
fn failed_consumption_or_final_notice_advances_no_canonical_counter() {
    let mut events = EventSequencer::new(binding(), u32::MAX);
    let mut actor = Sequences::new(32).unwrap();
    let mut items = BTreeMap::new();
    for max_bytes in [4096, 8] {
        assert!(
            project_skill_device(
                &mut events,
                binding(),
                view(5, 40),
                vec![InventoryProjection::Stack {
                    item: EntityId(20),
                    quantity: 1,
                    value: 2
                }],
                &mut items,
                &mut actor,
                objects(),
                BatchLimits {
                    max_bytes,
                    ..limits()
                }
            )
            .is_err()
        );
        assert_eq!(events.next_sequence(), u32::MAX);
        assert_eq!(actor.current(SequenceKind::Skill, 40), 255);
        assert_eq!(actor.current(SequenceKind::PropertyInt, 224), 255);
    }
    items.insert(EntityId(20), Sequences::new(8).unwrap());
    assert!(
        project_skill_device(
            &mut events,
            binding(),
            view(5, 40),
            vec![InventoryProjection::Stack {
                item: EntityId(20),
                quantity: 1,
                value: 2
            }],
            &mut items,
            &mut actor,
            objects(),
            BatchLimits {
                max_bytes: 120,
                ..limits()
            }
        )
        .is_err()
    );
    assert_eq!(
        items[&EntityId(20)].current(SequenceKind::PropertyInt, 12),
        255
    );
    assert_eq!(actor.current(SequenceKind::Skill, 40), 255);
    assert_eq!(events.next_sequence(), u32::MAX);
    let mut invalid = view(5, 40);
    invalid.change.after.details = Some(TraitDetails::Skill {
        initial_level: 10,
        resistance_at_last_check: 0,
        last_used_time: f64::NAN,
    });
    assert!(
        project_skill_device(
            &mut events,
            binding(),
            invalid,
            vec![InventoryProjection::Remove(EntityId(20))],
            &mut items,
            &mut actor,
            objects(),
            limits()
        )
        .is_err()
    );
    assert_eq!(events.next_sequence(), u32::MAX);
    project_skill_device(
        &mut events,
        binding(),
        view(5, 40),
        vec![InventoryProjection::Stack {
            item: EntityId(20),
            quantity: 1,
            value: 2,
        }],
        &mut items,
        &mut actor,
        objects(),
        limits(),
    )
    .unwrap();
    assert_eq!(events.next_sequence(), 0);
    assert_eq!(
        items[&EntityId(20)].current(SequenceKind::PropertyInt, 12),
        0
    );
}
#[test]
fn source_prompt_uses_enum_sentences_and_grouped_augmentation_cost() {
    assert_eq!(
        skill_device_prompt(
            41,
            SkillDeviceKind::Specialize,
            SkillAdvancement::Trained,
            8,
            "",
            0
        )
        .unwrap(),
        "This action will specialize your Two Handed Combat skill and cost 8 credits."
    );
    assert_eq!(
        skill_device_prompt(
            40,
            SkillDeviceKind::Augment,
            SkillAdvancement::Trained,
            0,
            "Test Gem",
            2_000_000_000
        )
        .unwrap(),
        "This action will augment your character with Test Gem and will cost 2,000,000,000 available experience."
    );
    let notice = skill_device_failure(28, SkillDeviceFailure::AugmentationNotTrained).unwrap();
    assert_eq!(notice.code, 0x55a);
    assert_eq!(
        notice.text.unwrap(),
        "You are not able to purchase this augmentation because you are not trained in Weapon Tinkering!"
    );
}
