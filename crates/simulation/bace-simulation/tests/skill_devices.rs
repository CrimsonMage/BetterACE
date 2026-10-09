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
#[test]
fn confirmed_skill_device_cooldown_waits_for_exact_durable_receipt() {
    // ACE EnchantmentRegistry.StartCooldown uses spell 0x8000 | SharedCooldown,
    // caster=device, layer one and the authored CooldownDuration.
    let mut k = kernel(31);
    k.register_magic_registry(
        EntityId(1),
        bace_magic::EnchantmentRegistry::new(4096).unwrap(),
        true,
    )
    .unwrap();
    k.register_skill_device_with_cooldown(
        EntityId(10),
        1,
        PreparedSkillDevice::Specialize(31),
        Some(ActivationRequirements {
            cooldown: Some(7),
            ..Default::default()
        }),
        Some(30.0),
    )
    .unwrap();
    let quote = k
        .request_skill_device(context(1), EntityId(10), 100)
        .unwrap();
    assert_eq!(
        k.register_skill_device_with_cooldown(
            EntityId(10),
            1,
            PreparedSkillDevice::Specialize(31),
            Some(ActivationRequirements {
                cooldown: Some(8),
                ..Default::default()
            }),
            Some(60.0),
        ),
        Err(SkillDeviceError::Busy),
        "authored cooldown cannot change behind an outstanding quote"
    );
    let ticket = k
        .confirm_skill_device(context(2), quote.token, true)
        .unwrap()
        .unwrap();
    let cooldown = ticket.cooldown.as_ref().unwrap();
    assert_eq!((cooldown.group, cooldown.seconds), (7, 30.0));
    assert_eq!(cooldown.after.last().unwrap().spell, 0x8007);
    assert!(k.magic_registry(EntityId(1)).unwrap().entries().is_empty());
    assert!(k.take_magic_event().is_none());
    assert_eq!(k.take_skill_device_proposal(), Some(ticket.clone()));
    let mut forged = receipt(&ticket);
    forged.revisions[0].1 += 1;
    assert!(k.confirm_skill_device_committed(&forged).is_err());
    assert!(k.magic_registry(EntityId(1)).unwrap().entries().is_empty());
    k.confirm_skill_device_committed(&receipt(&ticket)).unwrap();
    let entry = &k.magic_registry(EntityId(1)).unwrap().entries()[0];
    assert_eq!(
        (
            entry.spell,
            entry.caster,
            entry.spec.layer,
            entry.spec.duration
        ),
        (0x8007, 10, 1, 30.0)
    );
    assert!(
        matches!(k.take_magic_event(), Some(MagicEvent::Enchantment { actor, entry })
        if actor == EntityId(1) && entry.spell == 0x8007 && entry.caster == 10)
    );
    k.register_inventory_item(item(11, 0)).unwrap();
    k.register_skill_device_with_cooldown(
        EntityId(11),
        1,
        PreparedSkillDevice::Lower(31),
        Some(ActivationRequirements {
            cooldown: Some(7),
            ..Default::default()
        }),
        Some(30.0),
    )
    .unwrap();
    assert_eq!(
        k.request_skill_device(context(3), EntityId(11), 100),
        Err(SkillDeviceError::Activation(ActivationFailure::Cooldown))
    );
}

#[test]
fn cooldown_only_device_requires_registry_and_cannot_bypass_item_ownership() {
    let mut k = kernel(31);
    k.register_skill_device_with_cooldown(
        EntityId(10),
        1,
        PreparedSkillDevice::Specialize(31),
        Some(ActivationRequirements {
            cooldown: Some(7),
            ..Default::default()
        }),
        Some(30.0),
    )
    .unwrap();
    assert_eq!(
        k.request_skill_device(context(1), EntityId(10), 100),
        Err(SkillDeviceError::Activation(
            ActivationFailure::MissingValue
        ))
    );
    let mut foreign = context(2);
    foreign.actor = EntityId(2);
    assert!(k.request_skill_device(foreign, EntityId(10), 100).is_err());
    let mut mixed = kernel(31);
    mixed
        .register_magic_registry(
            EntityId(1),
            bace_magic::EnchantmentRegistry::new(4096).unwrap(),
            true,
        )
        .unwrap();
    mixed
        .register_skill_device_with_cooldown(
            EntityId(10),
            1,
            PreparedSkillDevice::Specialize(31),
            Some(ActivationRequirements {
                cooldown: Some(7),
                level: Some(50),
                ..Default::default()
            }),
            Some(30.0),
        )
        .unwrap();
    assert_eq!(
        mixed.request_skill_device(context(1), EntityId(10), 100),
        Err(SkillDeviceError::Activation(
            ActivationFailure::MissingValue
        )),
        "mixed requirements must retain the generic authoritative value check"
    );
}

#[test]
fn cooldown_only_device_matches_pinned_ace_retained_entry_boundary() {
    // Pinned ACE EnchantmentManager.GetCooldown returns the f32 cast of
    // Duration - Abs(StartTime); CheckCooldown accepts only exact zero (or
    // no row). The generic inventory activation path uses the same check.
    let request = |start_time: Option<f64>| {
        let mut k = kernel(31);
        let registry = if let Some(start_time) = start_time {
            let mut entries = registry(30.0).into_entries();
            entries[0].spell = 0x8007;
            entries[0].start_time = start_time;
            EnchantmentRegistry::restore(16, 20, entries).unwrap()
        } else {
            EnchantmentRegistry::new(16).unwrap()
        };
        k.register_magic_registry(EntityId(1), registry, true)
            .unwrap();
        k.register_skill_device_with_cooldown(
            EntityId(10),
            1,
            PreparedSkillDevice::Specialize(31),
            Some(ActivationRequirements {
                cooldown: Some(7),
                ..Default::default()
            }),
            Some(30.0),
        )
        .unwrap();
        k.request_skill_device(context(1), EntityId(10), 100)
    };
    assert!(request(None).is_ok(), "no cooldown row");
    assert_eq!(
        request(Some(-29.0)),
        Err(SkillDeviceError::Activation(ActivationFailure::Cooldown)),
        "active cooldown row"
    );
    assert!(request(Some(-30.0)).is_ok(), "exact source boundary");
    assert_eq!(
        request(Some(-31.0)),
        Err(SkillDeviceError::Activation(ActivationFailure::Cooldown)),
        "retained overrun row is not an absent row in pinned ACE"
    );
}

#[test]
fn specialize_confirmation_combines_consumption_and_skill_after_exact_durable_receipt() {
    let mut k = kernel(31);
    k.register_skill_device(
        EntityId(10),
        1,
        PreparedSkillDevice::alteration(1, 31).unwrap(),
    )
    .unwrap();
    let quote = k
        .request_skill_device(context(1), EntityId(10), 100)
        .unwrap();
    let t = k
        .confirm_skill_device(context(2), quote.token, true)
        .unwrap()
        .unwrap();
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 4);
    assert!(k.inventory_item(EntityId(10)).is_some());
    assert!(k.confirm_skill_device_committed(&receipt(&t)).is_err()); // unsubmitted
    assert!(k.confirm_skill_committed(t.skill).is_err()); // cannot split operation
    assert!(k.confirm_inventory_committed(&receipt(&t)).is_err());
    assert!(k.take_inventory_proposal().is_none());
    assert_eq!(k.take_skill_device_proposal(), Some(t.clone()));
    let mut forged = receipt(&t);
    forged.revisions[0].1 += 1;
    assert!(k.confirm_skill_device_committed(&forged).is_err());
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 4);
    k.retry_skill_device(t.inventory.operation).unwrap();
    assert_eq!(k.take_skill_device_proposal(), Some(t.clone()));
    k.confirm_skill_device_committed(&receipt(&t)).unwrap();
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 5);
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(14)
    );
    assert!(k.inventory_item(EntityId(10)).is_none());
    assert!(!k.has_skill_device_work());
    assert!(k.confirm_skill_device_committed(&receipt(&t)).is_err());
}
#[test]
fn lower_rechecks_all_four_wield_requirements_after_confirmation() {
    for slot in 0..4 {
        let mut k = kernel(31);
        k.register_skill_device(
            EntityId(10),
            1,
            PreparedSkillDevice::alteration(2, 31).unwrap(),
        )
        .unwrap();
        let quote = k
            .request_skill_device(context(1), EntityId(10), 100)
            .unwrap();
        k.register_inventory_item(item(11, 1)).unwrap();
        assert_eq!(
            k.confirm_skill_device(context(2), quote.token, true),
            Err(SkillDeviceError::MissingWieldProfile)
        );
        let mut requirements = [None; 4];
        requirements[slot] = Some(SkillWieldRequirement::RawSkill(31));
        k.register_skill_wield_requirements(EntityId(11), 1, requirements)
            .unwrap();
        assert!(matches!(
            k.confirm_skill_device(context(3), quote.token, true),
            Err(SkillDeviceError::Skill(SkillActionError::Domain(
                SkillTransitionError::WieldRequirement
            )))
        ));
        assert!(k.inventory_item(EntityId(10)).is_some());
        assert_eq!(k.character(EntityId(1)).unwrap().revision(), 4);
    }
}
#[test]
fn decline_wrong_session_and_profile_change_do_not_spend() {
    let mut k = kernel(31);
    k.register_skill_device(EntityId(10), 1, PreparedSkillDevice::Specialize(31))
        .unwrap();
    let q = k
        .request_skill_device(context(1), EntityId(10), 100)
        .unwrap();
    let mut forged = context(2);
    forged.session = SessionId(2);
    assert_eq!(
        k.confirm_skill_device(forged, q.token, true),
        Err(SkillDeviceError::Confirmation)
    );
    assert_eq!(
        k.register_skill_device(EntityId(10), 1, PreparedSkillDevice::Lower(31)),
        Err(SkillDeviceError::Busy),
        "an outstanding quote pins the authored device profile"
    );
    assert_eq!(k.confirm_skill_device(context(3), q.token, false), Ok(None));
    k.register_skill_device(EntityId(10), 1, PreparedSkillDevice::Lower(31))
        .unwrap();
    let next = k
        .request_skill_device(context(4), EntityId(10), 100)
        .unwrap();
    assert_eq!(next.device, PreparedSkillDevice::Lower(31));
    assert_eq!(
        k.confirm_skill_device(context(5), next.token, false),
        Ok(None)
    );
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 4);
    assert!(k.inventory_item(EntityId(10)).is_some());
}
#[test]
fn augmentation_and_definite_rollback_retain_all_before_state() {
    for (kind, skill) in [(7, 40), (8, 18), (9, 29), (10, 30), (11, 28)] {
        let mut k = kernel(skill);
        k.register_skill_device(
            EntityId(10),
            1,
            PreparedSkillDevice::augmentation(kind, 100).unwrap(),
        )
        .unwrap();
        let q = k
            .request_skill_device(context(1), EntityId(10), 100)
            .unwrap();
        let t = k
            .confirm_skill_device(context(2), q.token, true)
            .unwrap()
            .unwrap();
        assert!(t.skill.change.augmentation_added);
        k.reject_skill_device(t.inventory.operation).unwrap();
        assert_eq!(k.character(EntityId(1)).unwrap().revision(), 4);
        assert_eq!(
            k.character(EntityId(1)).unwrap().available_experience(),
            1000
        );
        assert_eq!(
            k.character(EntityId(1)).unwrap().augmented_skills().count(),
            0
        );
        assert!(k.inventory_item(EntityId(10)).is_some());
        assert!(k.take_skill_device_proposal().is_none());
    }
    assert!(PreparedSkillDevice::augmentation(1, 100).is_err());
    assert!(PreparedSkillDevice::augmentation(7, -1).is_err());
}

use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
};
fn registry(duration: f64) -> EnchantmentRegistry {
    EnchantmentRegistry::restore(
        16,
        20,
        vec![EnchantmentEntry {
            spell: 123,
            caster: 100,
            school: MagicSchool::Creature,
            spec: EnchantmentSpec {
                category: 23,
                power: 100,
                duration,
                layer: 7,
                stat_type: 0x2008000,
                stat_key: 7,
                value: 12.5,
                beneficial: true,
                set_id: None,
            },
            start_time: -15.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: EnchantmentMetadata {
                enchantment_category: 12,
                degrade_modifier: 0.1,
                degrade_limit: 0.2,
                last_time_degraded: -3.0,
                ..Default::default()
            },
        }],
    )
    .unwrap()
}

#[test]
fn device_and_compacted_sibling_registries_are_frozen_and_retired_with_combined_receipt() {
    let mut k = kernel(31);
    k.register_inventory_item(item(11, 0)).unwrap();
    for id in [10, 11] {
        k.register_magic_registry(EntityId(id), registry(60.0), true)
            .unwrap();
    }
    k.register_skill_device(EntityId(10), 1, PreparedSkillDevice::Specialize(31))
        .unwrap();
    let q = k
        .request_skill_device(context(1), EntityId(10), 100)
        .unwrap();
    k.confirm_skill_device(context(2), q.token, true).unwrap();
    let t = k.take_skill_device_proposal().unwrap();
    for _ in 0..300 {
        k.step().unwrap();
    }
    for id in [10, 11] {
        assert_eq!(
            k.magic_registry(EntityId(id)).unwrap().entries()[0].start_time,
            -15.0
        );
    }
    k.confirm_skill_device_committed(&receipt(&t)).unwrap();
    assert!(k.magic_registry(EntityId(10)).is_none());
    k.step().unwrap();
    assert_eq!(
        k.magic_registry(EntityId(11)).unwrap().entries()[0].start_time,
        -25.0
    );
}

#[test]
fn prepared_device_rejects_stale_or_missing_wield_evidence_without_consuming_action() {
    let mut k = kernel(18);
    k.register_inventory_item(item(11, 1)).unwrap();
    let request = |wielded| {
        Command::SkillDevice(SkillDeviceCommand::RequestPrepared {
            activation: None,
            cooldown_seconds: None,
            context: context(1),
            item: EntityId(10),
            revision: 1,
            device: PreparedSkillDevice::Lower(18),
            wielded,
            lifetime: 1800,
        })
    };
    k.enqueue(request(vec![])).unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_skill_device_outcome().unwrap().result,
        Err(SkillDeviceError::MissingWieldProfile)
    ));
    k.enqueue(request(vec![(EntityId(11), 2, [None; 4])]))
        .unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_skill_device_outcome().unwrap().result,
        Err(SkillDeviceError::Stale)
    ));
    k.enqueue(request(vec![(EntityId(11), 1, [None; 4])]))
        .unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_skill_device_outcome().unwrap().result,
        Ok(SkillDeviceResult::Confirmation(_))
    ));
    assert_eq!(
        k.character(EntityId(1))
            .unwrap()
            .projection(ProgressionTarget::Skill(18))
            .unwrap()
            .advancement,
        SkillAdvancement::Trained
    );
}
#[test]
fn prepared_device_bounds_do_not_admit_large_source_vectors() {
    let mut k = kernel(18);
    let command = Command::SkillDevice(SkillDeviceCommand::RequestPrepared {
        activation: None,
        cooldown_seconds: None,
        context: context(1),
        item: EntityId(10),
        revision: 1,
        device: PreparedSkillDevice::Lower(18),
        wielded: vec![(EntityId(11), 1, [None; 4]); 1024],
        lifetime: 1800,
    });
    assert!(k.try_enqueue(command).is_err());
    assert!(k.take_skill_device_outcome().is_none());
}

#[test]
fn prepared_activation_requirements_reach_authoritative_owner_before_quote() {
    let mut k = kernel(18);
    k.register_skill_device_with_activation(
        EntityId(10),
        1,
        PreparedSkillDevice::Lower(18),
        Some(ActivationRequirements {
            level: Some(50),
            ..ActivationRequirements::default()
        }),
    )
    .unwrap();
    assert!(matches!(
        k.request_skill_device(context(1), EntityId(10), 1800),
        Err(SkillDeviceError::Activation(
            ActivationFailure::MissingValue
        ))
    ));
    assert!(k.take_skill_device_outcome().is_none());
}

#[test]
fn inactive_device_use_authorizes_without_quote_cooldown_or_item_mutation() {
    // Pinned ACE WorldObject_Use.OnActivate returns immediately for Int119=0;
    // Player_Use.TryUseItem still schedules UseDone after that return.
    let mut k = kernel(18);
    let before_item = k.inventory_item(EntityId(10)).unwrap().clone();
    let mut idle = kernel(18);
    idle.step().unwrap();
    let command = |sequence, revision| {
        Command::SkillDevice(SkillDeviceCommand::RequestInactive {
            context: context(sequence),
            item: EntityId(10),
            revision,
        })
    };
    k.enqueue(command(1, 1)).unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_skill_device_outcome().unwrap().result,
        Ok(SkillDeviceResult::Inactive)
    ));
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        idle.character(EntityId(1)).unwrap().revision(),
        "inactive Use must not add a character mutation beyond the normal tick"
    );
    assert_eq!(k.inventory_item(EntityId(10)), Some(&before_item));
    assert!(k.take_skill_device_proposal().is_none());
    k.enqueue(command(2, 2)).unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_skill_device_outcome().unwrap().result,
        Err(SkillDeviceError::Stale)
    ));
    k.enqueue(command(1, 1)).unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_skill_device_outcome().unwrap().result,
        Err(SkillDeviceError::Ownership)
    ));
    assert!(k.take_skill_device_proposal().is_none());
}
