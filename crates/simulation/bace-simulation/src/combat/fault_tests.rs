use super::*;
#[test]
fn reported_melee_fault_retires_action_but_preserves_attack_deadline() {
    let (mut combat, mut world) = fixture();
    for id in 1..=2 {
        combat
            .register_physical(EntityId(id), Arc::new(prepared(id == 1)), &world)
            .unwrap();
    }
    combat
        .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(2), 0.)
        .unwrap();
    combat
        .apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: 0.5,
            },
            0.,
        )
        .unwrap();
    // Model an already-reported source failure; retirement is a separate owner
    // transition and must not depend on the client sending another cancel.
    combat
        .physical_attacks
        .get_mut(&EntityId(1))
        .unwrap()
        .faulted = true;
    combat.step_internal(&mut world, 0.05, None);
    assert!(!combat.active(EntityId(1)));
    assert!(combat.recovery_pending(EntityId(1), 0.05));
    assert_eq!(world.combatant(EntityId(2)).unwrap().health(), 100);
    assert_eq!(
        combat.apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMelee {
                target: EntityId(2),
                height: 2,
                power: 0.5
            },
            0.05
        ),
        Err(CombatRejection::Busy)
    );
}
#[test]
fn faulted_missile_cannot_discard_submitted_ammunition_until_definite_resolution() {
    let (mut combat, mut world) = fixture();
    let mut p = prepared(true);
    let mut launcher = p.main.clone().unwrap();
    launcher.entity = 101;
    let mut ammo = launcher.clone();
    ammo.entity = 202;
    ammo.revision = 5;
    p.launcher = Some(launcher);
    p.ammunition = Some(ammo);
    p.missile = Some(PhysicalMissileSpec {
        ammunition_count: 1,
        speed: 20.,
        radius: 0.1,
        gravity: false,
        tracking: false,
        attack_motion: 0x4000001e,
        launch_seconds: 0.,
        duration_seconds: 0.5,
        damage_modifier: 1.,
    });
    combat
        .register_physical(EntityId(1), Arc::new(p), &world)
        .unwrap();
    combat
        .register_physical(EntityId(2), Arc::new(prepared(false)), &world)
        .unwrap();
    combat.supply_missile_id(EntityId(1000), &world).unwrap();
    combat
        .apply(&mut world, EntityId(1), CombatRequest::ChangeMode(4), 0.)
        .unwrap();
    combat
        .apply(
            &mut world,
            EntityId(1),
            CombatRequest::TargetedMissile {
                target: EntityId(2),
                height: 2,
                accuracy: 0.5,
            },
            0.,
        )
        .unwrap();
    combat.step_internal(&mut world, 0.1, None);
    let launch = combat.take_launch().unwrap();
    combat.missiles.get_mut(&launch.operation).unwrap().faulted = true;
    combat.step_internal(&mut world, 10., None);
    assert_eq!(combat.pending_launch(launch.operation), Some(&launch));
    combat.reject_launch(launch.operation, &mut world).unwrap();
    assert!(combat.pending_launch(launch.operation).is_none());
    assert!(world.projectile(EntityId(1000)).is_none());
}
