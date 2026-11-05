use super::*;

/// Pinned ACE Healer.HandleActionUseOnTarget/DoHealMotion/DoHealing: a wounded
/// self-target uses SkillHealSelf, then spends one Structure after motion and
/// publishes UseDone only after the player/item durable placement.
#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn self_healer_use_waits_for_motion_and_durable_charge() {
    let mut fixture = Box::pin(fixture_with_healer()).await;
    let actor = fixture.binding.actor.0;
    let source = fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .inventory
        .iter()
        .find(|row| row.entity.state.weenie_id == 12748)
        .expect("private starter healer");
    let kit = source.entity.object_id;
    assert!(fixture.client.has_create(kit, 0));
    assert_eq!(
        source
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 92)
            .unwrap()
            .value,
        3
    );
    let start = fixture.client.messages.len();
    let mut payload = Vec::new();
    payload.extend_from_slice(&kit.to_le_bytes());
    payload.extend_from_slice(&actor.to_le_bytes());
    let request = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::UseWithTarget,
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
            runtime.healing_kit_failure(key).is_some()
                || client
                    .messages
                    .iter()
                    .skip(start)
                    .any(|m| m.queue == 9 && m.bytes.get(12..16) == Some(&0x01c7u32.to_le_bytes()))
        },
    )
    .await;
    assert_eq!(fixture.runtime.healing_kit_failure(key), None);
    let events = &fixture.client.messages[start..];
    let motion = events
        .iter()
        .position(|m| m.bytes.get(..4) == Some(&0xf74cu32.to_le_bytes()))
        .expect("SkillHealSelf motion");
    let use_done = events
        .iter()
        .position(|m| m.queue == 9 && m.bytes.get(12..16) == Some(&0x01c7u32.to_le_bytes()))
        .expect("committed UseDone");
    assert!(motion < use_done, "motion precedes durable result");
    assert_eq!(
        events[use_done].bytes.get(16..20),
        Some(&0u32.to_le_bytes()[..])
    );
    let stored = fixture
        .runtime
        .bootstrap
        .store
        .load(kit)
        .await
        .unwrap()
        .unwrap();
    let spent = bace_storage_codec::ItemSaveV5::decode_or_migrate(&stored.bytes, None).unwrap();
    assert_eq!(
        spent
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 92)
            .unwrap()
            .value,
        2
    );
    assert_eq!(
        spent
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 19)
            .unwrap()
            .value,
        60
    );
    assert!(
        events
            .iter()
            .any(|m| m.bytes.get(5..9) == Some(&kit.to_le_bytes())
                && m.bytes.get(9..13) == Some(&92u32.to_le_bytes()))
    );
    assert!(!fixture.runtime.healing_kits.has_pending());
    fixture.shutdown().await;
}
