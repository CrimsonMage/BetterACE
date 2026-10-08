use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_gameplay_api::{AttributeId, ProgressionTarget, SkillAdvancement};
use std::sync::Arc;
#[test]
fn queued_read_remains_at_its_original_revision_while_live_owner_changes() {
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let mut owner = CharacterProgression::new(
        &[TraitProgress {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            experience_spent: 0,
            advancement: SkillAdvancement::Inactive,
        }],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        30,
        4,
    )
    .unwrap();
    let earlier = owner.read_snapshot();
    let change = owner.propose_experience_credit(10).unwrap();
    owner.adopt_experience_credit(change).unwrap();
    let later = owner.read_snapshot();
    assert_eq!(earlier.state().revision(), 4);
    assert_eq!(earlier.state().available_experience(), 30);
    assert_eq!(later.state().revision(), 5);
    assert_eq!(later.state().available_experience(), 40);
    assert!(
        earlier
            .state()
            .trait_states()
            .eq(later.state().trait_states())
    );
}
