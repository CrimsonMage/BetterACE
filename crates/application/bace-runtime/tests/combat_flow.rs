use std::sync::Arc;

use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_entity::Actor;
use bace_entity::{Combatant, CombatantProfile};
use bace_gameplay_api::{
    ActionContext, AttributeId, CharacterBinding, ProgressionTarget, SessionId, SkillAdvancement,
};
use bace_gameplay_api::{CombatChange, CombatRejection, CombatRequest};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_runtime::simulation::{SimulationConfig, SimulationWorker, WorkerError};
use bace_simulation::CombatEvent;
use bace_simulation::{Command, Kernel};
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;

fn kernel(commands: usize, characters: usize, outcomes: usize) -> Kernel {
    kernel_with_actors(
        commands,
        characters,
        outcomes,
        [1, 2],
        None,
        false,
        binding(),
    )
}
fn kernel_with_actors(
    commands: usize,
    characters: usize,
    outcomes: usize,
    ids: [u32; 2],
    level: Option<i32>,
    second_player: bool,
    initial_binding: CharacterBinding,
) -> Kernel {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for (index, id) in ids.into_iter().enumerate() {
        let body = Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new((index + 1) as f32, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
    }
    for (index, id) in ids.into_iter().enumerate() {
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 10,
                    melee_damage: 20,
                    melee_range: 2.0,
                    attack_duration: 0.1,
                    strike_offsets: vec![0.05],
                    player: index == 0 || second_player,
                })
                .unwrap(),
            )
            .unwrap();
    }
    if let Some(level) = level {
        for id in ids {
            let mut properties = bace_entity::EntityProperties::new(8).unwrap();
            let change = properties
                .propose(
                    bace_entity::PropertyFamily::Int,
                    25,
                    Some(bace_entity::PropertyValue::Int(level)),
                )
                .unwrap();
            properties.adopt(change).unwrap();
            world.register_properties(EntityId(id), properties).unwrap();
        }
    }
    let mut kernel = Kernel::with_gameplay_limits(world, commands, characters, outcomes).unwrap();
    kernel
        .register_character(
            CharacterBinding {
                actor: EntityId(ids[0]),
                ..initial_binding
            },
            character(),
        )
        .unwrap();
    kernel
}

fn character() -> CharacterProgression {
    let table = RankTable::new(&[0, 1, 10, 100]).unwrap();
    CharacterProgression::new(
        &[TraitProgress {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            experience_spent: 0,
            advancement: SkillAdvancement::Inactive,
        }],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        100,
        0,
    )
    .unwrap()
}

fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(9),
        account: AccountId(4),
        actor: EntityId(1),
    }
}

fn context(sequence: u32) -> ActionContext {
    let binding = binding();
    ActionContext {
        session: binding.session,
        account: binding.account,
        actor: binding.actor,
        sequence,
    }
}

fn combat(sequence: u32, request: CombatRequest) -> Command {
    Command::Combat {
        context: context(sequence),
        request,
    }
}
fn attack(sequence: u32) -> Command {
    combat(
        sequence,
        CombatRequest::TargetedMelee {
            target: EntityId(2),
            height: 2,
            power: 0.5,
        },
    )
}
#[test]
fn stalled_combat_reply_consumer_preserves_every_request_and_ticks_continue() {
    let mut kernel = kernel(8, 2, 1);
    for seq in 1..=8 {
        kernel
            .enqueue(combat(seq, CombatRequest::QueryHealth(EntityId(2))))
            .unwrap();
    }
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 8,
            tick_limit: Some(20),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut exit = worker.wait_recover().unwrap();
    assert_eq!(exit.report.ticks, 20);
    assert!(exit.failure.is_none());
    let mut outcomes = exit.undelivered_combat_outcomes;
    while let Some(outcome) = exit.kernel.take_combat_outcome() {
        outcomes.push(outcome);
    }
    assert_eq!(outcomes.len() + exit.kernel.queued_commands(), 8);
    assert!(exit.unprocessed_commands.is_empty());
    for _ in 0..8 {
        exit.kernel.step().unwrap();
        while let Some(outcome) = exit.kernel.take_combat_outcome() {
            outcomes.push(outcome);
        }
    }
    assert_eq!(
        outcomes
            .iter()
            .map(|o| o.context.sequence)
            .collect::<Vec<_>>(),
        (1..=8).collect::<Vec<_>>()
    );
    assert!(outcomes.iter().all(|o| o.result
        == Ok(CombatChange::Health {
            target: EntityId(2),
            current: 10,
            maximum: 10
        })));
    assert_eq!(
        exit.kernel
            .world()
            .combatant(EntityId(2))
            .unwrap()
            .revision(),
        0
    );
}
#[test]
fn damage_death_completion_and_replies_survive_both_stalled_output_streams() {
    let mut kernel = kernel(8, 2, 8);
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel.enqueue(attack(2)).unwrap();
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 8,
            tick_limit: Some(20),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let exit = worker.wait_recover().unwrap();
    assert_eq!(exit.report.ticks, 20);
    assert!(exit.failure.is_none());
    assert_eq!(exit.undelivered_combat_outcomes.len(), 2);
    assert_eq!(
        exit.undelivered_combat_outcomes[1].result,
        Ok(CombatChange::AttackStarted {
            target: EntityId(2)
        })
    );
    assert!(matches!(
        exit.undelivered_combat_events.as_slice(),
        [
            CombatEvent::Damage {
                attacker: Some(EntityId(1)),
                target: EntityId(2),
                amount: 10,
                current: 0,
                maximum: 10,
                killed: true,
                revision: 1,
                ..
            },
            CombatEvent::Finished {
                actor: EntityId(1),
                cancelled: true,
                ..
            }
        ]
    ));
    assert_eq!(
        exit.kernel.world().combatant(EntityId(2)).unwrap().health(),
        0
    );
    assert_eq!(
        exit.kernel
            .world()
            .combatant(EntityId(2))
            .unwrap()
            .revision(),
        1
    );
}
#[test]
fn report_only_exit_refuses_to_drop_a_combat_rejection_without_registered_characters() {
    let mut kernel = Kernel::new(World::default(), 2).unwrap();
    kernel
        .enqueue(combat(1, CombatRequest::CancelAttack))
        .unwrap();
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 2,
            tick_limit: Some(1),
            real_time: false,
        },
    )
    .unwrap();
    let Err(WorkerError::RecoveryRequired(exit)) = worker.wait() else {
        panic!("combat output requires recovery")
    };
    assert_eq!(exit.undelivered_combat_outcomes.len(), 1);
    assert_eq!(
        exit.undelivered_combat_outcomes[0].result,
        Err(CombatRejection::NotBound)
    );
}

use bace_runtime::network::*;
use bace_wire::*;
use std::{net::UdpSocket, time::Duration};
fn login() -> Vec<u8> {
    let hex = "0400313830320000E703000002000000010000003930000007004163636F756E7400000000000000130000001273796E7468657469632D70617373776F72640000";
    let body: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect();
    encode_server_packet(
        PacketHeader {
            flags: flags::LOGIN_REQUEST,
            ..Default::default()
        },
        &body,
        &[],
        0,
    )
    .unwrap()
}
fn socket() -> UdpSocket {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s
}
fn net_event(net: &NetworkThread) -> NetworkEvent {
    net.events().recv_timeout(Duration::from_secs(3)).unwrap()
}

#[test]
fn udp_health_query_reaches_single_owner_and_returns_exact_health_event() {
    // This validates adapter composition with admitted synthetic actors. It is
    // not evidence of a stock-client login or DAT geometry admission.
    let net = (0..20)
        .find_map(|_| {
            NetworkThread::spawn(NetworkThreadConfig {
                bind_address: "127.0.0.1:0".parse().unwrap(),
                ..Default::default()
            })
            .ok()
        })
        .expect("port pair");
    let client = socket();
    let receiver = socket();
    client.send_to(&login(), net.client_address).unwrap();
    let key = match net_event(&net) {
        NetworkEvent::Login { key, .. } => key,
        event => panic!("{event:?}"),
    };
    net.try_send(NetworkCommand::Authenticated {
        key,
        account_id: AccountId(1),
    })
    .unwrap();
    let mut bytes = [0; 1025];
    let (n, _) = client.recv_from(&mut bytes).unwrap();
    let challenge = ConnectRequest::decode(Datagram::decode(&bytes[..n]).unwrap().body).unwrap();
    let response = encode_server_packet(
        PacketHeader {
            flags: flags::CONNECT_RESPONSE,
            ..Default::default()
        },
        &challenge.cookie.to_le_bytes(),
        &[],
        0,
    )
    .unwrap();
    receiver.send_to(&response, net.server_address).unwrap();
    assert!(matches!(net_event(&net),NetworkEvent::Connected{key:k} if k==key));
    let active = CharacterBinding {
        session: SessionId(key.generation),
        account: AccountId(1),
        actor: EntityId(1),
    };
    let kernel = kernel_with_actors(8, 2, 8, [1, 2], None, false, active);
    let sim = SimulationWorker::spawn(kernel, SimulationConfig::default()).unwrap();
    net.try_send(NetworkCommand::EnterWorldCommitted { key })
        .unwrap();
    let action = GameActionEnvelope {
        sequence: 1,
        action: opcode::GameActionType::QueryHealth,
        payload: &2u32.to_le_bytes(),
    }
    .encode(128)
    .unwrap();
    let fragments = bace_transport::fragment_message(1, 9, &action, 128).unwrap();
    let mut keys = Isaac::new(challenge.client_seed);
    let packet = encode_server_packet(
        PacketHeader {
            sequence: 2,
            flags: flags::ENCRYPTED_CHECKSUM | flags::BLOB_FRAGMENTS,
            id: key.id,
            iteration: 1,
            ..Default::default()
        },
        &[],
        &fragments,
        keys.next_key(),
    )
    .unwrap();
    client.send_to(&packet, net.client_address).unwrap();
    let message = match net_event(&net) {
        NetworkEvent::Message { key: k, message } => {
            assert_eq!(k, key);
            message
        }
        other => panic!("{other:?}"),
    };
    let decoded = bace_session::decode_combat(
        bace_session::SessionState::WorldConnected,
        ActionContext {
            session: active.session,
            account: active.account,
            actor: active.actor,
            sequence: 0,
        },
        &message.bytes,
        128,
    )
    .unwrap();
    sim.input()
        .try_submit(Command::Combat {
            context: decoded.context,
            request: decoded.request,
        })
        .unwrap();
    let outcome = sim
        .combat_outcomes()
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    assert_eq!(outcome.context.sequence, 1);
    let CombatChange::Health {
        target,
        current,
        maximum,
    } = outcome.result.unwrap()
    else {
        panic!("health response")
    };
    let mut projector = bace_replication::EventSequencer::new(active, 1);
    let batch = projector
        .project_combat(
            active,
            &[bace_wire::CombatEvent::UpdateHealth {
                target_id: target.0,
                fraction: current as f32 / maximum as f32,
            }],
            bace_replication::BatchLimits {
                max_messages: 4,
                max_bytes: 1024,
                max_message_bytes: 1024,
                max_string_bytes: 128,
            },
        )
        .unwrap();
    net.try_send(bace_runtime::game_messages::session_batch_command(key, batch).unwrap())
        .unwrap();
    let mut incoming = ClientKeys::new(challenge.server_seed);
    let response = loop {
        let (n, _) = receiver.recv_from(&mut bytes).unwrap();
        let packet = bace_transport::decode_transport_packet(&bytes[..n], &mut incoming).unwrap();
        if let Some(fragment) = packet.fragments.into_iter().next() {
            break fragment;
        }
    };
    assert_eq!(response.header.queue, 9);
    let event = GameEventEnvelope::decode(&response.data, 128).unwrap();
    assert_eq!(event.object_id, 1);
    assert_eq!(event.sequence, 1);
    assert_eq!(event.event, opcode::GameEventType::UpdateHealth);
    assert_eq!(event.payload, [2, 0, 0, 0, 0, 0, 128, 63]);
    let exit = sim.shutdown_recover().unwrap();
    assert!(exit.failure.is_none());
    assert_eq!(
        exit.kernel.world().combatant(EntityId(2)).unwrap().health(),
        10
    );
    net.shutdown().unwrap();
}

use bace_simulation::{NpcBlueprint, NpcLootEntry, PveEvent};
fn npc() -> NpcBlueprint {
    NpcBlueprint {
        cell: CellId(1),
        position: Vec3::new(2.0, 0.0, 0.5),
        radius: 0.5,
        capabilities: Capabilities {
            speed: 1.0,
            jump_impulse: 1.0,
        },
        combat: CombatantProfile {
            maximum_health: 20,
            melee_damage: 2,
            melee_range: 2.0,
            attack_duration: 0.1,
            strike_offsets: vec![0.05],
            player: false,
        },
        visual_range: 10.0,
        think_interval: 1,
        corpse_template: 9000,
        xp_override: Some(100),
        loot: vec![
            NpcLootEntry {
                template: 42,
                destination: 8,
                probability: 0.25,
                stack: 1,
            },
            NpcLootEntry {
                template: 43,
                destination: 8,
                probability: 0.75,
                stack: 2,
            },
        ],
        death_animation_ticks: 2,
        respawn_ticks: 3,
        corpse_decay_ticks: 5,
    }
}

#[test]
fn pve_death_proposals_and_corpse_effects_remain_owned_across_worker_recovery() {
    let mut kernel = kernel(16, 2, 16);
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    kernel.supply_loot_random(&[0.8]).unwrap();
    kernel.supply_spawn_id(EntityId(4)).unwrap();
    kernel
        .enqueue(combat(1, CombatRequest::ChangeMode(2)))
        .unwrap();
    kernel
        .enqueue(combat(
            2,
            CombatRequest::TargetedMelee {
                target: EntityId(3),
                height: 2,
                power: 0.5,
            },
        ))
        .unwrap();
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 16,
            tick_limit: Some(20),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut exit = worker.wait_recover().unwrap();
    assert!(exit.failure.is_none());
    assert_eq!(exit.undelivered_death_proposals.len(), 1);
    let proposal = exit.undelivered_death_proposals.pop().unwrap();
    assert_eq!(proposal.victim, EntityId(3));
    assert_eq!(
        proposal.drops,
        [bace_simulation::LootDrop {
            template: 43,
            stack: 2
        }]
    );
    assert_eq!(exit.kernel.pending_deaths(), 1);
    assert_eq!(
        exit.kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        100
    );
    // Synthetic persistence receipt: this test verifies worker ownership/recovery,
    // not database commit. Production must commit this exact XP/item proposal first.
    exit.kernel
        .confirm_death_committed(
            proposal.operation,
            EntityId(1000),
            &[EntityId(1001)],
            &proposal.experience_state,
        )
        .unwrap();
    let worker = SimulationWorker::spawn_with_output_capacity(
        exit.kernel,
        SimulationConfig {
            command_capacity: 16,
            tick_limit: Some(30),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut recovered = worker.wait_recover().unwrap();
    let mut effects = recovered.undelivered_pve_events;
    while let Some(effect) = recovered.kernel.take_pve_event() {
        effects.push(effect)
    }
    assert!(effects.contains(&PveEvent::CorpseCreated {
        operation: proposal.operation,
        corpse: EntityId(1000)
    }));
    assert!(effects.contains(&PveEvent::Respawned {
        previous: EntityId(3),
        actor: EntityId(4)
    }));
    assert!(!effects.contains(&PveEvent::CorpseDecayed {
        corpse: EntityId(1000)
    }));
    assert_eq!(
        recovered
            .kernel
            .world()
            .corpse(EntityId(1000))
            .unwrap()
            .items,
        [EntityId(1001)]
    );
    // Loot remains durable-owned; only a confirmed transfer permits decay.
    assert!(
        recovered
            .kernel
            .confirm_corpse_item_removed(EntityId(1000), EntityId(1001))
    );
    let worker = SimulationWorker::spawn_with_output_capacity(
        recovered.kernel,
        SimulationConfig {
            command_capacity: 16,
            tick_limit: Some(2),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let recovered = worker.wait_recover().unwrap();
    assert!(
        recovered
            .undelivered_pve_events
            .contains(&PveEvent::CorpseDecayed {
                corpse: EntityId(1000)
            })
    );
    assert_eq!(
        recovered
            .kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        200
    );
}
#[test]
fn report_only_exit_retains_npc_population_even_without_a_player_binding() {
    let mut kernel = kernel(16, 2, 16);
    kernel.take_character(binding()).unwrap();
    kernel.spawn_npc(EntityId(3), npc()).unwrap();
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 16,
            tick_limit: Some(1),
            real_time: false,
        },
    )
    .unwrap();
    let Err(WorkerError::RecoveryRequired(exit)) = worker.wait() else {
        panic!("NPC population requires owner recovery")
    };
    assert!(exit.kernel.has_pve_state());
}

use bace_gameplay_api::{DoorChange, UseDoor};
use bace_simulation::{PreparedDoor, PreparedDoorAnimation, PreparedDoorHook};
use bace_world::DoorCollider;
fn door() -> PreparedDoor {
    PreparedDoor {
        collider: DoorCollider {
            cell: CellId(1),
            position: Vec3::new(3.1, 0.0, 0.5),
            bounds: Aabb::new(Vec3::new(3.0, -1.0, 0.0), Vec3::new(3.2, 1.0, 2.0)).unwrap(),
            solid: true,
        },
        initially_open: false,
        initially_locked: false,
        reset_interval_ticks: None,
        use_radius: 5.0,
        open: PreparedDoorAnimation {
            duration_ticks: 2,
            hooks: vec![PreparedDoorHook {
                offset_ticks: 1,
                ethereal: true,
            }],
        },
        close: PreparedDoorAnimation {
            duration_ticks: 2,
            hooks: vec![PreparedDoorHook {
                offset_ticks: 1,
                ethereal: false,
            }],
        },
    }
}
fn use_door(sequence: u32) -> Command {
    Command::UseDoor {
        context: context(sequence),
        request: UseDoor { door: EntityId(10) },
    }
}
#[test]
fn door_motion_physics_and_correlated_replies_survive_stalled_consumers() {
    let mut kernel = kernel(16, 2, 16);
    kernel.register_door(EntityId(10), door()).unwrap();
    kernel.enqueue(use_door(1)).unwrap();
    kernel.enqueue(use_door(2)).unwrap();
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 16,
            tick_limit: Some(20),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let exit = worker.wait_recover().unwrap();
    assert!(exit.failure.is_none());
    assert_eq!(exit.undelivered_door_outcomes.len(), 2);
    assert!(matches!(
        exit.undelivered_door_outcomes[0].result,
        Ok(DoorChange::Motion {
            door: EntityId(10),
            open: true,
            ..
        })
    ));
    assert_eq!(
        exit.undelivered_door_outcomes[1].result,
        Ok(DoorChange::Busy)
    );
    assert!(
        exit.undelivered_door_events
            .iter()
            .any(|e| e.motion.is_some_and(|m| m.open))
    );
    assert!(
        exit.undelivered_door_events
            .iter()
            .any(|e| e.physics.is_some_and(|p| p.ethereal))
    );
    assert!(!exit.kernel.world().door(EntityId(10)).unwrap().solid);
}

#[path = "combat_flow/streams.rs"]
mod streams;

#[path = "combat_flow/native_loot.rs"]
mod native_loot;

#[path = "combat_flow/validation.rs"]
mod validation;
