use super::*;
use bace_gameplay_api::CharacterBinding;
use bace_replication::{
    BatchLimits, EventSequencer, InventoryProjection as P, PortalPhase, PortalView, Sequences,
    project_portal, project_server_motion,
};

fn transcript_limits() -> BatchLimits {
    BatchLimits {
        max_messages: 8,
        max_bytes: 8192,
        max_message_bytes: 4096,
        max_string_bytes: 4096,
    }
}

fn transcript_view(actor: EntityId) -> bace_simulation::PortalAcceptedView {
    bace_simulation::PortalAcceptedView {
        actor,
        position: bace_interactions::PortalPosition {
            cell: 0x1234_0001,
            origin: [4., 5., 6.],
            rotation: [1., 0., 0., 0.],
        },
        velocity: [0.; 3],
        grounded: true,
        epoch: 1,
        cloaked: false,
    }
}

fn transcript_portal_view(
    accepted: bace_simulation::PortalAcceptedView,
    state: Option<u32>,
) -> PortalView {
    PortalView {
        object: accepted.actor.0,
        position: bace_wire::PositionPack {
            position: bace_wire::WirePosition {
                cell: accepted.position.cell,
                origin: accepted.position.origin,
                rotation: accepted.position.rotation,
            },
            velocity: Some(accepted.velocity),
            placement: None,
            grounded: accepted.grounded,
            instance_sequence: 0,
            position_sequence: 0,
            teleport_sequence: 0,
            force_position_sequence: 0,
        },
        physics_state: state,
    }
}

fn opcode(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

#[test]
fn source_recall_notice_motion_and_accepted_portal_visibility_keep_one_transcript() {
    // ACE Player_Location's recall announcement precedes the authored action;
    // Teleport then publishes F751 privately, F748 to observers, and the
    // materialization state only after both readiness receipts. These are the
    // same source phases qualified individually by recall_output.tsv and
    // portal_state.csv; this checks their shared sequence owner and handoff.
    let actor = EntityId(0x5000_0001);
    let binding = CharacterBinding {
        actor,
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
    };
    let mut sequences = Sequences::new(64).unwrap();
    sequences
        .advance(bace_replication::SequenceKind::ObjectInstance, 0)
        .unwrap();
    let mut events = EventSequencer::new(binding, 1);
    let mut items = BTreeMap::new();
    let notice = events
        .project_inventory_with_actor(
            binding,
            &[P::System {
                text: "Alice is recalling to the lifestone.",
                chat_type: 0x17,
            }],
            &mut items,
            Some(&mut sequences),
            bace_wire::ObjectCodecLimits {
                max_message_bytes: 4096,
                max_model_entries: 255,
                max_children: 128,
                max_restrictions: 1024,
                max_motion_commands: 32,
                max_string_bytes: 4096,
            },
            transcript_limits(),
        )
        .unwrap();
    let motion = project_server_motion(
        actor.0,
        &bace_wire::MovementDescription {
            autonomous: false,
            motion_flags: 0,
            current_style: 0x3d,
            body: bace_wire::MotionBody::State {
                state: bace_wire::InterpretedMotion {
                    current_style: Some(0x3d),
                    forward_command: Some(3),
                    commands: vec![bace_wire::MotionCommandItem {
                        raw_command: bace_interactions::RecallKind::Lifestone.motion() as u16,
                        sequence: 0,
                        autonomous: false,
                        speed: 1.,
                    }],
                    ..Default::default()
                },
                sticky_object: None,
            },
        },
        &mut sequences,
        transcript_limits(),
    )
    .unwrap();
    let accepted = transcript_view(actor);
    let mut retained = PortalRuntime::new();
    retained.push(PortalDeliveryWork::Event(PortalServiceEvent::Hidden {
        operation: 55,
        actors: vec![actor],
        views: vec![accepted],
    }));
    retained.push(PortalDeliveryWork::Event(PortalServiceEvent::Teleported {
        operation: 55,
        actors: vec![actor],
        views: vec![accepted],
    }));
    retained.push(PortalDeliveryWork::Event(
        PortalServiceEvent::Materialized {
            operation: 55,
            actor,
            view: accepted,
        },
    ));
    assert!(retained.acknowledge(2).is_err());
    assert_eq!(retained.deliveries.front().unwrap().sequence, 1);
    let hide = project_portal(
        binding,
        PortalPhase::Hide,
        transcript_portal_view(accepted, None),
        &mut sequences,
        transcript_limits(),
    )
    .unwrap();
    retained.acknowledge(1).unwrap();
    let teleport = project_portal(
        binding,
        PortalPhase::Teleport,
        transcript_portal_view(accepted, Some(0x4010)),
        &mut sequences,
        transcript_limits(),
    )
    .unwrap();
    assert!(teleport.reset_visibility);
    assert_eq!(teleport.observers, teleport.owner.messages[1..]);
    retained.acknowledge(2).unwrap();
    let materialized = project_portal(
        binding,
        PortalPhase::Materialize,
        transcript_portal_view(accepted, Some(8)),
        &mut sequences,
        transcript_limits(),
    )
    .unwrap();
    retained.acknowledge(3).unwrap();
    assert!(!retained.has_pending());
    assert_eq!(hide.owner.messages, hide.observers);
    assert_eq!(materialized.owner.messages, materialized.observers);
    assert_eq!(
        [
            notice
                .messages
                .iter()
                .map(|m| opcode(&m.bytes))
                .collect::<Vec<_>>(),
            vec![opcode(&motion.bytes)],
            hide.owner
                .messages
                .iter()
                .map(|m| opcode(&m.bytes))
                .collect(),
            teleport
                .owner
                .messages
                .iter()
                .map(|m| opcode(&m.bytes))
                .collect(),
            materialized
                .owner
                .messages
                .iter()
                .map(|m| opcode(&m.bytes))
                .collect(),
        ],
        [
            vec![0xf7e0],
            vec![0xf74c],
            vec![0xf755],
            vec![0xf751, 0xf748, 0xf74b],
            vec![0xf74b],
        ]
    );
}
#[test]
fn portal_output_pressure_and_wrong_ack_preserve_exact_order_and_drain_obligation() {
    let mut portals = PortalRuntime::new();
    for operation in 1..=DELIVERY_CAPACITY as u64 {
        assert!(portals.has_room());
        portals.push(PortalDeliveryWork::Event(PortalServiceEvent::Linked {
            operation,
            actor: EntityId(1),
        }));
    }
    assert!(!portals.has_room());
    assert!(portals.has_pending());
    assert!(portals.acknowledge(2).is_err());
    for operation in 1..=DELIVERY_CAPACITY as u64 {
        let first = portals.deliveries.front().unwrap();
        assert_eq!(first.sequence, operation);
        assert!(
            matches!(first.work, PortalDeliveryWork::Event(PortalServiceEvent::Linked { operation: actual, .. }) if actual == operation)
        );
        portals.acknowledge(operation).unwrap();
    }
    assert!(!portals.has_pending());
    assert!(portals.has_room());
    assert!(portals.acknowledge(DELIVERY_CAPACITY as u64).is_err());
    portals.next = u64::MAX;
    assert!(!portals.has_room());
}

#[tokio::test]
async fn rejected_portal_completion_requires_exact_ticket_and_never_projects_link_success() {
    // A rejected durable operation has no accepted link mutation. Spell casts
    // obtain their failure from the simulation magic terminal owner; this
    // portal output owner must release only its matching retained ticket.
    let (_cluster, _directory, mut runtime, key, binding) = output_runtime().await;
    let actor = binding.actor;
    let position = bace_interactions::PortalPosition {
        cell: 0x1234_0001,
        origin: [4., 5., 6.],
        rotation: [1., 0., 0., 0.],
    };
    let ticket = PortalServiceTicket {
        origin: bace_simulation::PortalServiceOrigin::Spell,
        operation: 70,
        cast: 77,
        actor,
        cast_actor: actor,
        before_revision: 1,
        after_revision: 2,
        participants: vec![(actor, 1, 2)],
        mana: None,
        effect: bace_simulation::PortalServiceEffect::Link(bace_interactions::PortalLinkMutation {
            before_revision: 1,
            after_revision: 2,
            position_slot: 4,
            before: None,
            after: position,
            data_id: None,
            tied_summoned: false,
        }),
    };
    runtime
        .portals
        .tickets
        .insert(ticket.operation, ticket.clone());
    // Kernel portal completion hands spell failure to the magic terminal
    // owner. Blocked and exact failed/aborted tickets publish no portal packet.
    runtime.limits.messages = 0;
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Blocked {
            operation: 70,
            actor,
        }));
    runtime.project_portal_deliveries().unwrap();
    assert!(runtime.portals.blocked_effects.contains(&70));
    assert!(runtime.portals.deliveries.is_empty());
    let mut wrong = ticket.clone();
    wrong.cast = 78;
    runtime
        .portals
        .push(PortalDeliveryWork::Completed(Box::new(PortalCompletion {
            work: PortalWork {
                epoch: 1,
                bindings: vec![binding],
                ticket: wrong,
            },
            committed: false,
            aborted: false,
        })));
    assert!(runtime.project_portal_deliveries().is_err());
    assert!(runtime.portals.has_pending());
    assert_eq!(runtime.portals.tickets.get(&70), Some(&ticket));
    assert!(runtime.network_output.is_empty());
    let PortalDeliveryWork::Completed(completion) =
        &mut runtime.portals.deliveries.front_mut().unwrap().work
    else {
        panic!("retained exact completion")
    };
    completion.work.ticket = ticket.clone();
    runtime.project_portal_deliveries().unwrap();
    assert!(!runtime.portals.has_pending());
    assert!(runtime.network_output.is_empty());

    let mut aborted = ticket.clone();
    aborted.operation = 72;
    aborted.effect =
        bace_simulation::PortalServiceEffect::Teleport(vec![bace_world::WorldTeleport {
            actor,
            expected_epoch: 1,
            destination: bace_types::CellId(position.cell),
            position: bace_geometry::Vec3::new(
                position.origin[0],
                position.origin[1],
                position.origin[2],
            ),
            heading: 0.,
        }]);
    runtime.portals.tickets.insert(72, aborted.clone());
    runtime.portals.push(PortalDeliveryWork::Event(
        PortalServiceEvent::AbortedAfterCommit {
            operation: 72,
            actor,
        },
    ));
    runtime
        .portals
        .push(PortalDeliveryWork::Completed(Box::new(PortalCompletion {
            work: PortalWork {
                epoch: 1,
                bindings: vec![binding],
                ticket: aborted,
            },
            committed: true,
            aborted: true,
        })));
    runtime.project_portal_deliveries().unwrap();
    assert!(!runtime.portals.has_pending());
    assert!(runtime.network_output.is_empty());

    let mut binding_link = ticket.clone();
    binding_link.operation = 73;
    binding_link.origin = bace_simulation::PortalServiceOrigin::Binding;
    runtime.portals.tickets.insert(73, binding_link.clone());
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Linked {
            operation: 73,
            actor,
        }));
    runtime
        .portals
        .push(PortalDeliveryWork::Completed(Box::new(PortalCompletion {
            work: PortalWork {
                epoch: 1,
                bindings: vec![binding],
                ticket: binding_link,
            },
            committed: true,
            aborted: false,
        })));
    runtime.project_portal_deliveries().unwrap();
    assert!(!runtime.portals.has_pending());
    assert!(runtime.network_output.is_empty());

    let mut recall = ticket;
    recall.origin =
        bace_simulation::PortalServiceOrigin::Recall(bace_interactions::RecallKind::Lifestone);
    recall.operation = 71;
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime.portals.tickets.insert(71, recall.clone());
    runtime
        .portals
        .push(PortalDeliveryWork::Completed(Box::new(PortalCompletion {
            work: PortalWork {
                epoch: 1,
                bindings: vec![binding],
                ticket: recall.clone(),
            },
            committed: false,
            aborted: false,
        })));
    assert!(
        runtime
            .project_portal_deliveries()
            .unwrap_err()
            .contains("source-qualified failure output; retained")
    );
    assert!(runtime.portals.has_pending());
    assert_eq!(runtime.portals.tickets.get(&71), Some(&recall));
    assert!(runtime.network_output.is_empty());
    assert!(!runtime.disconnected_portal_handoff(key));
    // Test teardown only: the live owner retains this unsupported obligation.
    let sequence = runtime.portals.deliveries.front().unwrap().sequence;
    runtime.portals.acknowledge(sequence).unwrap();
    runtime.portals.push(PortalDeliveryWork::Event(
        PortalServiceEvent::AbortedAfterCommit {
            operation: 71,
            actor,
        },
    ));
    assert!(
        runtime
            .project_portal_deliveries()
            .unwrap_err()
            .contains("source-qualified failure output; retained")
    );
    assert!(runtime.portals.has_pending());
    let sequence = runtime.portals.deliveries.front().unwrap().sequence;
    runtime.portals.acknowledge(sequence).unwrap();
    runtime.portals.tickets.remove(&71);
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}

pub(in crate::game_runtime) async fn output_runtime() -> (
    crate::player_service::tests::cluster::Cluster,
    tempfile::TempDir,
    Box<GameRuntime>,
    SessionKey,
    CharacterBinding,
) {
    use bace_auth::{
        AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
    };
    use bace_persistence::{CharacterLease, OwnershipState};
    use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};

    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("portal-output-owner").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let key = SessionKey {
        id: 1,
        generation: 7,
    };
    runtime.players.authenticated(key, &account).unwrap();
    let actor = EntityId(0x5000_0001);
    let binding = CharacterBinding {
        actor,
        account: account.id,
        session: bace_gameplay_api::SessionId(key.generation),
    };
    let player = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: actor.0,
                template_revision: 1,
                mutation_revision: 4,
                state: bace_content::WeenieV1 {
                    schema_version: 1,
                    weenie_id: 1,
                    class_name: "test_player".into(),
                    weenie_type: 1,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            account_id: account.id.0,
            name: "Test Player".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let loaded = Arc::new(crate::game_login::LoadedPlayer {
        is_plussed: false,
        key,
        binding,
        lease: CharacterLease {
            character_id: actor.0,
            epoch: 1,
            state: OwnershipState::Online,
        },
        persisted_version: 1,
        player,
        cached_experience: 0,
        inventory: vec![],
    });
    runtime
        .players
        .test_admit_replication(key, loaded.clone())
        .unwrap();
    let replica = runtime.players.replication(actor).unwrap();
    replica.public_physics_state = Some(8);
    replica
        .properties
        .advance(bace_replication::SequenceKind::ObjectInstance, 0)
        .unwrap();
    runtime.sessions.insert(
        key,
        Session {
            account,
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: Some(lifecycle::Loading {
                loaded,
                phase: lifecycle::Phase::Entered,
                account_created: None,
                friends: vec![],
                friends_next: 0,
                cold: None,
                appearance: None,
                character_assets: None,
                enchantments: vec![],
                spell_table: None,
                online: None,
                receipt: None,
                snapshot: None,
                cold_token: None,
                region_requested: false,
                settle_attempts: 0,
            }),
            failure: None,
        },
    );
    (_cluster, _directory, runtime, key, binding)
}

pub(in crate::game_runtime) fn stage_output_event(
    runtime: &mut GameRuntime,
    event: PortalServiceEvent,
) {
    runtime.portals.push(PortalDeliveryWork::Event(event));
}

pub(in crate::game_runtime) fn ready_output_transit(
    runtime: &mut GameRuntime,
    actor: EntityId,
    operation: u64,
    epoch: u16,
) {
    let ready = bace_simulation::PortalResolution::DestinationReady {
        actor,
        operation,
        epoch,
    };
    runtime.portals.controls.insert(81, ready.clone());
    runtime
        .accept_portal_control(bace_simulation::PortalResolutionOutcome {
            correlation: 81,
            resolution: ready,
            result: Ok(()),
        })
        .unwrap();
}

pub(in crate::game_runtime) fn output_transit_drained(
    runtime: &GameRuntime,
    actor: EntityId,
) -> bool {
    !runtime.portals.transits.contains_key(&actor) && runtime.portals.deliveries.is_empty()
}

pub(in crate::game_runtime) fn project_output(runtime: &mut GameRuntime) {
    runtime.project_portal_deliveries().unwrap();
}

pub(in crate::game_runtime) fn finish_output_transit(runtime: &mut GameRuntime) {
    runtime.poll_portal_controls().unwrap();
}

#[tokio::test]
async fn accepted_portal_output_retains_visibility_and_disconnect_handoff() {
    // Synthetic owner-state transcript. The source opcode phases are qualified
    // by recall_output.tsv and portal_state.csv; no DAT or client is admitted.
    let (_cluster, _directory, mut runtime, key, binding) = output_runtime().await;
    let actor = binding.actor;
    let accepted = transcript_view(actor);
    // ACE's first LoginComplete becomes ready before the login-portal exit.
    // When the server does not require components, that materialization owns
    // the private Bool68=false override in the same reliable batch.
    runtime.assets.policy.require_spell_components = false;
    let initial = bace_simulation::PortalResolution::ClientReady {
        context: bace_gameplay_api::ActionContext {
            actor,
            account: binding.account,
            session: binding.session,
            sequence: 1,
        },
        operation: 0,
        epoch: 0,
    };
    runtime.portals.controls.insert(80, initial.clone());
    // The simulation event may be visible before its separate control receipt.
    // It cannot publish materialization or lose the one-time first-entry override.
    runtime.portals.push(PortalDeliveryWork::Event(
        PortalServiceEvent::Materialized {
            operation: 0,
            actor,
            view: accepted,
        },
    ));
    runtime.project_portal_deliveries().unwrap();
    assert_eq!(runtime.portals.deliveries.len(), 1);
    assert!(runtime.network_output.is_empty());
    assert!(!runtime.portals.initial_materialized.contains_key(&key));
    runtime
        .accept_portal_control(bace_simulation::PortalResolutionOutcome {
            correlation: 80,
            resolution: initial,
            result: Ok(()),
        })
        .unwrap();
    assert_eq!(runtime.portals.initial_ready.get(&key), Some(&binding));
    runtime.project_portal_deliveries().unwrap();
    assert_eq!(
        runtime.portals.initial_materialized.get(&key),
        Some(&binding)
    );
    let NetworkCommand::SendReliableBatch {
        correlation: initial_correlation,
        messages: initial_messages,
        ..
    } = runtime.network_output.pop_front().unwrap()
    else {
        panic!("initial materialization owner batch")
    };
    assert_eq!(initial_messages.len(), 2);
    assert_eq!(opcode(&initial_messages[0].1), 0xf74b);
    assert_eq!(opcode(&initial_messages[1].1), 0x02d1);
    assert_eq!(
        u32::from_le_bytes(initial_messages[1].1[5..9].try_into().unwrap()),
        68
    );
    assert_eq!(
        u32::from_le_bytes(initial_messages[1].1[9..13].try_into().unwrap()),
        0
    );
    let observer = runtime.pending_reward_observers().unwrap();
    assert_eq!(
        observer
            .messages
            .iter()
            .map(|m| opcode(&m.bytes))
            .collect::<Vec<_>>(),
        vec![0xf74b]
    );
    runtime
        .acknowledge_reward_observers(observer.sequence)
        .unwrap();
    runtime
        .reliable_admissions
        .push_back((key, initial_correlation, true));
    runtime.poll_portal_publications().unwrap();
    // ACE's FirstEnterWorldDone makes the property override one-shot even if
    // another initial materialization notification arrives in this generation.
    runtime.portals.push(PortalDeliveryWork::Event(
        PortalServiceEvent::Materialized {
            operation: 0,
            actor,
            view: accepted,
        },
    ));
    runtime.project_portal_deliveries().unwrap();
    let NetworkCommand::SendReliableBatch {
        correlation: duplicate_correlation,
        messages: duplicate_messages,
        ..
    } = runtime.network_output.pop_front().unwrap()
    else {
        panic!("duplicate initial materialization owner batch")
    };
    assert_eq!(duplicate_messages.len(), 1);
    assert_eq!(opcode(&duplicate_messages[0].1), 0xf74b);
    let observer = runtime.pending_reward_observers().unwrap();
    assert_eq!(observer.messages.len(), 1);
    runtime
        .acknowledge_reward_observers(observer.sequence)
        .unwrap();
    runtime
        .reliable_admissions
        .push_back((key, duplicate_correlation, true));
    runtime.poll_portal_publications().unwrap();
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Hidden {
            operation: 55,
            actors: vec![actor],
            views: vec![accepted],
        }));
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Teleported {
            operation: 55,
            actors: vec![actor],
            views: vec![accepted],
        }));
    runtime.project_portal_deliveries().unwrap();
    let mut correlations = Vec::new();
    for expected in [vec![0xf755], vec![0xf751, 0xf748, 0xf74b]] {
        let NetworkCommand::SendReliableBatch {
            key: recipient,
            correlation,
            messages,
        } = runtime.network_output.pop_front().unwrap()
        else {
            panic!("canonical portal owner batch")
        };
        assert_eq!(recipient, key);
        assert_eq!(
            messages
                .iter()
                .map(|(_, bytes)| opcode(bytes))
                .collect::<Vec<_>>(),
            expected
        );
        correlations.push(correlation);
    }
    assert!(runtime.portals.publication_holds(actor));
    assert!(!runtime.portals.transits[&actor].destination_ready);
    for expected in [vec![0xf755], vec![0xf748, 0xf74b]] {
        let observer = runtime.pending_reward_observers().unwrap();
        assert_eq!(observer.actor, actor);
        assert_eq!(
            observer
                .messages
                .iter()
                .map(|m| opcode(&m.bytes))
                .collect::<Vec<_>>(),
            expected
        );
        let sequence = observer.sequence;
        runtime.acknowledge_reward_observers(sequence).unwrap();
    }
    let ready = bace_simulation::PortalResolution::DestinationReady {
        actor,
        operation: 55,
        epoch: accepted.epoch,
    };
    runtime.portals.controls.insert(81, ready.clone());
    runtime
        .accept_portal_control(bace_simulation::PortalResolutionOutcome {
            correlation: 81,
            resolution: ready,
            result: Ok(()),
        })
        .unwrap();
    assert!(runtime.portals.transits[&actor].destination_ready);
    runtime.portals.push(PortalDeliveryWork::Event(
        PortalServiceEvent::Materialized {
            operation: 55,
            actor,
            view: accepted,
        },
    ));
    runtime.project_portal_deliveries().unwrap();
    let NetworkCommand::SendReliableBatch {
        correlation,
        messages,
        ..
    } = runtime.network_output.pop_front().unwrap()
    else {
        panic!("materialization owner batch")
    };
    assert_eq!(
        messages
            .iter()
            .map(|(_, bytes)| opcode(bytes))
            .collect::<Vec<_>>(),
        vec![0xf74b]
    );
    correlations.push(correlation);
    let observer = runtime.pending_reward_observers().unwrap();
    assert_eq!(
        observer
            .messages
            .iter()
            .map(|m| opcode(&m.bytes))
            .collect::<Vec<_>>(),
        vec![0xf74b]
    );
    runtime
        .acknowledge_reward_observers(observer.sequence)
        .unwrap();
    for correlation in correlations {
        runtime
            .reliable_admissions
            .push_back((key, correlation, true));
    }
    runtime.poll_portal_publications().unwrap();
    assert!(!runtime.portals.publication_holds(actor));
    runtime.poll_portal_controls().unwrap();
    assert!(!runtime.portals.transits.contains_key(&actor));

    // A later transit exercises disconnect at the same canonical owner. It
    // cannot invent ClientReady or Materialized; detach owns completion only
    // after the private and observer publications leave their queues.
    let mut disconnected = accepted;
    disconnected.epoch = 2;
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Teleported {
            operation: 56,
            actors: vec![actor],
            views: vec![disconnected],
        }));
    runtime.project_portal_deliveries().unwrap();
    let NetworkCommand::SendReliableBatch { correlation, .. } =
        runtime.network_output.pop_front().unwrap()
    else {
        panic!("disconnected teleport owner batch")
    };
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    assert!(!runtime.disconnected_portal_handoff(key));
    runtime
        .reliable_admissions
        .push_back((key, correlation, true));
    runtime.poll_portal_publications().unwrap();
    while let Some(work) = runtime.pending_reward_observers() {
        runtime.acknowledge_reward_observers(work.sequence).unwrap();
    }
    assert!(runtime.disconnected_portal_handoff(key));
    runtime.begin_portal_disconnect(key, 99).unwrap();
    assert!(runtime.portals.has_pending());
    runtime
        .finish_portal_disconnect(key, 99, binding, true)
        .unwrap();
    assert!(!runtime.portals.has_pending());
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    runtime.sessions.get_mut(&key).unwrap().loading = None;
    runtime.poll_portal_controls().unwrap();
    assert!(!runtime.portals.initial_ready.contains_key(&key));
    assert!(!runtime.portals.initial_materialized.contains_key(&key));
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}
