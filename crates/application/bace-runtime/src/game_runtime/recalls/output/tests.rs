use super::*;
#[test]
fn original_ace_admission_notice_order_and_error_codes() {
    let kinds = [
        RecallKind::Lifestone,
        RecallKind::House,
        RecallKind::Marketplace,
        RecallKind::AllegianceHometown,
        RecallKind::AllegianceHousing,
        RecallKind::PkArena,
        RecallKind::PklArena,
    ];
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
        sequence: 1,
    };
    let mut count = 0;
    for line in include_str!("../../../../tests/fixtures/recall_output.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let (input, expected) = line.split_once('\t').unwrap();
        let values: Vec<usize> = input.split(',').map(|v| v.parse().unwrap()).collect();
        let kind = kinds[values[0]];
        let combat = values[1] != 0;
        let scenario = values[2];
        let access = bace_interactions::RecallAccess {
            pk_recent: scenario == 1,
            recalls_disabled: scenario == 2,
            busy: scenario == 3,
            olthoi: scenario == 4,
            pk_status: if scenario == 12 {
                2
            } else if kind == RecallKind::PklArena {
                64
            } else {
                4
            },
            suicide_in_progress: false,
        };
        let result = bace_interactions::check_recall(
            &bace_interactions::RecallPolicy::default(),
            kind,
            access,
        )
        .and(match (kind, scenario) {
            (RecallKind::Lifestone, 5) => Err(RecallError::NoSanctuary),
            (RecallKind::House, 6) => Err(RecallError::NoHouse),
            (RecallKind::AllegianceHometown | RecallKind::AllegianceHousing, 7) => {
                Err(RecallError::NoAllegiance)
            }
            (RecallKind::AllegianceHometown, 8) => Err(RecallError::NoHometown),
            (RecallKind::AllegianceHousing, 9) => Err(RecallError::NoMansion),
            (RecallKind::AllegianceHousing, 10) => Err(RecallError::WrongHouseType),
            (RecallKind::AllegianceHousing, 11) => Err(RecallError::MansionClosed),
            _ => Ok(()),
        });
        let event = match result {
            Ok(()) => RecallEvent::Started {
                context,
                kind,
                motion: kind.motion(),
                until_tick: 0,
                mana_after: (kind == RecallKind::Lifestone).then_some(50),
                combat_mode_changed: combat,
            },
            Err(error) => RecallEvent::Rejected { context, error },
        };
        let text = announcement("Recall Tester", kind);
        let steps = notice_steps(&event, Some(&text)).unwrap();
        let mut actual: Vec<String> = steps
            .iter()
            .map(|step| match step {
                P::Vital { vital, current } => format!("V:{vital}:{current}"),
                P::PrivateProperty {
                    property,
                    value: bace_wire::PropertyValue::Int(value),
                } => format!("P:{property}:{value}"),
                P::System { text, chat_type } => format!("C:{chat_type}:{text}"),
                P::Simple(SimpleGameEvent::WeenieError(code)) => format!("E:{code}"),
                _ => panic!("unexpected recall notice"),
            })
            .collect();
        if let RecallEvent::Started { motion, .. } = event {
            actual.push(format!("M:{motion}:{}", 0x8000003du32));
        }
        assert_eq!(actual.join("|"), expected, "{input}");
        count += 1;
    }
    assert_eq!(count, 182);
}
#[test]
fn infrastructure_rejection_cannot_be_reported_as_success_or_source_gameplay_error() {
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
        sequence: 1,
    };
    for error in [
        RecallError::Stale,
        RecallError::Capacity,
        RecallError::MissingAssets,
        RecallError::Invalid,
    ] {
        assert!(notice_steps(&RecallEvent::Rejected { context, error }, None).is_err());
    }
}

#[test]
fn cancelled_recall_emits_no_completion_or_teleport_notice() {
    // The simulation's recall_motion cancellation regression proves that no
    // PortalProposal follows Cancel. This output owner has no success packet
    // to invent when it consumes that terminal event.
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
        sequence: 1,
    };
    assert!(
        notice_steps(&RecallEvent::Cancelled { context }, None)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn retained_recall_and_portal_owners_publish_one_ordered_transcript() {
    use bace_simulation::PortalServiceEvent;

    // Synthetic accepted events enter the same runtime owners as live events.
    // The simulation cancellation test separately proves that Cancel does not
    // produce a portal proposal; this checks the adapter's terminal output.
    let (_cluster, _directory, mut runtime, key, binding) =
        crate::game_runtime::portals::tests::output_runtime().await;
    let actor = binding.actor;
    let context = ActionContext {
        actor,
        account: binding.account,
        session: binding.session,
        sequence: 1,
    };
    runtime.recalls.pending.insert(
        key,
        Pending {
            context,
            binding,
            kind: RecallKind::Lifestone,
            name: "Test Player".into(),
            phase: Phase::Submitted,
            prepared: None,
        },
    );
    runtime.recalls.event = Some(RecallEvent::Started {
        context,
        kind: RecallKind::Lifestone,
        motion: RecallKind::Lifestone.motion(),
        until_tick: 30,
        mana_after: None,
        combat_mode_changed: false,
    });
    runtime.project_recall_output().unwrap();
    let NetworkCommand::SendOrderedBatch {
        key: recipient,
        messages,
    } = runtime.network_output.pop_front().unwrap()
    else {
        panic!("recall announcement and motion owner batch")
    };
    assert_eq!(recipient, key);
    let opcode = |bytes: &[u8]| u32::from_le_bytes(bytes[..4].try_into().unwrap());
    assert_eq!(
        messages
            .iter()
            .map(|(_, bytes)| opcode(bytes))
            .collect::<Vec<_>>(),
        vec![0xf7e0, 0xf74c]
    );
    let observer = runtime.pending_reward_observers().unwrap();
    assert_eq!(
        observer
            .messages
            .iter()
            .map(|m| opcode(&m.bytes))
            .collect::<Vec<_>>(),
        vec![0xf7e0, 0xf74c]
    );
    runtime
        .acknowledge_reward_observers(observer.sequence)
        .unwrap();
    assert!(matches!(
        runtime.recalls.pending[&key].phase,
        Phase::Running
    ));

    runtime.recalls.event = Some(RecallEvent::Staged {
        context,
        kind: RecallKind::Lifestone,
        operation: 55,
    });
    runtime.project_recall_output().unwrap();
    assert!(!runtime.recalls.pending.contains_key(&key));
    assert!(runtime.network_output.is_empty());

    let accepted = bace_simulation::PortalAcceptedView {
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
    };
    crate::game_runtime::portals::tests::stage_output_event(
        &mut runtime,
        PortalServiceEvent::Hidden {
            operation: 55,
            actors: vec![actor],
            views: vec![accepted],
        },
    );
    crate::game_runtime::portals::tests::stage_output_event(
        &mut runtime,
        PortalServiceEvent::Teleported {
            operation: 55,
            actors: vec![actor],
            views: vec![accepted],
        },
    );
    crate::game_runtime::portals::tests::project_output(&mut runtime);
    let mut correlations = Vec::new();
    for expected in [vec![0xf755], vec![0xf751, 0xf748, 0xf74b]] {
        let NetworkCommand::SendReliableBatch {
            key: recipient,
            correlation,
            messages,
        } = runtime.network_output.pop_front().unwrap()
        else {
            panic!("retained portal private batch")
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
    for expected in [vec![0xf755], vec![0xf748, 0xf74b]] {
        let observer = runtime.pending_reward_observers().unwrap();
        assert_eq!(
            observer
                .messages
                .iter()
                .map(|m| opcode(&m.bytes))
                .collect::<Vec<_>>(),
            expected
        );
        runtime
            .acknowledge_reward_observers(observer.sequence)
            .unwrap();
    }
    crate::game_runtime::portals::tests::ready_output_transit(
        &mut runtime,
        actor,
        55,
        accepted.epoch,
    );
    crate::game_runtime::portals::tests::stage_output_event(
        &mut runtime,
        PortalServiceEvent::Materialized {
            operation: 55,
            actor,
            view: accepted,
        },
    );
    crate::game_runtime::portals::tests::project_output(&mut runtime);
    let NetworkCommand::SendReliableBatch {
        correlation,
        messages,
        ..
    } = runtime.network_output.pop_front().unwrap()
    else {
        panic!("materialization private batch")
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
    crate::game_runtime::portals::tests::finish_output_transit(&mut runtime);
    assert!(!runtime.portals.publication_holds(actor));
    assert!(crate::game_runtime::portals::tests::output_transit_drained(
        &runtime, actor
    ));

    // Pinned ACE Player_Location reports YouHaveMovedTooFar before Teleport.
    // Its private event must finish this running recall without a portal phase.
    let moved = ActionContext {
        sequence: 2,
        ..context
    };
    runtime.recalls.pending.insert(
        key,
        Pending {
            context: moved,
            binding,
            kind: RecallKind::Lifestone,
            name: "Test Player".into(),
            phase: Phase::Running,
            prepared: None,
        },
    );
    runtime.recalls.event = Some(RecallEvent::Rejected {
        context: moved,
        error: RecallError::MovedTooFar,
    });
    runtime.project_recall_output().unwrap();
    let NetworkCommand::SendOrderedBatch { messages, .. } =
        runtime.network_output.pop_front().unwrap()
    else {
        panic!("source movement failure private event")
    };
    assert_eq!(messages.len(), 1);
    let error = bace_wire::GameEventEnvelope::decode(&messages[0].1, 4096).unwrap();
    assert_eq!(error.object_id, actor.0);
    assert_eq!(error.event.0, 0x028a);
    assert_eq!(error.payload, 0x498u32.to_le_bytes());
    assert!(!runtime.recalls.pending.contains_key(&key));
    assert!(runtime.pending_reward_observers().is_none());
    assert!(crate::game_runtime::portals::tests::output_transit_drained(
        &runtime, actor
    ));

    let cancelled = ActionContext {
        sequence: 3,
        ..context
    };
    runtime.recalls.pending.insert(
        key,
        Pending {
            context: cancelled,
            binding,
            kind: RecallKind::Lifestone,
            name: "Test Player".into(),
            phase: Phase::Submitted,
            prepared: None,
        },
    );
    runtime.recalls.event = Some(RecallEvent::Started {
        context: cancelled,
        kind: RecallKind::Lifestone,
        motion: RecallKind::Lifestone.motion(),
        until_tick: 60,
        mana_after: None,
        combat_mode_changed: false,
    });
    runtime.project_recall_output().unwrap();
    runtime.network_output.pop_front().unwrap();
    let observer = runtime.pending_reward_observers().unwrap();
    runtime
        .acknowledge_reward_observers(observer.sequence)
        .unwrap();
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    let message_capacity = runtime.limits.messages;
    runtime.limits.messages = 0;
    runtime.recalls.event = Some(RecallEvent::Cancelled { context: cancelled });
    runtime.project_recall_output().unwrap();
    runtime.limits.messages = message_capacity;
    assert!(!runtime.recalls.pending.contains_key(&key));
    assert!(runtime.recalls.event.is_none());
    assert!(runtime.network_output.is_empty());
    assert!(crate::game_runtime::portals::tests::output_transit_drained(
        &runtime, actor
    ));

    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}
