use bace_storage_codec::{npc_values_v1::*, npc_workflow_v1::*};
fn state() -> NpcCharacterStateV1 {
    NpcCharacterStateV1 {
        level: 1,
        total_experience: 0,
        total_skill_credits: None,
        titles: vec![5],
        enlightenment: 0,
        sanctuary: None,
    }
}
#[test]
fn frozen_effect_rejects_wrong_family_revision_and_unbounded_contracts() {
    let valid = NpcEffectV1::Property {
        actor: 1,
        aggregate: None,
        change: NpcPropertyChangeV1 {
            family: NpcPropertyFamilyV1::Int,
            stat: 4,
            before: None,
            after: Some(NpcValueV1::Int(2)),
            before_revision: 3,
            after_revision: 4,
        },
    };
    validate_npc_effect(&valid).unwrap();
    let mut invalid = valid.clone();
    if let NpcEffectV1::Property { change, .. } = &mut invalid {
        change.after = Some(NpcValueV1::String("wrong family".into()));
    }
    assert!(validate_npc_effect(&invalid).is_err());
    let mut invalid = valid;
    if let NpcEffectV1::Property { change, .. } = &mut invalid {
        change.after_revision = 5;
    }
    assert!(validate_npc_effect(&invalid).is_err());
    let too_many = NpcEffectV1::Contract {
        actor: 1,
        before_revision: 0,
        change: NpcContractChangeV1 {
            before: vec![],
            after: (1..=101)
                .map(|id| bace_storage_codec::ContractSaveV1 { id, display: false })
                .collect(),
        },
    };
    assert!(validate_npc_effect(&too_many).is_err());
}
#[test]
fn earned_experience_fences_entire_recipient_aggregate() {
    let mut before = state();
    before.total_skill_credits = Some(10);
    let mut after = before.clone();
    after.total_skill_credits = Some(11);
    after.total_experience = 100;
    after.level = 2;
    let mut effect = NpcEffectV1::EarnedExperience {
        actor: 1,
        change: NpcEarnedExperienceV1 {
            experience: NpcExperienceCreditV1 {
                before_revision: 7,
                after_revision: 8,
                before_available: 50,
                after_available: 150,
            },
            services: NpcCharacterChangeV1 {
                before_revision: 7,
                after_revision: 8,
                before,
                after,
            },
            before_skill_credits: Some(1),
            after_skill_credits: Some(2),
            earned_skill_credits: 1,
            credited: 100,
        },
    };
    validate_npc_effect(&effect).unwrap();
    let mut wrong = effect.clone();
    if let NpcEffectV1::EarnedExperience { change, .. } = &mut wrong {
        change.services.after.total_skill_credits = Some(10);
    }
    assert!(validate_npc_effect(&wrong).is_err());
    if let NpcEffectV1::EarnedExperience { change, .. } = &mut wrong {
        change.services.before.total_skill_credits = None;
        change.services.after.total_skill_credits = None;
    }
    validate_npc_effect(&wrong).unwrap();
    if let NpcEffectV1::EarnedExperience { change, .. } = &mut wrong {
        change.services.after.total_skill_credits = Some(1);
    }
    assert!(validate_npc_effect(&wrong).is_err());
    if let NpcEffectV1::EarnedExperience { change, .. } = &mut effect {
        change.services.after.total_experience = 101;
    }
    assert!(validate_npc_effect(&effect).is_err());
}

#[test]
fn queued_experience_phase_cannot_be_forged_into_applied_reward() {
    let mut pending = NpcPendingEffectV1 {
        ticket: 1,
        context: NpcContextV1 {
            source: 2,
            target: Some(1),
            operation: 4,
        },
        effect: NpcEffectV1::QueuedExperience {
            actor: 1,
            amount: 150,
            phase: NpcQueuedExperiencePhaseV1::AwaitingAdmission,
        },
        completion: NpcCompletionV1::Applied { post_delay: 0.0 },
        adopted: false,
        detached: false,
    };
    validate_npc_pending_effect(&pending).unwrap();
    pending.adopted = true;
    assert!(validate_npc_pending_effect(&pending).is_err());
    pending.adopted = false;
    pending.detached = true;
    assert!(validate_npc_pending_effect(&pending).is_err());
    if let NpcEffectV1::QueuedExperience { phase, .. } = &mut pending.effect {
        *phase = NpcQueuedExperiencePhaseV1::Ready;
    }
    validate_npc_pending_effect(&pending).unwrap();
}
