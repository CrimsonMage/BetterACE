mod magic_common;
use bace_gameplay_api::{CastChange, CastRejection};
use bace_magic::Vital;
use bace_simulation::{MagicResourceAction, MagicResourceCommand, MagicResourceResult};
use magic_common::*;
fn enqueue_resource(k: &mut Kernel, correlation: u64, action: MagicResourceAction) {
    let context = context(1);
    k.enqueue(Command::MagicResource(MagicResourceCommand {
        correlation,
        binding: Some(CharacterBinding {
            actor: context.actor,
            account: context.account,
            session: context.session,
        }),
        action,
    }))
    .unwrap();
}
fn resource_action(
    k: &mut Kernel,
    correlation: u64,
    action: MagicResourceAction,
) -> Arc<bace_simulation::MagicResourceOutcome> {
    enqueue_resource(k, correlation, action);
    step(k);
    let outcome = k
        .take_magic_resource_outcome()
        .expect("resource command outcome");
    assert_eq!(outcome.correlation, correlation);
    outcome
}
fn inspect_components(
    k: &mut Kernel,
    outcomes: &[Result<CastChange, CastRejection>],
) -> Arc<bace_simulation::PreparedMagicResources> {
    let cast = outcomes
        .iter()
        .find_map(|result| match result {
            Ok(CastChange::Started { cast }) => Some(*cast),
            _ => None,
        })
        .expect("component cast started");
    assert!(
        k.take_inventory_proposal().is_none(),
        "generic inventory cannot steal a component operation"
    );
    let outcome = resource_action(k, 100, MagicResourceAction::Inspect { cast });
    match &outcome.result {
        Ok(MagicResourceResult::Prepared(prepared)) => prepared.clone(),
        _ => panic!("named magic lane did not expose the retained component proposal"),
    }
}
fn assert_resolved(
    outcome: &bace_simulation::MagicResourceOutcome,
    operation: u64,
    committed: bool,
) {
    assert!(
        matches!(&outcome.result, Ok(MagicResourceResult::Resolved { operation: actual, committed: saved }) if *actual == operation && *saved == committed)
    );
}
fn acknowledge_resources(k: &mut Kernel, operation: u64) {
    let outcome = resource_action(k, 103, MagicResourceAction::Acknowledge { operation });
    assert!(
        matches!(&outcome.result, Ok(MagicResourceResult::Acknowledged { operation: actual }) if *actual == operation)
    );
}
fn cast_and_finish(
    k: &mut Kernel,
    sequence: u32,
    spell: u32,
) -> Vec<Result<CastChange, CastRejection>> {
    k.enqueue(Command::Cast {
        context: context(sequence),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell,
        },
    })
    .unwrap();
    let mut outcomes = Vec::new();
    for _ in 0..8 {
        step(k);
        while let Some(outcome) = k.take_cast_outcome() {
            outcomes.push(outcome.result);
        }
    }
    outcomes
}
#[test]
fn streak_recovery_survives_completed_attempt_and_reconnect() {
    let mut k = kernel(32);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 1,
            maximum: 1,
        },
    );
    s.fast_resistable_pk_spell = true;
    k.register_magic_spell(s.clone()).unwrap();
    assert!(
        cast_and_finish(&mut k, 1, 100)
            .iter()
            .any(|o| matches!(o, Ok(CastChange::Completed { .. })))
    );
    let saved = k.magic_recovery(EntityId(1)).unwrap();
    assert!(saved.streak_remaining > 1.0);
    assert!(cast_and_finish(&mut k, 2, 100).contains(&Err(CastRejection::StreakCooldown)));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    let mut restored = kernel(32);
    restored.register_magic_spell(s).unwrap();
    restored
        .restore_magic_recovery(EntityId(1), saved, 0.1)
        .unwrap();
    assert!(cast_and_finish(&mut restored, 1, 100).contains(&Err(CastRejection::StreakCooldown)));
    for _ in 0..65 {
        step(&mut restored);
    }
    assert!(
        cast_and_finish(&mut restored, 2, 100)
            .iter()
            .any(|o| matches!(o, Ok(CastChange::Completed { .. })))
    );
}
#[test]
fn war_void_lock_survives_attempt_but_healing_school_does_not_lock() {
    let mut k = kernel(32);
    for (id, school) in [
        (100, MagicSchool::War),
        (101, MagicSchool::Void),
        (102, MagicSchool::Life),
    ] {
        let mut s = spell(
            id,
            SpellEffect::Boost {
                vital: Vital::Health,
                minimum: 1,
                maximum: 1,
            },
        );
        s.spell.school = school;
        k.register_magic_spell(s).unwrap();
    }
    cast_and_finish(&mut k, 1, 100);
    assert!(cast_and_finish(&mut k, 2, 101).contains(&Err(CastRejection::SchoolRecovery)));
    assert!(
        cast_and_finish(&mut k, 3, 102)
            .iter()
            .any(|o| matches!(o, Ok(CastChange::Completed { .. })))
    );
}
#[test]
fn server_monster_and_instant_emote_use_owned_effects_without_client_session() {
    use bace_gameplay_api::CastOrigin;
    let mut k = kernel(32);
    k.register_magic_spell(spell(
        200,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    ))
    .unwrap();
    let origin = CastOrigin::Monster {
        actor: EntityId(2),
        event: 1,
    };
    k.cast_from_server(
        origin,
        CastRequest::Targeted {
            target: EntityId(2),
            spell: 200,
        },
    )
    .unwrap();
    for _ in 0..8 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert_eq!(k.take_server_cast_outcome().unwrap().origin, origin);
    assert_eq!(
        k.cast_from_server(
            origin,
            CastRequest::Targeted {
                target: EntityId(2),
                spell: 200
            }
        ),
        Err(CastRejection::StaleSequence)
    );
    k.cast_from_server(
        CastOrigin::Emote {
            actor: EntityId(2),
            event: 1,
            instant: true,
        },
        CastRequest::Targeted {
            target: EntityId(2),
            spell: 200,
        },
    )
    .unwrap();
    step(&mut k);
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        70
    );
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert!(k.take_cast_outcome().is_none());
}

fn component_kernel(loss: f32) -> Kernel {
    use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
    let mut k = kernel_with_components(64, true);
    k.register_inventory_container(InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 10,
        pack_slots: 1,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(InventoryItem {
        structure: None,
        id: EntityId(10),
        revision: 1,
        template: 500,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 10,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    })
    .unwrap();
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    s.spell.power = 1;
    s.component_loss = loss;
    s.components = vec![(500, 2)];
    s.component_modifiers = vec![(500, 1.0)];
    k.register_magic_spell(s).unwrap();
    k
}
#[test]
fn components_automatically_reserve_and_wait_for_exact_inventory_receipt() {
    use bace_simulation::InventoryReceipt;
    let mut k = component_kernel(10000.0);
    let results = cast_and_finish(&mut k, 1, 100);
    assert!(
        results
            .iter()
            .all(|r| !matches!(r, Ok(CastChange::Completed { .. })))
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    let prepared = inspect_components(&mut k, &results);
    let ticket = &prepared.inventory;
    assert_eq!(ticket.proposal.changes[0].after.stack, 8);
    assert!(k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    let resources = k.pending_magic_resources(ticket.operation).unwrap();
    assert_eq!(resources.operation, ticket.operation);
    assert_eq!((resources.mana.before, resources.mana.after), (100, 90));
    for _ in 0..300 {
        step(&mut k);
    }
    assert!(k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    assert!(k.take_inventory_proposal().is_none());
    assert_eq!(
        k.confirm_magic_components(1, EntityId(1), true),
        Err(CastRejection::InvalidState)
    );
    let receipt = InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert!(
        k.confirm_inventory_committed(&receipt).is_err(),
        "generic receipt cannot adopt a claimed magic operation"
    );
    let wrong = resource_action(
        &mut k,
        101,
        MagicResourceAction::Resolve {
            operation: ticket.operation + 1,
            committed: true,
        },
    );
    assert!(wrong.result.is_err());
    assert_eq!(k.inventory_count(EntityId(1), 500), 10);
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    let accepted = resource_action(
        &mut k,
        102,
        MagicResourceAction::Resolve {
            operation: ticket.operation,
            committed: true,
        },
    );
    assert_resolved(&accepted, ticket.operation, true);
    assert!(!k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    for _ in 0..3 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    assert_eq!(k.inventory_count(EntityId(1), 500), 8);
    assert!(k.confirm_inventory_committed(&receipt).is_err());
    assert!(
        k.take_cast_outcome()
            .is_some_and(|r| matches!(r.result, Ok(CastChange::Completed { .. })))
    );
    acknowledge_resources(&mut k, ticket.operation);
}
#[test]
fn life_projectile_drains_caster_once_keeps_one_health_and_hits_target() {
    let mut k = kernel(32);
    let mut projectile = shot(ProjectileShape::Bolt, 1);
    projectile.damage_type = 0x80;
    k.register_magic_spell(spell(
        100,
        SpellEffect::LifeProjectile {
            projectile,
            source: Vital::Health,
            proportion: 1.0,
            damage_ratio: 1.0,
        },
    ))
    .unwrap();
    k.supply_projectile_id(EntityId(50)).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    })
    .unwrap();
    for _ in 0..30 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        1
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        1
    );
}
#[test]
fn combat_pet_charge_receipt_then_spawn_expire_and_logout_stow() {
    use bace_ai::{PetUseRequirements, PetUser};
    use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
    use bace_physics::{CollisionShape, CollisionSphere};
    use bace_simulation::{
        CharacterRegistrationError, InventoryReceipt, PetAction, PetCommand, PetDecision, PetEvent,
        PlayerDetachRequest, PreparedCombatPet,
    };
    let mut k = kernel_fixture(64, false, true);
    k.register_inventory_container(InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 10,
        pack_slots: 1,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(InventoryItem {
        id: EntityId(10),
        revision: 1,
        template: 500,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        structure: Some(3),
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    })
    .unwrap();
    k.register_pet_owner(
        EntityId(1),
        PetUser {
            portal_space: false,
            owns_device: true,
            advancement: 3,
            skill: 600,
            level: 275,
            mastery: 1,
            charges: 3,
            active_combat_pet: false,
            cooldown_active: false,
        },
    )
    .unwrap();
    let profile = Arc::new(PreparedCombatPet {
        template: 900,
        shape: Arc::new(
            CollisionShape::prepare(
                vec![CollisionSphere {
                    center: Vec3::new(0., 0., 0.2),
                    radius: 0.2,
                }],
                0.0,
                0.1,
            )
            .unwrap(),
        ),
        capabilities: Capabilities {
            speed: 5.0,
            jump_impulse: 5.0,
        },
        combat: CombatantProfile {
            maximum_health: 20,
            melee_damage: 10,
            melee_range: 2.0,
            attack_duration: 0.2,
            strike_offsets: vec![0.1],
            player: false,
        },
        lifetime_seconds: 0.3,
        visual_range: 20.0,
        requirements: PetUseRequirements {
            skill_required: true,
            skill_level: 570,
            level: 200,
            mastery: 1,
            unlimited: false,
        },
        cooldown_group: Some(3),
        cooldown_seconds: 45.0,
    });
    let first_pet = EntityId(0x8000_0050);
    k.enqueue(Command::Pet(PetCommand {
        correlation: 70,
        action: PetAction::Summon {
            context: context(1),
            device: EntityId(10),
            device_revision: 1,
            pet: first_pet,
            user: PetUser {
                portal_space: false,
                owns_device: true,
                advancement: 3,
                skill: 600,
                level: 275,
                mastery: 1,
                charges: 3,
                active_combat_pet: false,
                cooldown_active: false,
            },
            activation: None,
            profile: profile.clone(),
        },
    }))
    .unwrap();
    step(&mut k);
    let proposed = k.take_pet_outcome().expect("pet command outcome");
    let (ticket, registry) = match proposed.result {
        Ok(PetDecision::Proposed {
            ticket,
            registry_after,
            ..
        }) => (ticket, registry_after),
        other => panic!("expected authenticated pet proposal: {other:?}"),
    };
    let operation = ticket.operation;
    assert!(!k.world().contains_identity(first_pet));
    assert!(
        registry
            .iter()
            .any(|e| e.spell == 0x8003 && e.spec.layer == 1 && e.metadata.degrade_limit == -666.0)
    );
    assert!(k.take_inventory_proposal().is_none());
    assert_eq!(ticket.proposal.changes[0].after.stack, 1);
    assert_eq!(ticket.proposal.changes[0].after.structure, Some(2));
    k.enqueue(Command::Pet(PetCommand {
        correlation: 71,
        action: PetAction::Resolve {
            receipt: InventoryReceipt {
                operation,
                revisions: ticket
                    .proposal
                    .changes
                    .iter()
                    .map(|c| (c.after.id, c.after.revision))
                    .collect(),
            },
            committed: true,
        },
    }))
    .unwrap();
    step(&mut k);
    assert!(matches!(
        k.take_pet_outcome().expect("pet receipt outcome").result,
        Ok(PetDecision::Resolved {
            operation: resolved,
            committed: true
        }) if resolved == operation
    ));
    assert!(matches!(
        k.take_pet_event(),
        Some(PetEvent::Spawned { pet, .. }) if pet == first_pet
    ));
    assert_eq!(k.pet_owner(first_pet), Some(EntityId(1)));
    for _ in 0..14 {
        step(&mut k);
    }
    let ticket = match k.take_pet_event() {
        Some(PetEvent::ReleaseProposed { ticket, .. }) => ticket,
        other => panic!("expiry requires durable device release: {other:?}"),
    };
    assert!(!ticket.proposal.changes[0].after.active_pet);
    assert_eq!(ticket.proposal.changes[0].after.structure, Some(2));
    k.confirm_inventory_committed(&InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    assert!(!k.world().contains_identity(first_pet));
    assert!(matches!(
        k.take_pet_event(),
        Some(PetEvent::Despawned { .. })
    ));

    // A second summon is still alive at logout. The detach owner must start
    // a durable release, retain the player, and wait for that exact receipt.
    let second = Arc::new(PreparedCombatPet {
        lifetime_seconds: 60.0,
        cooldown_group: None,
        ..(*profile).clone()
    });
    let operation = k
        .propose_combat_pet(EntityId(1), EntityId(10), EntityId(51), second)
        .unwrap();
    let ticket = k.take_inventory_proposal().unwrap();
    k.confirm_inventory_committed(&InventoryReceipt {
        operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    step(&mut k);
    assert!(matches!(
        k.take_pet_event(),
        Some(PetEvent::Spawned {
            pet: EntityId(51),
            ..
        })
    ));
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
    };
    let snapshot = k.read_player_snapshot(binding).unwrap();
    let detach = PlayerDetachRequest {
        correlation: 1,
        binding,
        expected_revision: snapshot.character().progression().revision(),
        capture_final: false,
        expected_items: snapshot
            .items()
            .iter()
            .map(|i| (i.id, i.revision))
            .collect(),
    };
    assert!(matches!(
        k.detach_player(&detach),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    assert!(k.world().contains_identity(EntityId(51)));
    let release = match k.take_pet_event() {
        Some(PetEvent::ReleaseProposed { ticket, .. }) => ticket,
        other => panic!("logout release ticket: {other:?}"),
    };
    assert!(!release.proposal.changes[0].after.active_pet);
    assert!(matches!(
        k.detach_player(&detach),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    assert!(k.world().contains_identity(EntityId(51)));
    k.confirm_inventory_committed(&InventoryReceipt {
        operation: release.operation,
        revisions: release
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    assert!(!k.world().contains_identity(EntityId(51)));
    assert_eq!(k.pet_owner(EntityId(51)), None);
    assert!(matches!(
        k.take_pet_event(),
        Some(PetEvent::Despawned {
            pet: EntityId(51),
            owner: EntityId(1)
        })
    ));
    assert_eq!(k.request_pet_stow(EntityId(1)), Ok(None));
}

#[test]
fn instant_server_effect_does_not_replace_player_windup_or_spend_its_mana() {
    use bace_gameplay_api::CastOrigin;
    let mut k = kernel(32);
    for id in [100, 201] {
        k.register_magic_spell(spell(
            id,
            SpellEffect::Boost {
                vital: Vital::Health,
                minimum: 10,
                maximum: 10,
            },
        ))
        .unwrap();
    }
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    step(&mut k);
    k.cast_from_server(
        CastOrigin::Emote {
            actor: EntityId(1),
            event: 1,
            instant: true,
        },
        CastRequest::Targeted {
            target: EntityId(1),
            spell: 201,
        },
    )
    .unwrap();
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    for _ in 0..8 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        70
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert!(
        k.take_server_cast_outcome()
            .is_some_and(|r| matches!(r.result, Ok(CastChange::Completed { .. })))
    );
    assert!(
        k.take_cast_outcome()
            .is_some_and(|r| matches!(r.result, Ok(CastChange::Started { .. })))
    );
    assert!(
        k.take_cast_outcome()
            .is_some_and(|r| matches!(r.result, Ok(CastChange::Completed { .. })))
    );
}

#[test]
fn no_burn_components_complete_locally_without_empty_persistence_ticket() {
    let mut k = component_kernel(0.0);
    let outcomes = cast_and_finish(&mut k, 1, 100);
    assert!(
        outcomes
            .iter()
            .any(|o| matches!(o, Ok(CastChange::Completed { .. })))
    );
    assert_eq!(k.inventory_count(EntityId(1), 500), 10);
    assert!(k.take_inventory_proposal().is_none());
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert!(k.pending_magic_resources(1).is_none());
    assert!(!k.world().has_vital_reservations());
}

#[test]
fn rejected_component_burn_releases_mana_without_consuming_items_or_resources() {
    let mut k = component_kernel(10000.0);
    let outcomes = cast_and_finish(&mut k, 1, 100);
    let prepared = inspect_components(&mut k, &outcomes);
    let ticket = &prepared.inventory;
    assert!(k.world().vital_reserved(EntityId(1), EntityVital::Mana));
    assert!(
        k.reject_inventory(ticket.operation).is_err(),
        "generic rejection cannot release a claimed magic operation"
    );
    let rejected = resource_action(
        &mut k,
        102,
        MagicResourceAction::Resolve {
            operation: ticket.operation,
            committed: false,
        },
    );
    assert_resolved(&rejected, ticket.operation, false);
    assert!(!k.world().has_vital_reservations());
    assert_eq!(k.inventory_count(EntityId(1), 500), 10);
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    for _ in 0..3 {
        step(&mut k);
    }
    assert!(
        k.take_cast_outcome()
            .is_some_and(|o| o.result == Err(CastRejection::MissingComponents))
    );
    acknowledge_resources(&mut k, ticket.operation);
}

#[test]
fn mode_change_after_component_receipt_waits_for_prepaid_effect_delivery() {
    use bace_gameplay_api::{CombatRejection, CombatRequest};
    let mut k = component_kernel(10000.0);
    let outcomes = cast_and_finish(&mut k, 1, 100);
    let prepared = inspect_components(&mut k, &outcomes);
    let ticket = &prepared.inventory;
    let receipt = bace_simulation::InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert!(k.confirm_inventory_committed(&receipt).is_err());
    assert!(k.world().has_reserved_vitals(EntityId(1)));
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    // Both commands run before the effect phase: the mana receipt releases its
    // reservation, but the prepaid cast must still block the following mode change.
    enqueue_resource(
        &mut k,
        102,
        MagicResourceAction::Resolve {
            operation: ticket.operation,
            committed: true,
        },
    );
    k.enqueue(Command::Combat {
        context: context(2),
        request: CombatRequest::ChangeMode(1),
    })
    .unwrap();
    let events = step(&mut k);
    assert_resolved(
        &k.take_magic_resource_outcome().unwrap(),
        ticket.operation,
        true,
    );
    assert!(
        !k.world().has_reserved_vitals(EntityId(1)),
        "the exact mana reservation has completed"
    );
    assert_eq!(
        k.take_combat_outcome().unwrap().result,
        Err(CombatRejection::Busy)
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MagicEvent::Fizzle { .. }))
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    assert!(matches!(
        k.take_cast_outcome().unwrap().result,
        Ok(CastChange::Completed { .. })
    ));
    assert_eq!(k.inventory_count(EntityId(1), 500), 8);
    k.enqueue(Command::Combat {
        context: context(3),
        request: CombatRequest::ChangeMode(1),
    })
    .unwrap();
    step(&mut k);
    assert!(
        k.take_combat_outcome().unwrap().result.is_ok(),
        "guard releases after actual effect completion"
    );
    acknowledge_resources(&mut k, ticket.operation);
}
