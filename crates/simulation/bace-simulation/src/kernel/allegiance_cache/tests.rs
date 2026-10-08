use super::*;
use bace_character::*;
use bace_gameplay_api::*;
use bace_types::AccountId;
use std::sync::Arc;
fn setup() -> Kernel {
    setup_at_level(25)
}
fn setup_at_level(level: u32) -> Kernel {
    let mut k = crate::synthetic_scenario(1, 0).unwrap();
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let traits: Vec<_> = [35, 36, 44]
        .into_iter()
        .map(|id| TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(id),
                experience_spent: 0,
                advancement: SkillAdvancement::Trained,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.,
            },
        })
        .collect();
    let progression = CharacterProgression::with_state(
        &traits,
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        1000,
        1,
    )
    .unwrap()
    .with_training(
        Arc::new(
            SkillTrainingRules::new(
                &[35, 36, 44]
                    .into_iter()
                    .map(|skill| SkillCosts {
                        skill,
                        trained_cost: 2,
                        specialized_cost: 4,
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        ),
        20,
        &[],
    )
    .unwrap();
    k.register_character(
        CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
        },
        progression,
    )
    .unwrap();
    k.characters
        .register_native_services(
            EntityId(1),
            CharacterServiceState {
                level,
                total_experience: 0,
                titles: vec![],
                enlightenment: 0,
                sanctuary: None,
                total_skill_credits: None,
            },
            bace_quests::ContractRegistry::restore(vec![]).unwrap(),
        )
        .unwrap();
    let input = SkillValueInputs {
        formula: VitalFormula {
            enabled: true,
            divisor: 1,
            attribute1: 1,
            attribute2: 0,
        },
        usable_untrained: false,
        base_attributes: [100; 6],
        current_attributes: [100; 6],
        bonuses: Default::default(),
        multiplier: 1.,
        vitae: 1.,
        additive: 0,
    };
    k.register_character_skill_inputs(
        EntityId(1),
        crate::PreparedCharacterSkillInputs {
            attack_skill: 44,
            inputs: [35, 36, 44].into_iter().map(|id| (id, input)).collect(),
            shield: None,
            attribute_modifiers: [crate::PreparedAttributeModifier::default(); 6],
        },
    )
    .unwrap();
    let node = bace_allegiance::AllegianceNode {
        character: EntityId(1),
        account: AccountId(1),
        name: "Alice".into(),
        gender: 1,
        heritage: 1,
        patron: None,
        monarch: EntityId(1),
        vassals: vec![],
        rank: 1,
        followers: 0,
        level: 1,
        leadership: 7,
        loyalty: 8,
        sworn_at: 0,
        online_seconds: 1,
        may_pass_up: false,
        received_total: 0,
        tithed_total: 0,
        unclaimed: 0,
    };
    k.register_allegiances(
        bace_allegiance::AllegianceRegistry::restore(
            vec![node],
            vec![bace_allegiance::AllegianceMetadata::new(
                EntityId(1),
                0x80000001,
            )],
            0,
            8,
        )
        .unwrap(),
        0.,
    )
    .unwrap();
    k
}
#[test]
fn accepted_skill_xp_freezes_current_values_until_exact_ledger_commit() {
    let mut k = setup();
    k.enqueue(crate::Command::RaiseProgression {
        context: ActionContext {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
            sequence: 1,
        },
        request: RaiseProgression {
            target: ProgressionTarget::Skill(35),
            amount: 10,
        },
    })
    .unwrap();
    k.step().unwrap();
    let ticket = k.take_allegiance_proposal().unwrap();
    let after = ticket.patch.nodes[0].1.as_ref().unwrap();
    assert_eq!(
        (after.level, after.leadership, after.loyalty),
        (25, 101, 100)
    );
    assert_eq!(
        k.allegiances.registry.node(EntityId(1)).unwrap().leadership,
        7,
        "proposal is not ledger adoption"
    );
    // Later modifiers are not a source cache trigger and cannot rewrite frozen inputs.
    k.combat
        .skills
        .get_mut(&EntityId(1))
        .unwrap()
        .values
        .get_mut(&35)
        .unwrap()
        .current = 777;
    k.confirm_allegiance_committed(&ticket).unwrap();
    assert_eq!(
        k.allegiances.registry.node(EntityId(1)).unwrap().leadership,
        101
    );
    assert!(k.allegiances.cached_skills.is_empty());
}
#[test]
fn login_and_nonsource_changes_do_not_refresh_and_rollback_preserves_debt() {
    let mut k = setup();
    k.step().unwrap();
    assert!(
        k.take_allegiance_proposal().is_none(),
        "admission alone is not a GDLE cache trigger"
    );
    k.note_allegiance_skill_award(EntityId(1), 44, 10).unwrap();
    k.note_allegiance_skill_award(EntityId(1), 35, 0).unwrap();
    k.step().unwrap();
    assert!(k.take_allegiance_proposal().is_none());
    k.note_allegiance_skill_award(EntityId(1), 36, 1).unwrap();
    k.step().unwrap();
    let first = k.take_allegiance_proposal().unwrap();
    k.reject_allegiance_proposal(&first).unwrap();
    assert_eq!(k.allegiances.registry.node(EntityId(1)).unwrap().loyalty, 8);
    k.step().unwrap();
    let retry = k.take_allegiance_proposal().unwrap();
    assert_eq!(retry.patch, first.patch);
    assert_ne!(retry.operation, first.operation);
    k.confirm_allegiance_committed(&retry).unwrap();
    assert_eq!(
        k.allegiances.registry.node(EntityId(1)).unwrap().loyalty,
        100
    );
}
#[test]
fn original_cpp_cache_values_use_current_skills_without_the_passup_cap() {
    for line in include_str!("fixtures/gdle_cached_skills.csv")
        .lines()
        .skip(2)
    {
        let n: Vec<u32> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let mut k = setup_at_level(n[0]);
        let profile = k.combat.skills.get_mut(&EntityId(1)).unwrap();
        profile.values.get_mut(&35).unwrap().base = n[1];
        profile.values.get_mut(&35).unwrap().current = n[2];
        profile.values.get_mut(&36).unwrap().base = n[3];
        profile.values.get_mut(&36).unwrap().current = n[4];
        k.note_allegiance_skill_award(EntityId(1), 35, 1).unwrap();
        k.step_allegiance_cached_skills().unwrap();
        let ticket = k.take_allegiance_proposal().unwrap();
        let node = ticket.patch.nodes[0].1.as_ref().unwrap();
        assert_eq!(
            (node.level, node.leadership, node.loyalty),
            (n[5], n[6], n[7])
        );
        assert_eq!(n[8], 0);
    }
}

#[test]
fn accepted_specialization_and_unspecialization_leave_source_cache_unchanged() {
    let mut k = setup();
    let context = |sequence| ActionContext {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
        sequence,
    };
    let specialized = k
        .propose_skill(context(1), crate::SkillIntent::Specialize(35))
        .unwrap();
    k.confirm_skill_committed(specialized).unwrap();
    k.step().unwrap();
    assert!(k.take_allegiance_proposal().is_none());
    assert_eq!(
        k.allegiances.registry.node(EntityId(1)).unwrap().leadership,
        7
    );
    let lowered = k
        .propose_skill(
            context(2),
            crate::SkillIntent::Lower {
                skill: 35,
                wielded: vec![],
            },
        )
        .unwrap();
    k.confirm_skill_committed(lowered).unwrap();
    k.step().unwrap();
    assert!(k.take_allegiance_proposal().is_none());
    assert_eq!(
        k.allegiances.registry.node(EntityId(1)).unwrap().leadership,
        7
    );
}
