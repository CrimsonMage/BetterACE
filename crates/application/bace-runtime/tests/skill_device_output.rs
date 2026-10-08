//! Pinned ACE device use/confirmation scenarios; numerical transition oracles
//! live in bace-character. Revisions/receipts are BetterACE durability hardening.
use bace_character::*;
use bace_gameplay_api::*;
use bace_inventory::*;
use bace_simulation::*;
use bace_types::{AccountId, EntityId};
use std::sync::Arc;
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
fn item(id: u32, equipped: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 100,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: id - 10,
            equipped,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 1,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
fn kernel(skill: u32) -> Kernel {
    let ranks = RankTable::new(&[0, 10, 50, 100]).unwrap();
    let c = CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(skill),
                advancement: SkillAdvancement::Trained,
                experience_spent: 50,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        1000,
        4,
    )
    .unwrap()
    .with_training(
        Arc::new(
            SkillTrainingRules::new(&[SkillCosts {
                skill,
                trained_cost: 4,
                specialized_cost: 6,
            }])
            .unwrap(),
        ),
        20,
        &[],
    )
    .unwrap();
    let mut k = synthetic_scenario(1, 0).unwrap();
    k.register_character(
        CharacterBinding {
            session: SessionId(1),
            account: AccountId(1),
            actor: EntityId(1),
        },
        c,
    )
    .unwrap();
    k.register_inventory_container(InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 20,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(item(10, 0)).unwrap();
    k
}
fn receipt(t: &SkillDeviceTicket) -> InventoryReceipt {
    InventoryReceipt {
        operation: t.inventory.operation,
        revisions: t
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    }
}
use bace_replication::{BatchLimits, EventSequencer};
use bace_runtime::skill_device_output::{
    SkillDeviceAdmissionError, SkillDeviceConfirmations, SkillDeviceOutputError, SkillDevicePrompt,
};
use bace_session::{SessionState, decode_skill_device};
use bace_wire::{GameActionEnvelope, GameEventEnvelope, Reader, opcode::GameActionType};
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 8,
        max_bytes: 10000,
        max_message_bytes: 4096,
        max_string_bytes: 4096,
    }
}
fn decode(
    sequence: u32,
    action: GameActionType,
    payload: &[u8],
) -> bace_session::DispatchedSkillDevice {
    let bytes = GameActionEnvelope {
        sequence,
        action,
        payload,
    }
    .encode(128)
    .unwrap();
    decode_skill_device(SessionState::WorldConnected, context(sequence), &bytes, 128).unwrap()
}
#[test]
fn use_prompt_response_pipeline_binds_type_token_and_waits_for_durable_receipt() {
    for (device, skill, wire_type) in [
        (PreparedSkillDevice::Specialize(31), 31, 2u32),
        (
            PreparedSkillDevice::Augment {
                skill: 40,
                experience_cost: 100,
            },
            40,
            6u32,
        ),
    ] {
        let mut kernel = kernel(skill);
        kernel
            .register_skill_device(EntityId(10), 1, device)
            .unwrap();
        let mut prompts = SkillDeviceConfirmations::new(8);
        let mut sequencer = EventSequencer::new(binding(), 1);
        let request = decode(1, GameActionType::Use, &10u32.to_le_bytes());
        prompts
            .submit_request(&request, 0, 100, |command| kernel.enqueue(command))
            .unwrap();
        kernel.step().unwrap();
        let outcome = kernel.take_skill_device_outcome().unwrap();
        let batch = prompts
            .project(
                &mut sequencer,
                binding(),
                SkillDevicePrompt {
                    outcome: &outcome,
                    text: "Apply skill device?",
                    now: kernel.ticks(),
                },
                limits(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(batch.messages.len(), 1);
        let envelope = GameEventEnvelope::decode(&batch.messages[0].bytes, 4096).unwrap();
        assert_eq!(envelope.event.0, 0x274);
        let mut reader = Reader::new(envelope.payload);
        assert_eq!(reader.u32().unwrap(), wire_type);
        let token = reader.u32().unwrap();
        // The synthetic actor moves during the tick, independently dirtying its
        // saved pose. Rejected confirmation must preserve the current revision.
        let before_confirmation = kernel.character(EntityId(1)).unwrap().revision();
        let wrong_payload = [
            if wire_type == 2 { 6u32 } else { 2u32 }.to_le_bytes(),
            token.to_le_bytes(),
            1u32.to_le_bytes(),
        ]
        .concat();
        let wrong = decode(2, GameActionType::ConfirmationResponse, &wrong_payload);
        let reject =
            |_: Command| -> Result<(), ()> { panic!("rejected confirmation reached admission") };
        assert_eq!(
            prompts.submit_request(&wrong, 1, 100, reject),
            Err(SkillDeviceAdmissionError::Request(
                SkillDeviceOutputError::InvalidConfirmation
            ))
        );
        assert!(kernel.take_skill_device_proposal().is_none());
        assert_eq!(
            kernel.character(EntityId(1)).unwrap().revision(),
            before_confirmation
        );
        let payload = [
            wire_type.to_le_bytes(),
            token.to_le_bytes(),
            1u32.to_le_bytes(),
        ]
        .concat();
        let request = decode(2, GameActionType::ConfirmationResponse, &payload);
        let mut wrong_session = request;
        wrong_session.context.session = SessionId(99);
        assert_eq!(
            prompts.submit_request(&wrong_session, 1, 100, reject),
            Err(SkillDeviceAdmissionError::Request(
                SkillDeviceOutputError::WrongBinding
            ))
        );
        assert_eq!(
            prompts.submit_request(&request, 100, 100, reject),
            Err(SkillDeviceAdmissionError::Request(
                SkillDeviceOutputError::Expired
            ))
        );
        assert_eq!(
            prompts.submit_request(&request, 1, 100, |_| Err("full")),
            Err(SkillDeviceAdmissionError::Admission("full"))
        );
        assert_eq!(prompts.len(), 1);
        prompts
            .submit_request(&request, 1, 100, |command| kernel.enqueue(command))
            .unwrap();
        assert!(prompts.is_empty());
        kernel.step().unwrap();
        let outcome = kernel.take_skill_device_outcome().unwrap();
        assert!(matches!(
            outcome.result,
            Ok(SkillDeviceResult::Proposed(Some(_)))
        ));
        assert!(
            prompts
                .project(
                    &mut sequencer,
                    binding(),
                    SkillDevicePrompt {
                        outcome: &outcome,
                        text: "",
                        now: 2
                    },
                    limits()
                )
                .unwrap()
                .is_none()
        );
        assert!(kernel.inventory_item(EntityId(10)).is_some());
        assert_eq!(
            kernel.character(EntityId(1)).unwrap().revision(),
            before_confirmation
        );
        let ticket = kernel.take_skill_device_proposal().unwrap();
        kernel
            .confirm_skill_device_committed(&receipt(&ticket))
            .unwrap();
        assert!(kernel.inventory_item(EntityId(10)).is_none());
        assert_eq!(
            kernel.character(EntityId(1)).unwrap().revision(),
            before_confirmation + 1
        );
        assert_eq!(
            prompts.submit_request(&request, 2, 100, reject),
            Err(SkillDeviceAdmissionError::Request(
                SkillDeviceOutputError::InvalidConfirmation
            ))
        );
    }
}
fn golden(name: &str) -> Vec<u8> {
    let line = include_str!("../../../network/bace-wire/tests/fixtures/crafting.tsv")
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
fn device_prompt_matches_official_csharp_and_rejects_unrepresentable_token_atomically() {
    let binding = CharacterBinding {
        actor: EntityId(0x50000001),
        ..binding()
    };
    for (name, device) in [
        ("confirm-alter-skill", PreparedSkillDevice::Lower(31)),
        (
            "confirm-augmentation",
            PreparedSkillDevice::Augment {
                skill: 40,
                experience_cost: 100,
            },
        ),
    ] {
        let mut prompts = SkillDeviceConfirmations::new(1);
        let mut sequence = EventSequencer::new(binding, 9);
        let mut outcome = SkillDeviceOutcome {
            context: Some(ActionContext {
                actor: binding.actor,
                ..context(1)
            }),
            result: Ok(SkillDeviceResult::Confirmation(SkillDeviceConfirmation {
                token: u64::from(u32::MAX) + 1,
                actor: binding.actor,
                item: EntityId(10),
                expires: 100,
                device,
            })),
        };
        assert_eq!(
            prompts.project(
                &mut sequence,
                binding,
                SkillDevicePrompt {
                    outcome: &outcome,
                    text: "Apply skill device?",
                    now: 1
                },
                limits()
            ),
            Err(SkillDeviceOutputError::InvalidConfirmation)
        );
        assert_eq!(sequence.next_sequence(), 9);
        assert!(prompts.is_empty());
        let Ok(SkillDeviceResult::Confirmation(ref mut quote)) = outcome.result else {
            panic!()
        };
        quote.token = 17;
        let batch = prompts
            .project(
                &mut sequence,
                binding,
                SkillDevicePrompt {
                    outcome: &outcome,
                    text: "Apply skill device?",
                    now: 1,
                },
                limits(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(batch.messages[0].bytes, golden(name));
    }
}
