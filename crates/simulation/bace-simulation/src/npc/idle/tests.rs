use super::*;
fn registered() -> Npcs {
    let mut owner = Npcs::new(8);
    owner.sources.insert(
        EntityId(2),
        Source {
            admission_bound: false,
            admission_hold: None,
            admission: None,
            manager: NativeEmoteManager::new(Arc::new(
                NativeProgram::prepare(vec![], bace_emotes::NativeLimits::default()).unwrap(),
            )),
            random: None,
            use_radius: 3.0,
            clock_offset: 0.0,
            event_id: [0; 16],
            key_version: 0,
            active_operation: 0,
            invocations: BTreeMap::new(),
            recovery_ready: true,
            journal_hold: None,
            held_logical_now: None,
        },
    );
    owner.source_order.push(EntityId(2));
    owner
}

#[test]
fn admitted_fresh_idle_source_seeds_one_authoritative_checkpoint() {
    let mut owner = registered();
    let actor = EntityId(2);
    owner.root = Some(Arc::new(RandomRoot::new([7; 32], 1).unwrap()));
    let source = owner.sources.get_mut(&actor).unwrap();
    source.admission = Some(NpcScriptIdentity {
        template: 12,
        program_hash: [3; 32],
        content_generation: [4; 32],
    });
    source.admission_bound = true;
    assert_eq!(
        owner.freeze_bootstrap_idle(actor, 41, [0; 16], 30),
        Err(NpcFailure::DurabilityPending)
    );
    assert_eq!(owner.sources[&actor].event_id, [0; 16]);
    let checkpoint = owner.freeze_bootstrap_idle(actor, 41, [5; 16], 30).unwrap();
    assert_eq!(checkpoint.active_operation, 41);
    assert_eq!(checkpoint.event_id, [5; 16]);
    assert_eq!(checkpoint.key_version, 1);
    assert_eq!(checkpoint.invocations.len(), 1);
    assert_eq!(checkpoint.invocations[0].operation, 41);
    assert_eq!(checkpoint.invocations[0].random_position, 0);
    assert!(checkpoint.vm.work.is_empty());
    assert!(checkpoint.pending.is_empty());
    assert_eq!(
        owner.freeze_bootstrap_idle(actor, 41, [5; 16], 31).unwrap(),
        checkpoint
    );
    assert_eq!(
        owner.freeze_bootstrap_idle(actor, 42, [6; 16], 31),
        Err(NpcFailure::Conflict)
    );
    assert_eq!(owner.sources[&actor].event_id, [5; 16]);
    owner.release_idle(actor, 41, 31).unwrap();
}
#[test]
fn immutable_idle_registration_can_drain_but_journal_and_recovery_holds_cannot() {
    let mut owner = registered();
    owner.sources.get_mut(&EntityId(2)).unwrap().journal_hold = Some((41, 3.0));
    assert_eq!(
        owner.discard_idle_sources(),
        Err(NpcFailure::DurabilityPending)
    );
    assert!(owner.sources.contains_key(&EntityId(2)));
    owner.sources.get_mut(&EntityId(2)).unwrap().journal_hold = None;
    owner.sources.get_mut(&EntityId(2)).unwrap().recovery_ready = false;
    assert_eq!(
        owner.discard_idle_sources(),
        Err(NpcFailure::DurabilityPending)
    );
    owner.sources.get_mut(&EntityId(2)).unwrap().recovery_ready = true;
    owner.discard_idle_sources().unwrap();
    assert!(!owner.has_state());
}
#[test]
fn pending_domain_receipt_is_never_discarded_as_idle_catalog_state() {
    let mut owner = registered();
    let proposal = NpcProposal {
        ticket: 41,
        context: NpcContext {
            source: EntityId(2),
            target: None,
            operation: 1,
        },
        effect: NpcEffect::Service(NpcOperation::ResetHome),
    };
    owner.cast_services.insert(41, proposal.clone());
    assert_eq!(
        owner.discard_idle_sources(),
        Err(NpcFailure::DurabilityPending)
    );
    assert_eq!(owner.cast_services.get(&41), Some(&proposal));
    owner.cast_services.clear();
    owner.discard_idle_sources().unwrap();
}

#[test]
fn ordinary_retirement_waits_for_terminal_checkpoint_and_observer_output() {
    let mut owner = registered();
    let actor = EntityId(2);
    owner.durable_dirty.insert(actor);
    assert_eq!(
        owner.retire_idle_source(actor),
        Err(NpcFailure::DurabilityPending)
    );
    owner.mark_checkpoint_complete(actor);
    owner.notifications.push_back(NpcNotification {
        context: NpcContext {
            source: actor,
            target: Some(EntityId(3)),
            operation: 1,
        },
        operation: NpcOperation::ResetHome,
    });
    assert_eq!(
        owner.retire_idle_source(actor),
        Err(NpcFailure::DurabilityPending)
    );
    owner.take_notification().unwrap();
    owner.retire_idle_source(actor).unwrap();
    assert!(!owner.has_source(actor));
    assert!(!owner.source_order.contains(&actor));
    owner.retire_idle_source(actor).unwrap();
}

#[test]
fn idle_retirement_preserves_archives_and_unrelated_service_receipts() {
    let mut owner = registered();
    let actor = EntityId(2);
    owner.archives.insert(
        actor,
        NpcSourceArchive {
            source: actor,
            facts: bace_emotes::NpcActorFacts {
                creature: true,
                player: false,
            },
            cell: bace_types::CellId(1),
            position: bace_geometry::Vec3::new(0., 0., 0.),
            heading: 0.,
            properties: bace_entity::EntityProperties::new(16).unwrap(),
        },
    );
    assert_eq!(
        owner.retire_idle_source(actor),
        Err(NpcFailure::DurabilityPending)
    );
    assert!(owner.archives.contains_key(&actor));
    owner.archives.remove(&actor);
    let proposal = NpcProposal {
        ticket: 44,
        context: NpcContext {
            source: EntityId(3),
            target: None,
            operation: 1,
        },
        effect: NpcEffect::Service(NpcOperation::ResetHome),
    };
    owner.cast_services.insert(44, proposal.clone());
    owner.retire_idle_source(actor).unwrap();
    assert_eq!(owner.cast_services.get(&44), Some(&proposal));
}
