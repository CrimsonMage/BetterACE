use super::*;

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn identify_and_wield_cross_real_udp() {
    let mut fixture = Box::pin(fixture()).await;
    let actor = fixture.binding.actor.0;
    let start = fixture.client.messages.len();
    let request = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::IdentifyObject,
        payload: &actor.to_le_bytes(),
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, fixture.key, 3, &request);
    poll_until(&mut fixture.runtime, &mut fixture.client, |_, client| {
        client.messages.iter().skip(start).any(|message| {
            message.queue == 9
                && message.bytes.get(..4) == Some(&0xf7b0u32.to_le_bytes())
                && message.bytes.get(12..16) == Some(&0xc9u32.to_le_bytes())
                && message.bytes.get(16..20) == Some(&actor.to_le_bytes())
        })
    })
    .await;
    let response = fixture
        .client
        .messages
        .iter()
        .skip(start)
        .find(|message| {
            message.queue == 9 && message.bytes.get(12..16) == Some(&0xc9u32.to_le_bytes())
        })
        .unwrap();
    assert_eq!(response.bytes.get(4..8), Some(&actor.to_le_bytes()[..]));
    assert_eq!(response.bytes.get(24..28), Some(&1u32.to_le_bytes()[..]));
    let mut stamps: Vec<_> = fixture
        .client
        .messages
        .iter()
        .filter(|message| {
            message.queue == 9
                && message.bytes.get(..4) == Some(&0xf7b0u32.to_le_bytes())
                && message.bytes.get(4..8) == Some(&actor.to_le_bytes())
        })
        .map(|message| u32::from_le_bytes(message.bytes[8..12].try_into().unwrap()))
        .collect();
    stamps.sort_unstable();
    assert_eq!(
        stamps,
        (1..=6).collect::<Vec<_>>(),
        "no missing UI event stamp before appraisal"
    );

    let item = fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .inventory
        .first()
        .expect("created character has starter inventory")
        .entity
        .object_id;
    assert!(
        fixture.client.has_create(item, 0),
        "appraised item must be resident on the client"
    );
    let start = fixture.client.messages.len();
    let request = bace_wire::GameActionEnvelope {
        sequence: 2,
        action: bace_wire::opcode::GameActionType::IdentifyObject,
        payload: &item.to_le_bytes(),
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, fixture.key, 3, &request);
    poll_until(&mut fixture.runtime, &mut fixture.client, |_, client| {
        client.messages.iter().skip(start).any(|message| {
            message.queue == 9
                && message.bytes.get(12..16) == Some(&0xc9u32.to_le_bytes())
                && message.bytes.get(16..20) == Some(&item.to_le_bytes())
        })
    })
    .await;
    let response = fixture
        .client
        .messages
        .iter()
        .skip(start)
        .find(|message| {
            message.queue == 9 && message.bytes.get(16..20) == Some(&item.to_le_bytes())
        })
        .unwrap();
    assert_eq!(response.bytes.get(24..28), Some(&1u32.to_le_bytes()[..]));
    let mut stamps: Vec<_> = fixture
        .client
        .messages
        .iter()
        .filter(|message| {
            message.queue == 9
                && message.bytes.get(..4) == Some(&0xf7b0u32.to_le_bytes())
                && message.bytes.get(4..8) == Some(&actor.to_le_bytes())
        })
        .map(|message| u32::from_le_bytes(message.bytes[8..12].try_into().unwrap()))
        .collect();
    stamps.sort_unstable();
    assert_eq!(stamps, (1..=7).collect::<Vec<_>>());
    let start = fixture.client.messages.len();
    let request = bace_wire::GameActionEnvelope {
        sequence: 3,
        action: bace_wire::opcode::GameActionType::QueryHealth,
        payload: &actor.to_le_bytes(),
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, fixture.key, 3, &request);
    poll_until(&mut fixture.runtime, &mut fixture.client, |_, client| {
        client.messages.iter().skip(start).any(|message| {
            message.queue == 9 && message.bytes.get(12..16) == Some(&0x01c0u32.to_le_bytes())
        })
    })
    .await;
    assert!(!fixture.runtime.staff_ingress_blocked(fixture.key));
    let weapon = fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .inventory
        .iter()
        .find(|item| item.entity.state.weenie_id == 12748)
        .expect("ACE starter wand")
        .entity
        .object_id;
    assert!(fixture.client.has_create(weapon, 0));
    let nearby_before: Vec<_> = fixture
        .client
        .messages
        .iter()
        .filter(|message| {
            message.queue == 10
                && message.bytes.get(..4)
                    == Some(
                        &bace_wire::opcode::GameMessageOpcode::ObjectCreate
                            .0
                            .to_le_bytes(),
                    )
        })
        .filter_map(|message| message.bytes.get(4..8))
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .filter(|id| {
            *id != actor
                && *id != weapon
                && fixture
                    .runtime
                    .visibility
                    .service
                    .knows(fixture.key, EntityId(*id))
        })
        .collect();
    assert!(
        !nearby_before.is_empty(),
        "fixture needs a known area object"
    );
    let start = fixture.client.messages.len();
    let mut payload = Vec::new();
    payload.extend_from_slice(&weapon.to_le_bytes());
    payload.extend_from_slice(&0x0100_0000u32.to_le_bytes());
    let request = bace_wire::GameActionEnvelope {
        sequence: 4,
        action: bace_wire::opcode::GameActionType::GetAndWieldItem,
        payload: &payload,
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, fixture.key, 3, &request);
    let key = fixture.key;
    poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, client| {
            runtime.inventory_failure(key).is_some()
                || client
                    .messages
                    .iter()
                    .skip(start)
                    .any(|message| message.bytes.get(12..16) == Some(&0x0023u32.to_le_bytes()))
        },
    )
    .await;
    assert_eq!(fixture.runtime.inventory_failure(key), None);
    let response = fixture
        .client
        .messages
        .iter()
        .skip(start)
        .find(|message| message.bytes.get(12..16) == Some(&0x0023u32.to_le_bytes()))
        .unwrap();
    assert_eq!(response.bytes.get(16..20), Some(&weapon.to_le_bytes()[..]));
    assert!(matches!(
        fixture
            .runtime
            .online_saves
            .inventory_baseline(actor, weapon)
            .expect("committed wand equip baseline")
            .placement,
        bace_storage_codec::ItemPlacementV2::Contained {
            container,
            equipped: 0x0100_0000,
            ..
        } if container == actor
    ));
    // Equipment updates may replace the actor's attachment, but an ordinary
    // wield must not reset and delete every unrelated object in the area.
    let settle_started = std::time::Instant::now();
    poll_until(&mut fixture.runtime, &mut fixture.client, |runtime, _| {
        runtime.rewards.observers.is_empty()
            && !runtime.observer_output.has_pending()
            && settle_started.elapsed() >= Duration::from_millis(250)
    })
    .await;
    for object in nearby_before {
        assert!(
            !fixture.client.has_delete(object, start),
            "equipping reset unrelated known object {object:#010x}"
        );
    }
    let start = fixture.client.messages.len();
    let mut payload = Vec::with_capacity(12);
    payload.extend_from_slice(&weapon.to_le_bytes());
    payload.extend_from_slice(&actor.to_le_bytes());
    payload.extend_from_slice(&0i32.to_le_bytes());
    // ACE reads the payload counter but does not validate it. A fresh reliable
    // message with this repeated counter must still be processed in order.
    let dequip = bace_wire::GameActionEnvelope {
        sequence: 4,
        action: bace_wire::opcode::GameActionType::PutItemInContainer,
        payload: &payload,
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, fixture.key, 3, &dequip);
    poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, client| {
            runtime.inventory_failure(key).is_some()
                || client.messages.iter().skip(start).any(|message| {
                    message.bytes.get(12..16) == Some(&0x0022u32.to_le_bytes())
                        && message.bytes.get(16..20) == Some(&weapon.to_le_bytes())
                })
        },
    )
    .await;
    assert_eq!(fixture.runtime.inventory_failure(key), None);
    assert!(fixture.client.messages.iter().skip(start).any(|message| {
        message.bytes.get(12..16) == Some(&0x0022u32.to_le_bytes())
            && message.bytes.get(16..20) == Some(&weapon.to_le_bytes())
    }));
    assert!(matches!(
        fixture
            .runtime
            .online_saves
            .inventory_baseline(actor, weapon)
            .expect("committed wand dequip baseline")
            .placement,
        bace_storage_codec::ItemPlacementV2::Contained {
            container,
            equipped: 0,
            ..
        } if container == actor
    ));
    fixture.shutdown().await;
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn training_spadone_wields_across_real_udp_and_durable_inventory() {
    let mut fixture = Box::pin(fixture_with_twohanded()).await;
    let weapon = fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .inventory
        .iter()
        .find(|item| item.entity.state.weenie_id == 41512)
        .expect("trained two-handed character has ACE starter Training Spadone")
        .entity
        .object_id;
    assert!(fixture.client.has_create(weapon, 0));
    let start = fixture.client.messages.len();
    let mut payload = Vec::with_capacity(8);
    payload.extend_from_slice(&weapon.to_le_bytes());
    payload.extend_from_slice(&0x0200_0000u32.to_le_bytes());
    let request = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::GetAndWieldItem,
        payload: &payload,
    }
    .encode(1024)
    .unwrap();
    fixture
        .client
        .send_message_on_queue(&fixture.runtime, fixture.key, 3, &request);
    let key = fixture.key;
    poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, client| {
            runtime.inventory_failure(key).is_some()
                || client.messages.iter().skip(start).any(|message| {
                    message.bytes.get(12..16) == Some(&0x0023u32.to_le_bytes())
                        && message.bytes.get(16..20) == Some(&weapon.to_le_bytes())
                })
        },
    )
    .await;
    assert_eq!(fixture.runtime.inventory_failure(key), None);
    let response = fixture
        .client
        .messages
        .iter()
        .skip(start)
        .find(|message| {
            message.bytes.get(12..16) == Some(&0x0023u32.to_le_bytes())
                && message.bytes.get(16..20) == Some(&weapon.to_le_bytes())
        })
        .expect("committed Training Spadone wield event");
    assert_eq!(
        response.bytes.get(20..24),
        Some(&0x0200_0000u32.to_le_bytes()[..])
    );
    let committed = fixture
        .runtime
        .online_saves
        .inventory_baseline(fixture.binding.actor.0, weapon)
        .expect("committed starter weapon baseline");
    assert!(matches!(
        committed.placement,
        bace_storage_codec::ItemPlacementV2::Contained {
            container,
            equipped: 0x0200_0000,
            ..
        } if container == fixture.binding.actor.0
    ));
    fixture.shutdown().await;
}
