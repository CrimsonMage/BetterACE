use super::*;

const SOURCE_STONE: EntityId = EntityId(2_136_313_869);

/// Move one accepted authored source row into a private, DAT-validated starter
/// cell. The source pack is never changed, and the old index loses this exact
/// GUID in the private generation, so there is still only one stone owner.
fn relocated_bindstone_records() -> (Vec<bace_storage_codec::PackRecord>, u32, u32) {
    use bace_content::WorldRecordV1;
    use bace_storage_codec::{PackKey, PackLookup, PackRecord};

    let dat = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let manifest = crate::region_activation::RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut portal = bace_dat::DatArchive::open(&manifest.portal).unwrap();
    let chargen =
        bace_dat::CharGen::decode(&portal.read(bace_dat::CharGen::RECORD_ID).unwrap()).unwrap();
    let (area, shoushi) = chargen
        .starter_areas
        .iter()
        .enumerate()
        .find(|(_, area)| area.name == "Shoushi")
        .unwrap();
    let start = shoushi.locations[0];
    let block = (start.cell >> 16) as u16;

    let path = std::path::PathBuf::from(std::env::var_os("BACE_WORLD_MANIFEST").unwrap());
    let accepted = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let generation = accepted
        .open(path.parent().unwrap(), Default::default())
        .unwrap();
    let lookup = |namespace, id| match generation.lookup(PackKey { namespace, id }).unwrap() {
        PackLookup::Record(record) => record.bytes().to_vec(),
        _ => panic!("accepted bindstone source {namespace}:{id} missing"),
    };
    let WorldRecordV1::LandblockInstance(mut stone) =
        bace_content_tools::decode_world_record(&lookup(20, SOURCE_STONE.0 as u64)).unwrap()
    else {
        panic!("accepted bindstone source row");
    };
    let original_block = (stone.obj_cell_id >> 16) as u16;
    assert_eq!(original_block, 0xf559);
    let template: bace_content::WeenieV1 =
        bace_content_tools::decode(&lookup(1, stone.weenie_class_id as u64)).unwrap();
    assert_eq!(template.weenie_type, 65);
    assert!(template.properties.strings.iter().any(|p| p.id == 18));
    let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest).unwrap();
    let (geometry, visibility) = assets.prepare_geometry_with_visibility(block).unwrap();
    let physical = assets.prepare_template(&template).unwrap();
    let visible = visibility
        .iter()
        .find(|row| row.cell.0 == start.cell)
        .map(|row| {
            row.visible_cells
                .iter()
                .map(|cell| cell.0)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let existing = crate::world_content::prepare(&generation, block).unwrap();
    let (cell, position) = [
        (2.0, 0.0),
        (-2.0, 0.0),
        (0.0, 2.0),
        (0.0, -2.0),
        (1.8, 1.8),
        (-1.8, 1.8),
        (1.8, -1.8),
        (-1.8, -1.8),
    ]
    .into_iter()
    .find_map(|(dx, dy)| {
        let position =
            bace_geometry::Vec3::new(start.origin[0] + dx, start.origin[1] + dy, start.origin[2]);
        if existing.instances.iter().any(|root| {
            let source = &root.source;
            (source.origin_z - position.z).abs() < 2.0
                && (source.origin_x - position.x).powi(2) + (source.origin_y - position.y).powi(2)
                    < 2.25
        }) {
            return None;
        }
        let cell = geometry
            .placement_cell(start.cell, position, &physical.shape, &visible)
            .ok()?;
        geometry
            .validate_placement(cell, position, &physical.shape, &[], SOURCE_STONE.0)
            .ok()?;
        Some((cell, position))
    })
    .expect("approved DAT has a clear bindstone placement near starter pose");

    let mut old_index =
        bace_content_tools::decode_landblock_index(&lookup(2, original_block as u64)).unwrap();
    assert!(old_index.instance_ids.contains(&SOURCE_STONE.0));
    old_index.instance_ids.retain(|id| *id != SOURCE_STONE.0);
    assert!(!old_index.instance_ids.contains(&SOURCE_STONE.0));
    let mut new_index =
        bace_content_tools::decode_landblock_index(&lookup(2, block as u64)).unwrap();
    assert!(!new_index.instance_ids.contains(&SOURCE_STONE.0));
    new_index.instance_ids.push(SOURCE_STONE.0);
    new_index.instance_ids.sort_unstable();
    assert!(new_index.instance_ids.contains(&SOURCE_STONE.0));
    stone.landblock = i32::from(block);
    stone.obj_cell_id = cell;
    stone.origin_x = position.x;
    stone.origin_y = position.y;
    stone.origin_z = position.z;
    let mut records = vec![
        PackRecord {
            key: PackKey {
                namespace: 2,
                id: original_block as u64,
            },
            schema: 1,
            value: Some(bace_content_tools::compile_landblock_index(&old_index).unwrap()),
        },
        PackRecord {
            key: PackKey {
                namespace: 2,
                id: block as u64,
            },
            schema: 1,
            value: Some(bace_content_tools::compile_landblock_index(&new_index).unwrap()),
        },
        PackRecord {
            key: PackKey {
                namespace: 20,
                id: SOURCE_STONE.0 as u64,
            },
            schema: 1,
            value: Some(
                bace_content_tools::compile_world_record(&WorldRecordV1::LandblockInstance(stone))
                    .unwrap(),
            ),
        },
    ];
    records.sort_by_key(|record| record.key);
    (records, area as u32, cell)
}

#[test]
#[ignore = "requires approved DATs and accepted full native pack"]
fn approved_dat_relocated_bindstone_has_one_valid_source_cell() {
    let (records, _area, cell) = relocated_bindstone_records();
    assert_eq!(records.len(), 3);
    assert_eq!(cell >> 16, 0x7f03);
    assert_eq!(records[2].key.namespace, 20);
    assert_eq!(records[2].key.id, SOURCE_STONE.0 as u64);
}

#[tokio::test]
async fn allegiance_bindstone_action_and_committed_message_have_exact_output_owners() {
    use bace_wire::{ObjectDescription, ObjectGameData, ObjectGameOptions, ObjectModel};
    use bace_wire::{PhysicsDescription, PhysicsOptions};

    let (_cluster, _directory, mut runtime, key, binding) =
        crate::game_runtime::portals::tests::output_runtime().await;
    runtime.players.test_mark_entered(key, binding).unwrap();
    let object = EntityId(0x7000_0065);
    runtime
        .register_binding_presentation(object, BindingKind::Allegiance)
        .unwrap();
    let sequences = bace_replication::Sequences::new(10).unwrap();
    runtime
        .visibility
        .service
        .register_object(
            1,
            1,
            0,
            Arc::new(ObjectDescription {
                object_id: object.0,
                model: ObjectModel::default(),
                physics: PhysicsDescription {
                    state: 0,
                    options: PhysicsOptions::default(),
                    sequences: bace_replication::physics_sequences(&sequences),
                },
                game: ObjectGameData {
                    name: "Bindstone".into(),
                    class_id: 27547,
                    icon_id: 0x0600_0001,
                    item_type: 16,
                    description_flags: 0,
                    options: ObjectGameOptions::default(),
                },
            }),
            vec![],
        )
        .unwrap();
    assert!(matches!(
        runtime
            .handle_binding_message(key, &use_message(object, 81))
            .unwrap(),
        ProgressionIngress::Accepted
    ));
    let context = runtime.recalls.bindings.pending[&key].context;
    let p = runtime.recalls.bindings.pending.get_mut(&key).unwrap();
    p.phase = BindingPhase::Submitted;
    p.prepared = Some(BindingPrepared {
        revision: 1,
        style: None,
        motion: output_motion(),
        seconds: 0.1,
    });
    runtime.recalls.event = Some(RecallEvent::BindingStarted {
        context,
        object,
        until_tick: 3,
    });
    runtime.project_recall_output().unwrap();
    let NetworkCommand::SendOrderedBatch {
        key: recipient,
        messages,
    } = runtime.network_output.pop_front().unwrap()
    else {
        panic!("bindstone action batch");
    };
    assert_eq!(recipient, key);
    assert_eq!(messages.len(), 1, "bindstone has no lifestone sound");
    assert_eq!(
        u32::from_le_bytes(messages[0].1[..4].try_into().unwrap()),
        0xf74c
    );
    assert_eq!(
        u32::from_le_bytes(messages[0].1[4..8].try_into().unwrap()),
        binding.actor.0
    );
    let stone_motion = runtime.pending_reward_observers().unwrap();
    assert_eq!(stone_motion.actor, object);
    assert_eq!(stone_motion.messages.len(), 1);
    assert_eq!(
        u32::from_le_bytes(stone_motion.messages[0].bytes[..4].try_into().unwrap()),
        0xf74c
    );
    assert_eq!(
        u32::from_le_bytes(stone_motion.messages[0].bytes[4..8].try_into().unwrap()),
        object.0
    );
    let stone_sequence = stone_motion.sequence;
    runtime
        .acknowledge_reward_observers(stone_sequence)
        .unwrap();
    let actor_motion = runtime.pending_reward_observers().unwrap();
    assert_eq!(actor_motion.actor, binding.actor);
    assert_eq!(actor_motion.messages.len(), 1);
    let actor_sequence = actor_motion.sequence;
    runtime
        .acknowledge_reward_observers(actor_sequence)
        .unwrap();

    let message: Arc<str> = Arc::from("authored bindstone UseMessage fixture");
    runtime.recalls.event = Some(RecallEvent::BindingStaged {
        context,
        object,
        operation: 65,
        allegiance: true,
        use_message: message.clone(),
        stamina_after: None,
    });
    runtime.project_recall_output().unwrap();
    assert!(
        runtime.network_output.is_empty(),
        "no success before durability"
    );
    runtime
        .record_binding_allegiance_completion(65, true)
        .unwrap();
    runtime.project_completed_bindings().unwrap();
    let NetworkCommand::SendOrderedBatch { messages, .. } =
        runtime.network_output.pop_front().unwrap()
    else {
        panic!("durable bindstone completion");
    };
    assert_eq!(
        messages.len(),
        1,
        "allegiance binding has no stamina mutation"
    );
    assert_eq!(
        messages[0].1,
        bace_wire::ChatMessage::System {
            text: &message,
            chat_type: 7,
        }
        .encode()
        .unwrap()
    );
    assert!(!runtime.recalls.bindings.pending.contains_key(&key));
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}

async fn seed_durable_monarch(fixture: &mut crate::game_runtime::tests::entered::EnteredFixture) {
    use bace_persistence::{AllegianceOperation, AllegianceWrite};
    use bace_simulation::{SocialControl, SocialControlAction};

    // Entry can finish while the shard's next allegiance chat-room seed still
    // has an outstanding exact control receipt. Let its runtime owner claim it
    // before the test sends an independent trusted restore request.
    crate::game_runtime::tests::entered::poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, _| !runtime.allegiance.has_pending() && runtime.social_controls.is_empty(),
    )
    .await;
    let actor = fixture.binding.actor;
    let account = fixture.runtime.sessions[&fixture.key].account.id;
    let lease = fixture.runtime.online_saves.baseline(actor.0).unwrap().2;
    let node = bace_allegiance::AllegianceNode {
        character: actor,
        account,
        name: "Entryprobe".into(),
        gender: 1,
        heritage: 1,
        patron: None,
        monarch: actor,
        vassals: vec![],
        rank: 1,
        followers: 0,
        level: 1,
        leadership: 0,
        loyalty: 0,
        sworn_at: 0,
        online_seconds: 1,
        may_pass_up: false,
        received_total: 0,
        tithed_total: 0,
        unclaimed: 0,
    };
    let metadata = bace_allegiance::AllegianceMetadata::new(actor, 0x8000_0001);
    let seed = AllegianceOperation {
        operation_id: format!("binding-monarch-seed-{}", actor.0),
        nodes: vec![AllegianceWrite {
            character: actor.0,
            mutation_revision: 1,
            expected_version: 0,
            bytes: Some(
                crate::social_saves::freeze_allegiance_node(&node)
                    .unwrap()
                    .encode()
                    .unwrap(),
            ),
        }],
        metadata: vec![AllegianceWrite {
            character: actor.0,
            mutation_revision: 1,
            expected_version: 0,
            bytes: Some(
                crate::social_saves::freeze_allegiance_metadata(&metadata)
                    .unwrap()
                    .encode()
                    .unwrap(),
            ),
        }],
        players: vec![],
        leases: vec![lease],
    };
    assert!(matches!(
        fixture
            .runtime
            .bootstrap
            .store
            .allegiance_operation(&seed)
            .await
            .unwrap(),
        bace_persistence::AllegianceCommit::Committed { .. }
    ));
    let stored_nodes = fixture
        .runtime
        .bootstrap
        .store
        .load_allegiance_nodes(&[actor.0])
        .await
        .unwrap();
    let stored_metadata = fixture
        .runtime
        .bootstrap
        .store
        .load_allegiance_metadata(&[actor.0])
        .await
        .unwrap();
    assert_eq!((stored_nodes.len(), stored_metadata.len()), (1, 1));
    fixture.runtime.allegiance.service = crate::allegiance_service::AllegianceService::new(
        fixture.runtime.bootstrap.world_owner.epoch(),
        &stored_nodes,
        &stored_metadata,
    )
    .unwrap();
    let registry =
        bace_allegiance::AllegianceRegistry::restore(vec![node], vec![metadata], 1, 32).unwrap();
    let sequence = u64::MAX - 200;
    fixture
        .runtime
        .simulation
        .input()
        .try_submit(bace_simulation::Command::SocialControl(SocialControl {
            sequence,
            action: SocialControlAction::RestoreAllegiances {
                registry: Box::new(registry),
                now_seconds: 0.,
            },
        }))
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Ok(outcome) = fixture
                .runtime
                .simulation
                .social_control_outcomes()
                .try_recv()
            {
                if outcome.sequence != sequence {
                    assert!(fixture.runtime.allegiance_owns_control(&outcome));
                    fixture.runtime.accept_allegiance_control(outcome).unwrap();
                    continue;
                }
                outcome.result.unwrap();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn approved_dat_relocated_bindstone_use_commits_copied_allegiance_sanctuary() {
    let (records, area, authored_cell) = relocated_bindstone_records();
    let mut fixture =
        Box::pin(crate::game_runtime::tests::entered::fixture_with_start_area(records, area)).await;
    seed_durable_monarch(&mut fixture).await;
    let block = (authored_cell >> 16) as u16;
    let stone_id = SOURCE_STONE;
    crate::game_runtime::tests::entered::poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, _| {
            runtime
                .world
                .as_ref()
                .and_then(|world| world.regions.prepared_region(block))
                .is_some()
                && runtime.recalls.bindings.objects.get(&stone_id) == Some(&BindingKind::Allegiance)
        },
    )
    .await;
    let region = fixture
        .runtime
        .world
        .as_ref()
        .unwrap()
        .regions
        .prepared_region(block)
        .unwrap()
        .clone();
    let stone = region
        .content
        .instances
        .iter()
        .find(|instance| instance.source.guid == stone_id.0)
        .unwrap();
    assert_eq!(stone.template.weenie_type, 65);
    let cell = stone.source.obj_cell_id;
    assert_eq!(
        cell, authored_cell,
        "private source relocation admitted exactly"
    );
    let pose = bace_geometry::Vec3::new(
        stone.source.origin_x,
        stone.source.origin_y,
        stone.source.origin_z,
    );
    let source_chat = bace_wire::ChatMessage::System {
        text: stone
            .template
            .properties
            .strings
            .iter()
            .find(|p| p.id == 18)
            .unwrap()
            .value
            .as_str(),
        chat_type: 7,
    }
    .encode()
    .unwrap();
    let actor = &fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .player
        .player
        .entity
        .state;
    let mut assets =
        crate::region_activation::VerifiedRegionAssets::open(&fixture.runtime.bootstrap.assets)
            .unwrap();
    let avatar = assets.prepare_avatar_dat(actor).unwrap();
    let mut accepted = None;
    let mut attempted = 0_u64;
    for offset in [
        [1., 0., 0.5],
        [-1., 0., 0.5],
        [0., 1., 0.5],
        [0., -1., 0.5],
        [1.2, 0., 0.5],
        [-1.2, 0., 0.5],
        [0., 1.2, 0.5],
        [0., -1.2, 0.5],
        [1.5, 0., 0.5],
        [2., 0., 0.5],
        [-1.5, 0., 0.5],
        [-2., 0., 0.5],
        [0., 1.5, 0.5],
        [0., -1.5, 0.5],
        [2.5, 0., 0.5],
        [-2.5, 0., 0.5],
    ] {
        let near = pose + bace_geometry::Vec3::new(offset[0], offset[1], offset[2]);
        if region
            .geometry
            .validate_placement(cell, near, &avatar.shape, &[], fixture.binding.actor.0)
            .is_err()
        {
            continue;
        }
        attempted += 1;
        fixture
            .runtime
            .simulation
            .input()
            .try_submit(bace_simulation::Command::ServerTeleport {
                actor: fixture.binding.actor,
                cell: bace_types::CellId(cell),
                position: near,
            })
            .unwrap();
        let correlation = u64::MAX - 300 - attempted;
        fixture
            .runtime
            .simulation
            .input()
            .try_submit(bace_simulation::Command::ObjectView(Box::new(
                bace_gameplay_api::visibility::ObjectViewRequest {
                    correlation,
                    binding: fixture.binding,
                    entities: vec![fixture.binding.actor, stone_id],
                },
            )))
            .unwrap();
        let view = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Ok(outcome) = fixture.runtime.simulation.object_view_outcomes().try_recv()
                    && outcome.correlation == correlation
                {
                    break outcome.result.clone().unwrap();
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        if view.views[0]
            .1
            .as_ref()
            .is_ok_and(|player| player.cell == cell && player.position == [near.x, near.y, near.z])
        {
            accepted = Some((near, view));
            break;
        }
    }
    let (near, view) = accepted.unwrap_or_else(|| {
        panic!("no source-valid live bindstone placement among {attempted} probes")
    });
    assert_eq!(
        view.views[0].1.as_ref().unwrap().position,
        [near.x, near.y, near.z]
    );
    let accepted_stone_pose = view.views[1].1.as_ref().unwrap().position;
    assert_eq!(view.views[1].1.as_ref().unwrap().cell, cell);
    assert_eq!(&accepted_stone_pose[..2], &[pose.x, pose.y]);
    assert!(
        (accepted_stone_pose[2] - pose.z).abs() < 0.01,
        "admission may settle the authored stone to the DAT floor"
    );
    let before = fixture
        .runtime
        .online_saves
        .baseline(fixture.binding.actor.0)
        .unwrap()
        .0
        .clone();
    let stamina_before = before
        .player
        .entity
        .state
        .properties
        .secondary_attributes
        .iter()
        .find(|p| p.id == 3)
        .unwrap()
        .value
        .current_level;
    let output_start = fixture.client.messages.len();
    let use_packet = GameActionEnvelope {
        sequence: 2,
        action: bace_wire::opcode::GameActionType::Use,
        payload: &stone_id.0.to_le_bytes(),
    }
    .encode(4096)
    .unwrap();
    fixture
        .client
        .send_message(&fixture.runtime, fixture.key, &use_packet);
    let started = std::time::Instant::now();
    let mut authored_seconds = None;
    let mut saw_running = false;
    let mut saw_staged = false;
    let completion = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let elapsed = fixture.runtime.clock.monotonic.elapsed();
            fixture.runtime.poll(elapsed).unwrap();
            fixture.client.drain(&fixture.runtime);
            if let Some(pending) = fixture.runtime.recalls.bindings.pending.get(&fixture.key) {
                if matches!(pending.phase, BindingPhase::Running) {
                    saw_running = true;
                    authored_seconds = pending.prepared.as_ref().map(|motion| motion.seconds);
                }
                saw_staged |= matches!(pending.phase, BindingPhase::Staged);
            }
            if fixture.runtime.recalls.bindings.pending.is_empty()
                && fixture.client.messages[output_start..]
                    .iter()
                    .any(|m| m.bytes == source_chat)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await;
    let phase = fixture
        .runtime
        .recalls
        .bindings
        .pending
        .get(&fixture.key)
        .map(|p| match &p.phase {
            BindingPhase::Capture => "Capture",
            BindingPhase::Capturing(_) => "Capturing",
            BindingPhase::Captured(..) => "Captured",
            BindingPhase::Preparing { .. } => "Preparing",
            BindingPhase::Ready => "Ready",
            BindingPhase::Submitted => "Submitted",
            BindingPhase::Running => "Running",
            BindingPhase::Staged => "Staged",
            BindingPhase::Failed(_) => "Failed",
        });
    assert!(
        completion.is_ok(),
        "bindstone Use did not finish: phase={phase:?}, failure={:?}, output opcodes={:?}",
        fixture.runtime.binding_failure(fixture.key),
        fixture.client.messages[output_start..]
            .iter()
            .filter_map(|m| m
                .bytes
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_le_bytes))
            .collect::<Vec<_>>()
    );
    assert!(
        saw_running && saw_staged,
        "authored action and durable stage"
    );
    assert!(
        authored_seconds.is_some_and(|seconds| {
            seconds > 0. && started.elapsed().as_secs_f64() + 0.1 >= seconds
        }),
        "source Sanctuary action timing"
    );
    let correlation = u64::MAX - 400;
    fixture
        .runtime
        .simulation
        .input()
        .try_submit(bace_simulation::Command::ObjectView(Box::new(
            bace_gameplay_api::visibility::ObjectViewRequest {
                correlation,
                binding: fixture.binding,
                entities: vec![fixture.binding.actor, stone_id],
            },
        )))
        .unwrap();
    let final_view = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Ok(outcome) = fixture.runtime.simulation.object_view_outcomes().try_recv()
                && outcome.correlation == correlation
            {
                break outcome.result.clone().unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let final_player = final_view.views[0].1.as_ref().unwrap();
    let final_stone = final_view.views[1].1.as_ref().unwrap();
    assert_eq!(final_player.cell, cell);
    assert_eq!(
        (final_stone.cell, final_stone.position),
        (cell, accepted_stone_pose),
        "binding never moves its accepted stone pose"
    );
    let stored = fixture
        .runtime
        .bootstrap
        .store
        .load_allegiance_metadata(&[fixture.binding.actor.0])
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    let durable = bace_storage_codec::AllegianceMetadataV1::decode(&stored[0].bytes).unwrap();
    let sanctuary = durable.sanctuary.unwrap();
    assert_eq!(
        (sanctuary.cell, sanctuary.origin),
        (cell, final_player.position)
    );
    assert_eq!(stored[0].persisted_version, 2);
    let stamina_after = fixture
        .runtime
        .online_saves
        .baseline(fixture.binding.actor.0)
        .unwrap()
        .0
        .player
        .entity
        .state
        .properties
        .secondary_attributes
        .iter()
        .find(|p| p.id == 3)
        .unwrap()
        .value
        .current_level;
    assert_eq!(stamina_after, stamina_before);
    let output = &fixture.client.messages[output_start..];
    let actor_motion = output
        .iter()
        .position(|m| {
            m.bytes.starts_with(&0xf74c_u32.to_le_bytes())
                && m.bytes.get(4..8) == Some(&fixture.binding.actor.0.to_le_bytes())
        })
        .expect("authored Sanctuary action for bound actor");
    let chat = output
        .iter()
        .position(|m| m.bytes == source_chat)
        .expect("authored bindstone UseMessage");
    assert!(actor_motion < chat, "source action precedes durable chat");
    assert!(
        !output
            .iter()
            .any(|m| m.bytes.starts_with(&0xf750_u32.to_le_bytes())),
        "bindstone has no lifestone sound"
    );
    assert!(
        !output.iter().any(|m| m.bytes.starts_with(
            &bace_wire::opcode::GameMessageOpcode::PrivateUpdateAttribute2ndLevel
                .0
                .to_le_bytes()
        )),
        "bindstone does not debit stamina"
    );
    fixture.shutdown().await;
}
