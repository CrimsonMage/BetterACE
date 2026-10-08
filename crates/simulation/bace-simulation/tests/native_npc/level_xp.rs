use super::*;
#[test]
fn level_awards_follow_compiled_ace_rounding_clamps_and_allegiance_policy() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../gameplay/bace-emotes/tests/fixtures/level_xp.csv"
    ));
    let mut count = 0;
    for row in fixture
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let fields: Vec<_> = row.split(',').collect();
        let next = fields[0].parse().unwrap();
        let percent = fields[1].parse().unwrap();
        let minimum = fields[2].parse().unwrap();
        let maximum = fields[3].parse().unwrap();
        let expected = fields[4].parse::<i64>().unwrap();
        assert_eq!(fields[5], "Allegiance");
        assert_eq!(
            bace_character::level_proportional_xp(next, percent, minimum, maximum).unwrap(),
            expected
        );
        if next == 100 {
            let mut k = xp_kernel();
            k.configure_social_experience(Arc::new(
                bace_character::CharacterLevelTable::prepare(
                    vec![0, 0, 100, 300],
                    vec![0, 0, 0, 0],
                )
                .unwrap(),
            ))
            .unwrap();
            install(
                &mut k,
                vec![set(
                    7,
                    None,
                    vec![EmoteAction {
                        r#type: 49,
                        percent: Some(percent),
                        min64: Some(minimum),
                        max64: Some(maximum),
                        ..Default::default()
                    }],
                )],
            );
            assert!(k.step().unwrap().is_empty());
            let proposal = k.take_npc_proposal().unwrap();
            assert!(
                matches!(proposal.effect,NpcEffect::QueuedExperience{amount,share:bace_simulation::NpcExperienceSharing::Allegiance,..}if amount==expected as u64)
            );
            k.admit_npc_queued_experience(&proposal).unwrap();
            assert!(k.step().unwrap().is_empty());
            let ticket = k.take_allegiance_proposal().unwrap();
            assert_eq!(
                k.npc_character_services(EntityId(1))
                    .unwrap()
                    .total_experience,
                0
            );
            k.confirm_allegiance_committed(&ticket).unwrap();
            assert_eq!(
                k.npc_character_services(EntityId(1))
                    .unwrap()
                    .total_experience,
                expected as u64
            );
        }
        count += 1;
    }
    assert_eq!(count, 7);
}
