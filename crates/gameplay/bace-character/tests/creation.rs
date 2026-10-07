use bace_character::{
    CreationRejection, CreationRules, CreationRulesError, CreationSkillCosts, validate_attributes,
};
use bace_gameplay_api::{CreationAllocation, CreationAttributes, SkillAdvancement};

fn attributes(values: &[u32]) -> CreationAttributes {
    CreationAttributes {
        strength: values[0],
        endurance: values[1],
        coordination: values[2],
        quickness: values[3],
        focus: values[4],
        self_attribute: values[5],
    }
}

fn request() -> CreationAllocation {
    CreationAllocation {
        attributes: attributes(&[10; 6]),
        skills: [SkillAdvancement::Inactive; 55],
    }
}

#[test]
fn allocation_matches_official_attribute_and_skill_initialization_methods() {
    let mut count = 0;
    for line in include_str!("fixtures/creation.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let (kind, numbers) = line.split_once(',').unwrap();
        let values: Vec<u32> = numbers
            .split(',')
            .map(|number| number.parse().unwrap())
            .collect();
        if kind == "attr" {
            let outcome = validate_attributes(attributes(&values), values[6]);
            let expected = match values[7] {
                0 => Ok(()),
                1 => Err(CreationRejection::TooManyAttributeCredits),
                2 => Err(CreationRejection::AttributeOutOfRange),
                _ => panic!("unexpected ACE result"),
            };
            assert_eq!(outcome, expected, "{line}");
        } else {
            assert_eq!(kind, "skill");
            let mut costs = [None; 55];
            costs[6] = Some(CreationSkillCosts {
                trained: values[2],
                specialized: values[3],
            });
            let rules = CreationRules::new(60, values[1], costs).unwrap();
            let mut request = request();
            request.skills[6] = SkillAdvancement::try_from(values[0]).unwrap();
            let outcome = rules.validate(&request);
            assert_eq!(outcome.is_ok(), values[4] == 1, "{line}");
            if let Ok(allocation) = outcome {
                assert_eq!(allocation.available_skill_credits, values[5], "{line}");
                let skill = allocation.skills[6].unwrap();
                assert_eq!(skill.experience_spent, values[6], "{line}");
                assert_eq!(u32::from(skill.ranks), values[7], "{line}");
                assert_eq!(skill.initial_level, values[8], "{line}");
                assert_eq!(skill.advancement, request.skills[6]);
            }
        }
        count += 1;
    }
    assert_eq!(count, 320);
}

#[test]
fn attribute_and_skill_budgets_are_independent_and_allocated_atomically() {
    let mut costs = [None; 55];
    costs[6] = Some(CreationSkillCosts {
        trained: 4,
        specialized: 6,
    });
    costs[7] = Some(CreationSkillCosts {
        trained: 5,
        specialized: 7,
    });
    let rules = CreationRules::new(330, 14, costs).unwrap();
    let mut request = request();
    request.skills[6] = SkillAdvancement::Specialized;
    request.skills[7] = SkillAdvancement::Trained;
    let before = request.clone();
    assert_eq!(
        rules.validate(&request),
        Err(CreationRejection::InsufficientTrainingCredits(7))
    );
    assert_eq!(request, before);
    request.skills[7] = SkillAdvancement::Untrained;
    let valid = rules.validate(&request).unwrap();
    assert_eq!(valid.available_skill_credits, 4);
    assert_eq!(valid.total_skill_credits, 14);
    assert_eq!(valid.skills[7].unwrap().experience_spent, 0);
    assert_eq!(
        valid.skills[7].unwrap().advancement,
        SkillAdvancement::Untrained
    );
}

#[test]
fn inactive_unknown_skills_are_skipped_but_active_unknown_skills_fail() {
    let rules = CreationRules::new(60, 52, [None; 55]).unwrap();
    let mut request = request();
    assert!(
        rules
            .validate(&request)
            .unwrap()
            .skills
            .iter()
            .all(Option::is_none)
    );
    for advancement in [
        SkillAdvancement::Untrained,
        SkillAdvancement::Trained,
        SkillAdvancement::Specialized,
    ] {
        request.skills[54] = advancement;
        assert_eq!(
            rules.validate(&request),
            Err(CreationRejection::UnknownSkill(54))
        );
    }
}

#[test]
fn no_client_word_or_asset_cost_can_become_a_negative_credit() {
    for invalid in [4, 65536, u32::MAX] {
        assert!(SkillAdvancement::try_from(invalid).is_err());
    }
    assert_eq!(
        CreationRules::new(60, u32::MAX, [None; 55]),
        Err(CreationRulesError::CreditOutOfRange)
    );
    for cost in [
        CreationSkillCosts {
            trained: u32::MAX,
            specialized: 0,
        },
        CreationSkillCosts {
            trained: 0,
            specialized: u32::MAX,
        },
    ] {
        let mut skills = [None; 55];
        skills[6] = Some(cost);
        assert_eq!(
            CreationRules::new(60, 52, skills),
            Err(CreationRulesError::CreditOutOfRange)
        );
    }
}

#[test]
fn reported_locked_skill_divergence_is_explicitly_preserved_for_allocations() {
    // User divergence #36: pinned factory accepts Untrained for these skills.
    // This does not assert the retail witness or implement untraining later.
    let locked = [14, 15, 22, 24, 36, 40];
    let mut skills = [None; 55];
    let mut request = request();
    for id in locked {
        skills[id] = Some(CreationSkillCosts {
            trained: 0,
            specialized: 4,
        });
        request.skills[id] = SkillAdvancement::Untrained;
    }
    let result = CreationRules::new(60, 52, skills)
        .unwrap()
        .validate(&request)
        .unwrap();
    assert_eq!(result.available_skill_credits, 52);
    for id in locked {
        assert_eq!(
            result.skills[id].unwrap().advancement,
            SkillAdvancement::Untrained
        );
    }
}
