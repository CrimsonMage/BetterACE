use super::*;
use bace_admin::{AuthorizedCommand, command_catalog::command, prepare_shard_command};
use bace_auth::{CharacterPrivileges, StaffPrincipal};
use bace_types::EntityId;
use serde_json::{Value, json};
fn prepared(name: &str, args: Vec<String>) -> PreparedShardCommand {
    prepare_shard_command(&AuthorizedCommand {
        spec: command(name).unwrap(),
        raw_arguments: args.join(" "),
        arguments: args,
        sudo: false,
    })
    .unwrap()
    .unwrap()
}
fn clock(ms: u64) -> ShardClock {
    ShardClock {
        monotonic_millis: ms,
        unix_millis: 1_728_432_000_000 + ms as i64,
        local_offset_seconds: 0,
    }
}
fn principal(access: AccessLevel) -> StaffPrincipal {
    StaffPrincipal {
        account_access: access,
        character: CharacterPrivileges::from_account(access),
        in_world: true,
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/shard_commands.json")).unwrap()
}
fn event(e: &ShardEvent) -> Value {
    let (kind, text) = match &e.effect {
        ShardEffect::Reply { chat, text, .. } => (
            match chat {
                ShardChat::Broadcast => "ReplyBroadcast",
                ShardChat::WorldBroadcast => "ReplyWorldBroadcast",
            },
            text,
        ),
        ShardEffect::Broadcast { text } => ("Broadcast", text),
        ShardEffect::Audit { text, .. } => ("Audit", text),
        ShardEffect::Log { text } => ("Log", text),
    };
    json!({"kind":kind,"text":text})
}
#[test]
fn unchanged_compiled_ace_handlers_and_interval_parser() {
    let fixture = fixture();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut owner = ShardControl::new(32, input["opened"].as_bool().unwrap()).unwrap();
        owner.state.interval = input["interval"].as_u64().unwrap() as u32;
        if input["pending"].as_bool().unwrap() {
            owner.state.shutdown = ShardShutdown::Countdown {
                deadline_millis: 60_000,
            };
            owner.state.deadline_unix = clock(60_000).unix_millis;
        }
        let name = input["name"].as_str();
        let issuer = match name {
            None => ShardIssuer::HostConsole,
            Some(name) => ShardIssuer::Player {
                actor: EntityId(7),
                name,
                principal: principal(AccessLevel::Admin),
            },
        };
        let cmd = prepared(
            input["command"].as_str().unwrap(),
            input["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
        );
        owner.apply(&cmd, issuer, clock(0)).unwrap();
        let actual: Vec<_> = owner.events.iter().map(event).collect();
        let expected = case["events"].as_array().unwrap();
        // Original handler returns before its worker emits the cancellation
        // broadcast; compare that source handler prefix, then verify worker below.
        if input["command"] == "cancel-shutdown" && input["pending"] == true {
            assert_eq!(&actual[..expected.len()], expected, "{input}");
            assert_eq!(actual.len(), expected.len() + 2);
            assert_eq!(
                actual.last().unwrap()["text"],
                "Broadcast from System> ATTENTION - This Asheron's Call Server shut down has been cancelled."
            );
        } else {
            assert_eq!(&actual, expected, "{input}");
        }
        assert_eq!(
            owner.interval(),
            case["interval"].as_u64().unwrap() as u32,
            "{input}"
        );
        assert_eq!(
            owner.world_open(),
            case["opened"].as_bool().unwrap(),
            "{input}"
        );
        assert_eq!(
            matches!(owner.shutdown(), ShardShutdown::Countdown { .. }),
            case["pending"].as_bool().unwrap(),
            "{input}"
        );
        let boot = !case["boots"].as_array().unwrap().is_empty();
        assert_eq!(
            owner.peek_drain().map(|r| r.kind),
            boot.then_some(ShardDrainKind::BootOrdinaryPlayers)
        );
        if boot {
            assert_eq!(
                case["boots"],
                json!(["Player| because the world is now closed|The world is now closed"])
            );
        }
    }
}
#[test]
fn unchanged_compiled_ace_periodic_notice_windows_and_dates() {
    let fixture = fixture();
    for case in fixture["notices"].as_array().unwrap() {
        let input = &case["input"];
        let now = 100_000_000;
        let remaining = input["remaining"].as_u64().unwrap();
        let mut owner = ShardControl::new(32, true).unwrap();
        owner.state.shutdown = ShardShutdown::Countdown {
            deadline_millis: now + remaining - 1000,
        };
        owner.state.last_notice = now - input["elapsed"].as_u64().unwrap();
        owner.advance(clock(now)).unwrap();
        assert_eq!(
            owner.events.iter().map(event).collect::<Vec<_>>(),
            *case["events"].as_array().unwrap(),
            "{input}"
        );
        assert_eq!(
            owner.state.last_notice == now,
            case["updated"].as_bool().unwrap(),
            "{input}"
        );
    }
    for case in fixture["dates"].as_array().unwrap() {
        let (local, utc) = time::labels(case["unix"].as_i64().unwrap(), 0).unwrap();
        assert_eq!(local, case["text"].as_str().unwrap());
        assert_eq!(utc, local);
    }
}
#[test]
fn stale_authority_and_output_backpressure_never_partially_apply() {
    let mut owner = ShardControl::new(8, true).unwrap();
    let cmd = prepared("stop-now", vec!["maintenance".into()]);
    let issuer = ShardIssuer::Player {
        actor: EntityId(7),
        name: "Admin",
        principal: principal(AccessLevel::Player),
    };
    assert_eq!(
        owner.apply(&cmd, issuer, clock(0)),
        Err(ShardError::NotAuthorized)
    );
    assert_eq!(owner.shutdown(), ShardShutdown::Idle);
    assert_eq!(owner.interval(), 60);
    assert!(owner.events.is_empty());
    owner
        .apply(
            &prepared("world", vec![]),
            ShardIssuer::HostConsole,
            clock(0),
        )
        .unwrap();
    // stop-now needs eight retained output slots. One occupied slot rejects the
    // entire command, including interval reset and the shutdown countdown.
    assert_eq!(
        owner.apply(&cmd, ShardIssuer::HostConsole, clock(1)),
        Err(ShardError::Capacity)
    );
    assert_eq!(owner.interval(), 60);
    assert_eq!(owner.shutdown(), ShardShutdown::Idle);
    assert_eq!(owner.events.len(), 1);
    let seq = owner.peek_event().unwrap().sequence;
    assert_eq!(
        owner.acknowledge_event(seq + 1),
        Err(ShardError::StaleReceipt)
    );
    owner.acknowledge_event(seq).unwrap();
    owner
        .apply(&cmd, ShardIssuer::HostConsole, clock(1))
        .unwrap();
    assert_eq!(owner.events.len(), 8);
}
#[test]
fn existing_deadline_is_immutable_and_draining_requires_exact_durable_ack() {
    let mut owner = ShardControl::new(64, true).unwrap();
    owner
        .apply(
            &prepared("shutdown", vec![]),
            ShardIssuer::HostConsole,
            clock(0),
        )
        .unwrap();
    owner
        .apply(
            &prepared("stop-now", vec![]),
            ShardIssuer::HostConsole,
            clock(1),
        )
        .unwrap();
    assert_eq!(owner.interval(), 0);
    assert_eq!(
        owner.shutdown(),
        ShardShutdown::Countdown {
            deadline_millis: 60_000
        }
    );
    owner.advance(clock(60_000)).unwrap();
    assert!(owner.peek_drain().is_none());
    owner.advance(clock(60_001)).unwrap();
    assert!(!owner.accepts_players());
    for kind in [
        ShardDrainKind::ShutdownPlayers,
        ShardDrainKind::DisconnectSessions,
        ShardDrainKind::UnloadRegions,
        ShardDrainKind::StopWorld,
    ] {
        let request = owner.peek_drain().unwrap();
        assert_eq!(request.kind, kind);
        let mut receipt = ShardDrainReceipt {
            request,
            remaining: 0,
            unsaved: 1,
            pending_operations: 0,
        };
        assert_eq!(
            owner.confirm_drain(receipt),
            Err(ShardError::DurabilityPending)
        );
        owner.advance(clock(60_001 + 600_000)).unwrap();
        assert_eq!(owner.peek_drain(), Some(request));
        receipt.unsaved = 0;
        receipt.pending_operations = 1;
        assert_eq!(
            owner.confirm_drain(receipt),
            Err(ShardError::DurabilityPending)
        );
        receipt.pending_operations = 0;
        owner.confirm_drain(receipt).unwrap();
        assert_eq!(owner.confirm_drain(receipt), Err(ShardError::StaleReceipt));
    }
    assert_eq!(owner.shutdown(), ShardShutdown::Complete);
    assert!(owner.peek_drain().is_none());
}
#[test]
fn world_boot_is_retained_and_no_cancel_resurrects_draining_world() {
    let mut owner = ShardControl::new(64, true).unwrap();
    let close = prepared("world", vec!["close".into(), "boot".into()]);
    owner
        .apply(&close, ShardIssuer::HostConsole, clock(0))
        .unwrap();
    let boot = owner.peek_drain().unwrap();
    owner
        .apply(&close, ShardIssuer::HostConsole, clock(1))
        .unwrap();
    assert_eq!(owner.drains.len(), 1);
    owner
        .apply(
            &prepared("stop-now", vec![]),
            ShardIssuer::HostConsole,
            clock(2),
        )
        .unwrap();
    owner.advance(clock(3)).unwrap();
    assert_eq!(owner.drains.len(), 2);
    owner
        .apply(
            &prepared("cancel-shutdown", vec![]),
            ShardIssuer::HostConsole,
            clock(4),
        )
        .unwrap();
    assert_eq!(
        owner.shutdown(),
        ShardShutdown::Draining(ShardDrainKind::ShutdownPlayers)
    );
    owner
        .confirm_drain(ShardDrainReceipt {
            request: boot,
            remaining: 0,
            unsaved: 0,
            pending_operations: 0,
        })
        .unwrap();
    assert_eq!(
        owner.peek_drain().unwrap().kind,
        ShardDrainKind::ShutdownPlayers
    );
    assert_eq!(owner.advance(clock(3)), Err(ShardError::Clock));
}

#[test]
fn boot_account_filter_and_clock_overflow_retain_all_state() {
    for access in [
        AccessLevel::Player,
        AccessLevel::Advocate,
        AccessLevel::Sentinel,
        AccessLevel::Envoy,
        AccessLevel::Developer,
        AccessLevel::Admin,
    ] {
        assert_eq!(
            ShardDrainKind::BootOrdinaryPlayers.includes_player(access),
            access == AccessLevel::Player
        );
        assert!(ShardDrainKind::ShutdownPlayers.includes_player(access));
        assert!(!ShardDrainKind::UnloadRegions.includes_player(access));
    }
    assert_eq!(
        ShardDrainKind::BootOrdinaryPlayers.boot_message(),
        Some((
            " because the world is now closed",
            "The world is now closed"
        ))
    );
    let mut owner = ShardControl::new(8, true).unwrap();
    let mut now = clock(0);
    now.monotonic_millis = u64::MAX;
    assert_eq!(
        owner.apply(&prepared("shutdown", vec![]), ShardIssuer::HostConsole, now),
        Err(ShardError::Clock)
    );
    assert_eq!(owner.shutdown(), ShardShutdown::Idle);
    assert!(owner.events.is_empty());
    now = clock(0);
    now.unix_millis = 253_402_300_799_999;
    assert_eq!(
        owner.apply(&prepared("shutdown", vec![]), ShardIssuer::HostConsole, now),
        Err(ShardError::Clock)
    );
    assert!(owner.events.is_empty());
}
