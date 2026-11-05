use super::*;

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn immediate_wand_requip_after_dequip_keeps_exact_inventory_owner() {
    // Pinned ACE Player_Inventory.HandleActionGetAndWieldItem (1524) and
    // HandleActionPutItemInContainer (976) permit this ordered sequence without
    // asking the client to replay a valid final wield after a server-side save.
    let mut fixture = Box::pin(fixture()).await;
    let key = fixture.key;
    let actor = fixture.binding.actor.0;
    let wand = fixture.runtime.sessions[&key]
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
    let mut first_avatar = None;
    for (action, payload, expected_event, expected_slot) in [
        (
            bace_wire::opcode::GameActionType::GetAndWieldItem,
            [wand.to_le_bytes(), 0x0100_0000u32.to_le_bytes()].concat(),
            0x23u32,
            0x0100_0000u32,
        ),
        (
            bace_wire::opcode::GameActionType::PutItemInContainer,
            [wand.to_le_bytes(), actor.to_le_bytes(), 0i32.to_le_bytes()].concat(),
            0x22u32,
            0,
        ),
        (
            bace_wire::opcode::GameActionType::GetAndWieldItem,
            [wand.to_le_bytes(), 0x0100_0000u32.to_le_bytes()].concat(),
            0x23u32,
            0x0100_0000u32,
        ),
    ] {
        let start = fixture.client.messages.len();
        let started = std::time::Instant::now();
        let request = bace_wire::GameActionEnvelope {
            sequence: 4,
            action,
            payload: &payload,
        }
        .encode(1024)
        .unwrap();
        fixture
            .client
            .send_message_on_queue(&fixture.runtime, key, 3, &request);
        poll_until(
            &mut fixture.runtime,
            &mut fixture.client,
            |runtime, client| {
                runtime.inventory_failure(key).is_some()
                    || client.messages.iter().skip(start).any(|message| {
                        message.bytes.get(12..16) == Some(&expected_event.to_le_bytes())
                            && message.bytes.get(16..20) == Some(&wand.to_le_bytes())
                    })
            },
        )
        .await;
        assert_eq!(
            fixture.runtime.inventory_failure(key),
            None,
            "{action:?} failed after {:?}",
            started.elapsed()
        );
        eprintln!("{action:?} committed in {:?}", started.elapsed());
        assert!(matches!(
            fixture
                .runtime
                .online_saves
                .inventory_baseline(actor, wand)
                .expect("wand baseline after committed action")
                .placement,
            bace_storage_codec::ItemPlacementV2::Contained {
                container,
                equipped,
                ..
            } if container == actor && equipped == expected_slot
        ));
        let avatar = fixture
            .runtime
            .inventory
            .cached_avatar_dat()
            .expect("completed equipment action caches its immutable DAT closure");
        if let Some(first) = &first_avatar {
            assert!(Arc::ptr_eq(first, avatar), "same avatar DAT closure reused");
        } else {
            first_avatar = Some(avatar.clone());
        }
    }
    fixture.shutdown().await;
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn carried_item_moves_to_bag_and_reorders_with_durable_containment() {
    let mut fixture = Box::pin(fixture()).await;
    let actor = fixture.binding.actor.0;
    let loaded = &fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded;
    let bag = loaded
        .inventory
        .iter()
        .find(|row| {
            matches!(row.placement, bace_storage_codec::ItemPlacementV2::Contained {
                container, pack_slot: true, equipped: 0, ..
            } if container == actor)
        })
        .expect("starter bag")
        .entity
        .object_id;
    let item = loaded
        .inventory
        .iter()
        .find(|row| {
            matches!(row.placement, bace_storage_codec::ItemPlacementV2::Contained {
                container, pack_slot: false, equipped: 0, ..
            } if container == actor)
                && !row.entity.state.properties.ints.iter().any(|p| p.id == 6)
        })
        .expect("loose starter item")
        .entity
        .object_id;
    assert!(fixture.client.has_create(item, 0));
    assert!(fixture.client.has_create(bag, 0));
    let store = fixture.runtime.bootstrap.store.clone();
    let mut player_version = None;

    // ACE parses but does not act on the GameAction counter. All three fresh
    // ordered transport messages may carry the same payload counter.
    for (sequence, destination, placement) in [(1, bag, 0), (1, actor, 0), (1, actor, 1)] {
        let start = fixture.client.messages.len();
        let started = std::time::Instant::now();
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&item.to_le_bytes());
        payload.extend_from_slice(&destination.to_le_bytes());
        payload.extend_from_slice(&(placement as i32).to_le_bytes());
        let request = bace_wire::GameActionEnvelope {
            sequence,
            action: bace_wire::opcode::GameActionType::PutItemInContainer,
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
                        message.queue == 9
                            && message.bytes.get(12..16) == Some(&0x22u32.to_le_bytes())
                            && message.bytes.get(16..20) == Some(&item.to_le_bytes())
                    })
            },
        )
        .await;
        assert_eq!(
            fixture.runtime.inventory_failure(key),
            None,
            "move {sequence}"
        );
        eprintln!(
            "carried item move to {destination:#010x} slot {placement} committed in {:?}",
            started.elapsed()
        );
        let output = &fixture.client.messages;
        let (event_index, event) = output
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, message)| {
                message.queue == 9
                    && message.bytes.get(12..16) == Some(&0x22u32.to_le_bytes())
                    && message.bytes.get(16..20) == Some(&item.to_le_bytes())
            })
            .expect("committed containment event");
        let (property_index, property) = output
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, message)| {
                message.queue == 9
                    && message.bytes.get(..4) == Some(&0x02dau32.to_le_bytes())
                    && message.bytes.get(5..9) == Some(&item.to_le_bytes())
                    && message.bytes.get(9..13) == Some(&2u32.to_le_bytes())
            })
            .expect("ACE container property update");
        assert!(property_index < event_index);
        assert_eq!(
            property.bytes.get(13..17),
            Some(&destination.to_le_bytes()[..])
        );
        assert_eq!(
            event.bytes.get(20..24),
            Some(&destination.to_le_bytes()[..])
        );
        assert_eq!(
            event.bytes.get(24..28),
            Some(&(placement as i32).to_le_bytes()[..])
        );
        let stored = store.load(item).await.unwrap().unwrap();
        let saved = bace_storage_codec::ItemSaveV5::decode_or_migrate(&stored.bytes, None).unwrap();
        assert!(matches!(
            saved.placement,
            bace_storage_codec::ItemPlacementV2::Contained {
                container,
                slot,
                pack_slot: false,
                equipped: 0,
            } if container == destination && slot == placement
        ));
        let current_player_version = store.load(actor).await.unwrap().unwrap().persisted_version;
        if let Some(previous) = player_version {
            assert_eq!(
                current_player_version, previous,
                "item-only move rewrote player"
            );
        }
        player_version = Some(current_player_version);
    }
    fixture.shutdown().await;
}
