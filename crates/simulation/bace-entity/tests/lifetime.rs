use bace_entity::{Combatant, CombatantProfile};
fn actor(player: bool) -> Combatant {
    Combatant::new(CombatantProfile {
        maximum_health: 100,
        melee_damage: 1,
        melee_range: 1.,
        attack_duration: 1.,
        strike_offsets: vec![0.5],
        player,
    })
    .unwrap()
}
#[test]
fn output_lifetime_cannot_be_rebound_or_confused_with_health_revision() {
    let mut old = actor(true);
    assert!(old.stamp_incarnation(0).is_err());
    old.stamp_incarnation(17).unwrap();
    old.stamp_incarnation(17).unwrap();
    assert!(old.stamp_incarnation(18).is_err());
    assert_eq!(old.incarnation(), 17);
    assert_eq!(old.revision(), 0);
    let mut new = actor(true);
    new.stamp_incarnation(18).unwrap();
    assert_eq!(
        new.revision(),
        old.revision(),
        "new health owners can start with equal revision counters"
    );
    assert_ne!(
        new.incarnation(),
        old.incarnation(),
        "queued output must check the lifetime as well"
    );
    let mut npc = actor(false);
    assert!(npc.stamp_incarnation(17).is_err());
    assert_eq!(npc.incarnation(), 0);
}

#[test]
fn source_creature_without_melee_assets_uses_an_explicit_disabled_capability() {
    let disabled = CombatantProfile {
        maximum_health: 100,
        melee_damage: 0,
        melee_range: 0.,
        attack_duration: 0.,
        strike_offsets: vec![],
        player: false,
    };
    assert!(Combatant::new(disabled.clone()).is_ok());
    for partial in [
        CombatantProfile {
            melee_damage: 1,
            ..disabled.clone()
        },
        CombatantProfile {
            melee_range: 1.,
            ..disabled.clone()
        },
        CombatantProfile {
            attack_duration: 1.,
            ..disabled.clone()
        },
        CombatantProfile {
            strike_offsets: vec![0.],
            ..disabled.clone()
        },
        CombatantProfile {
            melee_range: f32::NAN,
            ..disabled
        },
    ] {
        assert!(Combatant::new(partial).is_err());
    }
}
