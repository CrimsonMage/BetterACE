use super::*;

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn shortcut_action_survives_options_routine_save_and_reentry() {
    let mut fixture = Box::pin(fixture()).await;
    let actor = fixture.binding.actor.0;
    let key = fixture.key;
    let store = fixture.runtime.bootstrap.store.clone();
    let item = fixture.runtime.sessions[&key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .inventory
        .first()
        .expect("starter item")
        .entity
        .object_id;
    assert!(fixture.client.has_create(item, 0));
    let original_options = fixture.runtime.sessions[&key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .player
        .player
        .metadata
        .options1;
    let slot = 2u32;
    let mut shortcut = Vec::with_capacity(12);
    shortcut.extend_from_slice(&slot.to_le_bytes());
    shortcut.extend_from_slice(&item.to_le_bytes());
    shortcut.extend_from_slice(&0u32.to_le_bytes()); // spell ID and layer
    let add = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::AddShortCut,
        payload: &shortcut,
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, key, 3, &add);

    // Let the five-second routine lane write the character without a logout.
    poll_until(&mut fixture.runtime, &mut fixture.client, |runtime, _| {
        runtime
            .online_saves
            .baseline(actor)
            .is_some_and(|(saved, _, _)| {
                saved.ui.shortcuts.iter().any(|s| {
                    s.index == slot && s.object_id == item && s.spell_id == 0 && s.layer == 0
                })
            })
    })
    .await;
    let first = store.load(actor).await.unwrap().unwrap();
    let first_saved = bace_storage_codec::PlayerSaveV6::decode(&first.bytes).unwrap();
    assert_eq!(first_saved.ui.shortcuts.len(), 1);
    assert_eq!(first_saved.ui.shortcuts[0].object_id, item);

    // ACE consumes the shortcut table in SetCharacterOptions but leaves the
    // AddShortCut-owned table intact. Send an empty table and change options1
    // so the action must be applied and saved again.
    let changed_options = original_options ^ 2;
    let mut options = Vec::with_capacity(16);
    options.extend_from_slice(&1u32.to_le_bytes()); // shortcut section present
    options.extend_from_slice(&changed_options.to_le_bytes());
    options.extend_from_slice(&0u32.to_le_bytes()); // zero shortcuts
    options.extend_from_slice(&0u32.to_le_bytes()); // empty first spell bar
    let set = bace_wire::GameActionEnvelope {
        sequence: 2,
        action: bace_wire::opcode::GameActionType::SetCharacterOptions,
        payload: &options,
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, key, 3, &set);
    poll_until(&mut fixture.runtime, &mut fixture.client, |runtime, _| {
        runtime
            .online_saves
            .baseline(actor)
            .is_some_and(|(saved, _, _)| saved.player.metadata.options1 == changed_options)
    })
    .await;
    assert_eq!(fixture.runtime.progression_failure(key), None);
    let second = store.load(actor).await.unwrap().unwrap();
    assert!(second.persisted_version > first.persisted_version);
    let second_saved = bace_storage_codec::PlayerSaveV6::decode(&second.bytes).unwrap();
    assert_eq!(second_saved.ui.shortcuts, first_saved.ui.shortcuts);

    let mut logoff = bace_wire::Writer::new();
    logoff.u32(bace_wire::opcode::GameMessageOpcode::CharacterLogOff.0);
    fixture
        .client
        .send_message(&fixture.runtime, key, &logoff.into_bytes());
    poll_until(&mut fixture.runtime, &mut fixture.client, |runtime, _| {
        !runtime.sessions.contains_key(&key)
    })
    .await;

    let mut reconnect = client::Client::new(&fixture.runtime, "Account");
    poll_until(&mut fixture.runtime, &mut reconnect, |runtime, _| {
        runtime.sessions.values().any(|s| s.connected)
    })
    .await;
    let new_key = *fixture.runtime.sessions.keys().next().unwrap();
    assert_ne!(key.generation, new_key.generation);
    reconnect.bind(new_key);
    poll_until(&mut fixture.runtime, &mut reconnect, |runtime, _| {
        runtime.login_job.is_none() && runtime.login_queue.is_empty()
    })
    .await;
    reconnect.send_message_on_queue(
        &fixture.runtime,
        new_key,
        5,
        &current_ddd_response(&fixture.runtime),
    );
    poll_until(&mut fixture.runtime, &mut reconnect, |runtime, _| {
        runtime.dat.as_ref().unwrap().ready_for_world(new_key)
    })
    .await;
    reconnect.send_message(
        &fixture.runtime,
        new_key,
        &bace_wire::opcode::GameMessageOpcode::CharacterEnterWorldRequest
            .0
            .to_le_bytes(),
    );
    poll_until(&mut fixture.runtime, &mut reconnect, |_, client| {
        client.messages.iter().any(|message| {
            message.queue == 9
                && message.bytes
                    == bace_wire::opcode::GameMessageOpcode::CharacterEnterWorldServerReady
                        .0
                        .to_le_bytes()
        })
    })
    .await;
    let entry_start = reconnect.messages.len();
    let account_name = fixture.runtime.sessions[&new_key]
        .account
        .name
        .as_str()
        .to_owned();
    let mut enter = bace_wire::Writer::new();
    enter.u32(bace_wire::opcode::GameMessageOpcode::CharacterEnterWorld.0);
    enter.u32(actor);
    enter.string16(&account_name).unwrap();
    reconnect.send_message(&fixture.runtime, new_key, &enter.into_bytes());
    poll_until(&mut fixture.runtime, &mut reconnect, |runtime, client| {
        runtime.players.entered(EntityId(actor)) && client.has_create(actor, entry_start)
    })
    .await;
    let loaded = &fixture.runtime.sessions[&new_key]
        .loading
        .as_ref()
        .unwrap()
        .loaded;
    assert_eq!(loaded.player.ui.shortcuts, first_saved.ui.shortcuts);
    let description = reconnect
        .messages
        .iter()
        .skip(entry_start)
        .find(|message| {
            message.queue == 9
                && message.bytes.get(..4)
                    == Some(
                        &bace_wire::opcode::GameMessageOpcode::GameEvent
                            .0
                            .to_le_bytes(),
                    )
                && message.bytes.get(12..16)
                    == Some(
                        &bace_wire::opcode::GameEventType::PlayerDescription
                            .0
                            .to_le_bytes(),
                    )
        })
        .expect("reentry PlayerDescription");
    assert!(
        description
            .bytes
            .windows(shortcut.len())
            .any(|bytes| bytes == shortcut),
        "reentry PlayerDescription carries the saved shortcut"
    );

    let mut logoff = bace_wire::Writer::new();
    logoff.u32(bace_wire::opcode::GameMessageOpcode::CharacterLogOff.0);
    reconnect.send_message(&fixture.runtime, new_key, &logoff.into_bytes());
    poll_until(&mut fixture.runtime, &mut reconnect, |runtime, _| {
        !runtime.sessions.contains_key(&new_key)
    })
    .await;
    drop(reconnect);
    fixture.shutdown().await;
}
