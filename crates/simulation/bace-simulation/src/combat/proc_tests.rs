use super::*;
use bace_gameplay_api::physical_procs::*;
#[test]
fn accepted_contact_waits_for_exact_proc_receipts_and_cloak_heal_precedes_hp_write() {
    let (mut combat, mut world) = fixture();
    for id in 1..=2 {
        let mut p = prepared(id == 1);
        p.maneuvers[0].hooks.truncate(1);
        p.skills
            .iter_mut()
            .find(|(id, _)| *id == 6)
            .unwrap()
            .1
            .current = 0;
        combat
            .register_physical(EntityId(id), Arc::new(p), &world)
            .unwrap();
    }
    combat.enable_physical_procs();
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
    combat.step_internal(&mut world, 0.1, None);
    let request = combat.pending_physical_proc().unwrap();
    assert_eq!(request.phase(), PhysicalProcPhase::Attack);
    assert_eq!(world.combatant(EntityId(2)).unwrap().health(), 100);
    assert!(combat.physical_proc_pending(EntityId(1)) && combat.physical_proc_pending(EntityId(2)));
    assert_eq!(
        combat.apply(&mut world, EntityId(1), CombatRequest::CancelAttack, 0.1),
        Err(CombatRejection::Busy)
    );
    let mut receipt = PhysicalProcReceipt {
        key: request.key(),
        phase: request.phase(),
        damage: None,
    };
    receipt.key.operation += 1;
    assert_eq!(
        combat.accept_physical_proc(receipt),
        Err(CombatRejection::InvalidRequest)
    );
    receipt.key.operation -= 1;
    combat.accept_physical_proc(receipt).unwrap();
    combat.step_internal(&mut world, 0.2, None);
    let request = combat.pending_physical_proc().unwrap();
    let PhysicalProcRequest::Cloak { damage, .. } = request else {
        panic!("expected cloak after no Dirty skill")
    };
    world.damage_from(EntityId(2), EntityId(1), 90).unwrap();
    world
        .combatant_mut(EntityId(2))
        .unwrap()
        .apply_vital(EntityVital::Health, 10, 80, None)
        .unwrap();
    // The cloak can reduce the frozen incoming amount as well as cast a heal.
    combat
        .accept_physical_proc(PhysicalProcReceipt {
            key: request.key(),
            phase: request.phase(),
            damage: Some(damage / 2),
        })
        .unwrap();
    combat.step_internal(&mut world, 0.3, None);
    assert_eq!(
        world.combatant(EntityId(2)).unwrap().health(),
        80 - (damage / 2).min(80)
    );
    assert!(!combat.physical_proc_pending(EntityId(2)));
    let health = world.combatant(EntityId(2)).unwrap().health();
    combat.step_internal(&mut world, 1.0, None);
    assert_eq!(world.combatant(EntityId(2)).unwrap().health(), health);
}

#[test]
fn cleave_commits_each_target_before_starting_the_next_proc_but_fences_all_participants() {
    let (mut combat, mut world) = fixture();
    for id in 1..=3 {
        let mut p = prepared(id == 1);
        p.maneuvers[0].hooks.truncate(1);
        p.skills
            .iter_mut()
            .find(|(id, _)| *id == 6)
            .unwrap()
            .1
            .current = 0;
        if id == 1 {
            p.main.as_mut().unwrap().cleave_targets = 2;
        }
        combat
            .register_physical(EntityId(id), Arc::new(p), &world)
            .unwrap();
    }
    combat.enable_physical_procs();
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
    combat.step_internal(&mut world, 0.1, None);
    assert!(
        combat.physical_proc_pending(EntityId(3)),
        "entire accepted cleave blocks retirement"
    );
    let attack = combat.pending_physical_proc().unwrap();
    assert_eq!(attack.key().target, EntityId(2));
    combat
        .accept_physical_proc(PhysicalProcReceipt {
            key: attack.key(),
            phase: attack.phase(),
            damage: None,
        })
        .unwrap();
    combat.step_internal(&mut world, 0.2, None);
    let cloak = combat.pending_physical_proc().unwrap();
    let PhysicalProcRequest::Cloak { damage, .. } = cloak else {
        panic!("cloak")
    };
    combat
        .accept_physical_proc(PhysicalProcReceipt {
            key: cloak.key(),
            phase: cloak.phase(),
            damage: Some(damage),
        })
        .unwrap();
    combat.step_internal(&mut world, 0.3, None);
    assert!(world.combatant(EntityId(2)).unwrap().health() < 100);
    assert_eq!(world.combatant(EntityId(3)).unwrap().health(), 100);
    assert!(combat.physical_proc_pending(EntityId(3)));
    assert!(
        combat.pending_physical_proc().is_none(),
        "first HP write precedes second proc"
    );
    combat.step_internal(&mut world, 0.4, None);
    let attack = combat.pending_physical_proc().unwrap();
    assert_eq!(attack.key().target, EntityId(3));
    combat
        .accept_physical_proc(PhysicalProcReceipt {
            key: attack.key(),
            phase: attack.phase(),
            damage: None,
        })
        .unwrap();
    combat.step_internal(&mut world, 0.5, None);
    let cloak = combat.pending_physical_proc().unwrap();
    let PhysicalProcRequest::Cloak { damage, .. } = cloak else {
        panic!("cloak")
    };
    combat
        .accept_physical_proc(PhysicalProcReceipt {
            key: cloak.key(),
            phase: cloak.phase(),
            damage: Some(damage),
        })
        .unwrap();
    combat.step_internal(&mut world, 0.6, None);
    assert!(world.combatant(EntityId(3)).unwrap().health() < 100);
    assert!(!combat.physical_proc_pending(EntityId(1)));
    assert!(!combat.physical_proc_pending(EntityId(3)));
}
