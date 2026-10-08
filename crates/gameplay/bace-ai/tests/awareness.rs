use bace_ai::{Awareness, AwarenessError, TargetCandidate};
#[test]
fn authority_range_faction_lock_and_overload() {
    let mut awareness = Awareness {
        current_target: Some(1),
        visual_range_squared: 100.0,
        chase_range_squared: 400.0,
        target_locked: false,
        monsters_only: false,
    };
    let mut target = TargetCandidate {
        actor: 1,
        distance_squared: 101.0,
        attackable: true,
        has_targeting_tactic: false,
        teleporting: false,
        same_faction: false,
        retaliate: false,
        player_or_combat_pet: true,
    };
    assert!(awareness.eligible(&target).unwrap());
    target.actor = 2;
    assert!(!awareness.eligible(&target).unwrap());
    target.distance_squared = 100.0;
    assert!(awareness.eligible(&target).unwrap());
    target.same_faction = true;
    assert!(!awareness.eligible(&target).unwrap());
    target.retaliate = true;
    assert!(awareness.eligible(&target).unwrap());
    awareness.target_locked = true;
    assert!(!awareness.eligible(&target).unwrap());
    awareness.target_locked = false;
    let mut output = Vec::new();
    assert_eq!(
        awareness.filter(&[target], &mut output),
        Err(AwarenessError::OutputCapacity)
    );
    target.distance_squared = f32::NAN;
    assert_eq!(
        awareness.eligible(&target),
        Err(AwarenessError::InvalidDistance)
    );
}
