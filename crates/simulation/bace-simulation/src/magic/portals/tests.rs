//! Failure retention for the durable portal-to-magic acknowledgment boundary.
use super::*;
fn pending() -> Magic {
    let mut magic = Magic::new(8);
    let mut driver = CastDriver::default();
    driver
        .begin(
            CastPreparation {
                id: 1,
                spell: 100,
                target: Some(1),
                gestures: vec![],
                uses_mana: false,
                player: false,
                fast_resistable_pk_spell: false,
            },
            1.0,
            CastObservation {
                cell: 1,
                position: Vec3::ZERO,
                alive: true,
                in_portal: false,
                peace_mode: false,
                heading_to_target: None,
                target_valid: true,
                target_in_range: true,
                geometry_clear: true,
                turning_to_target: false,
                manual_turning: false,
            },
        )
        .unwrap();
    magic.recovery.insert(EntityId(1), Default::default());
    magic.attempts.insert(
        EntityId(1),
        Attempt {
            item_target: None,
            cast_skill: 100,
            pending_direct: None,
            style_entry: None,
            motion_sequence_offset: 0,
            origin: CastOrigin::Staff {
                actor: EntityId(1),
                event: 7,
            },
            origin_epoch: 0,
            initial_cast: (CellId(1), Vec3::ZERO),
            target: Some(EntityId(1)),
            prepared: Arc::new(PreparedMagicSpell {
                spell: PreparedSpell {
                    id: 100,
                    school: bace_magic::MagicSchool::Life,
                    power: 0,
                    base_mana: 0,
                    range_constant: 0.0,
                    range_per_skill: 0.0,
                    harmful: false,
                    resistable: false,
                    effect: SpellEffect::Portal(bace_magic::PortalEffect::Recall { slot: 2 }),
                },
                gestures: vec![],
                components: vec![],
                component_modifiers: vec![],
                component_loss: 0.0,
                fast_resistable_pk_spell: false,
            }),
            driver,
            random: RandomRoot::new([1; 32], 1)
                .unwrap()
                .event_stream([2; 16], Domain::Magic)
                .unwrap(),
            resources: None,
            motion: None,
            turn_target: None,
            turn_control: None,
            cast: 1,
            components_confirmed: true,
            component_request_sent: false,
            component_failure: None,
            mana_applied: true,
            portal_request_sent: true,
            portal_operation: Some(5),
            portal_completion: None,
            previous_mode: None,
            terminal: None,
            peace_fizzle: None,
        },
    );
    magic
}
#[test]
fn failed_driver_completion_retains_exact_attempt_for_later_acknowledgment() {
    let mut magic = pending();
    let mut world = World::default();
    assert_eq!(
        magic.finish_portal_service(EntityId(1), 1, Ok(()), &mut world, f64::NAN),
        Err(CastRejection::InvalidState)
    );
    let attempt = magic.attempts.get(&EntityId(1)).unwrap();
    assert_eq!(attempt.portal_operation, Some(5));
    assert_eq!(attempt.portal_completion, None);
    assert_eq!(attempt.driver.stage(), bace_magic::CastStage::Release);
    magic
        .finish_portal_service(EntityId(1), 1, Ok(()), &mut world, 2.0)
        .unwrap();
    assert!(magic.attempts.is_empty());
    assert_eq!(magic.server_outcomes.len(), 1);
    assert_eq!(
        magic
            .recovery
            .get(&EntityId(1))
            .unwrap()
            .snapshot(2.0)
            .unwrap()
            .revision,
        1
    );
}
#[test]
fn recovery_retry_never_releases_again_or_changes_the_accepted_result() {
    let mut magic = pending();
    let mut world = World::default();
    // Inject the precise state after successful driver release but before the
    // fallible recovery recording. Its accepted portal result is immutable.
    let attempt = magic.attempts.get_mut(&EntityId(1)).unwrap();
    attempt.driver.resolve_release(1, 2.0, Ok(())).unwrap();
    attempt.portal_completion = Some(Ok(()));
    assert_eq!(
        magic.finish_portal_service(EntityId(1), 1, Ok(()), &mut world, f64::NAN),
        Err(CastRejection::InvalidState)
    );
    assert!(magic.attempts.contains_key(&EntityId(1)));
    assert!(magic.server_outcomes.is_empty());
    assert_eq!(
        magic.finish_portal_service(
            EntityId(1),
            1,
            Err(CastRejection::InvalidTarget),
            &mut world,
            3.0
        ),
        Err(CastRejection::InvalidState)
    );
    magic
        .finish_portal_service(EntityId(1), 1, Ok(()), &mut world, 3.0)
        .unwrap();
    assert!(magic.attempts.is_empty());
    assert_eq!(magic.server_outcomes.len(), 1);
    assert_eq!(
        magic.server_outcomes[0].result,
        Ok(CastChange::Completed { cast: 1 })
    );
    assert_eq!(
        magic
            .recovery
            .get(&EntityId(1))
            .unwrap()
            .snapshot(3.0)
            .unwrap()
            .revision,
        1
    );
}
