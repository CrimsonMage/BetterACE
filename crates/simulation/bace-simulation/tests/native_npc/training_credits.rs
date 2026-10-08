use super::*;
#[test]
fn nullable_credit_changes_match_compiled_ace_and_reject_underflow() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../gameplay/bace-emotes/tests/fixtures/training_credits.csv"
    ));
    let optional = |s: &str| {
        if s == "null" {
            None
        } else {
            Some(s.parse::<i32>().unwrap())
        }
    };
    let mut count = 0;
    for row in fixture
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let f: Vec<_> = row.split(',').collect();
        let total = optional(f[0]);
        let available = optional(f[1]);
        let amount = f[2].parse().unwrap();
        let ranks = RankTable::new(&[0, 10]).unwrap();
        let mut character = CharacterProgression::new(
            &[],
            Arc::new(ProgressionTables {
                attributes: ranks.clone(),
                vitals: ranks.clone(),
                trained_skills: ranks.clone(),
                specialized_skills: ranks,
            }),
            0,
            0,
        )
        .unwrap();
        if let Some(credits) = available {
            character = character
                .with_training(
                    Arc::new(bace_character::SkillTrainingRules::new(&[]).unwrap()),
                    credits as u32,
                    &[],
                )
                .unwrap();
        }
        let mut state = bace_character::CharacterServiceState {
            level: 1,
            total_experience: 0,
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
            total_skill_credits: total,
        };
        let change = character.propose_training_credits(&state, amount).unwrap();
        assert_eq!(change.after_total, optional(f[3]));
        assert_eq!(change.after_available, optional(f[4]).map(|n| n as u32));
        let before = character.available_skill_credits();
        let mut wrong = change.clone();
        wrong.after_revision += 1;
        assert!(
            character
                .adopt_training_credits(&mut state, &wrong)
                .is_err()
        );
        assert_eq!(character.available_skill_credits(), before);
        character
            .adopt_training_credits(&mut state, &change)
            .unwrap();
        assert_eq!(character.available_skill_credits(), change.after_available);
        assert_eq!(state.total_skill_credits, change.after_total);
        if character.available_skill_credits() == Some(0) {
            assert!(character.propose_training_credits(&state, -1).is_err());
        }
        count += 1;
    }
    assert_eq!(count, 6);
}
#[test]
fn npc_credit_debit_uses_canonical_owner_and_joint_receipt() {
    let mut k = xp_kernel_with_credit(1.0, 1.0, Some(10), 0);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 47,
                amount: Some(-3),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let ticket = k.prepare_npc_training_credits(&proposal).unwrap();
    assert_eq!(ticket.change.after_available, Some(2));
    assert_eq!(ticket.change.after_total, Some(7));
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(5)
    );
    assert!(
        k.complete_npc_service(
            &proposal,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 }
        )
        .is_err()
    );
    let mut wrong = ticket.clone();
    wrong.change.after_total = Some(99);
    assert!(k.confirm_npc_training_credits_committed(&wrong).is_err());
    k.confirm_npc_training_credits_committed(&ticket).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(2)
    );
    assert_eq!(
        k.npc_character_services(EntityId(1))
            .unwrap()
            .total_skill_credits,
        Some(7)
    );
    assert!(k.confirm_npc_training_credits_committed(&ticket).is_err());
}

#[test]
fn npc_full_skill_reset_uses_existing_character_proposal_and_exact_receipt() {
    use bace_gameplay_api::{ProgressionTarget, SkillAdvancement, TraitDetails};
    let mut k = kernel(16);
    k.take_character(binding()).unwrap();
    let ranks = RankTable::new(&[0, 10, 50, 100]).unwrap();
    let character = CharacterProgression::with_state(
        &[bace_character::TraitState {
            progress: bace_character::TraitProgress {
                target: ProgressionTarget::Skill(6),
                advancement: SkillAdvancement::Trained,
                experience_spent: 50,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 27,
                last_used_time: 14.5,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        100,
        4,
    )
    .unwrap()
    .with_training(
        Arc::new(
            bace_character::SkillTrainingRules::new(&[bace_character::SkillCosts {
                skill: 6,
                trained_cost: 4,
                specialized_cost: 6,
            }])
            .unwrap(),
        ),
        20,
        &[],
    )
    .unwrap();
    k.register_character(binding(), character).unwrap();
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 110,
                stat: Some(6),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let ticket = k.prepare_npc_skill_reset(&proposal).unwrap();
    assert_eq!(
        k.character(EntityId(1))
            .unwrap()
            .projection(ProgressionTarget::Skill(6))
            .unwrap()
            .advancement,
        SkillAdvancement::Trained
    );
    // Existing compiled-original Player_Skills fixture: lower,6,2,0,1 ... 150,24.
    assert_eq!(ticket.change.after.advancement, SkillAdvancement::Untrained);
    assert_eq!(ticket.change.available_experience, 150);
    assert_eq!(ticket.change.available_skill_credits, 24);
    let mut wrong = ticket.clone();
    wrong.change.available_skill_credits += 1;
    assert!(k.confirm_npc_skill_reset_committed(&wrong).is_err());
    k.reject_npc_skill_reset(&ticket).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_experience(),
        100
    );
    let retry = k.prepare_npc_skill_reset(&proposal).unwrap();
    assert_eq!(retry, ticket);
    k.confirm_npc_skill_reset_committed(&retry).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_experience(),
        150
    );
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(24)
    );
    assert!(k.confirm_npc_skill_reset_committed(&retry).is_err());
}
