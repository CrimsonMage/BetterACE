use super::*;
use bace_content::{Property, WeenieV1};
fn item(kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "skill_device".into(),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    }
}
#[test]
fn preparation_selects_only_source_device_types_and_preserves_four_requirements() {
    let mut w = item(62);
    w.properties.ints = vec![
        Property { id: 185, value: 2 },
        Property { id: 186, value: 18 },
    ];
    assert_eq!(
        preparation::device(&w).unwrap(),
        Some(PreparedSkillDevice::Lower(18))
    );
    w.weenie_type = 1;
    assert_eq!(preparation::device(&w).unwrap(), None);
    for (kind, skill, difficulty, raw) in [
        (158, 159, 160, 1),
        (270, 271, 272, 4),
        (273, 274, 275, 8),
        (276, 277, 278, 12),
    ] {
        w.properties.ints.extend([
            Property { id: kind, value: 8 },
            Property {
                id: skill,
                value: raw,
            },
            Property {
                id: difficulty,
                value: 3,
            },
        ]);
    }
    let requirements = preparation::requirements(&w).unwrap();
    for (r, id) in requirements.into_iter().zip([1, 4, 8, 12]) {
        assert_eq!(
            r,
            Some(bace_character::SkillWieldRequirement::Training {
                skill: id,
                advancement: bace_gameplay_api::SkillAdvancement::Specialized
            })
        );
    }
}
#[test]
fn stale_capture_error_is_owned_and_unrelated_capture_is_retained() {
    let mut devices = SkillDeviceRuntime::new();
    let key = SessionKey {
        id: 1,
        generation: 1,
    };
    let context = ActionContext {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
        sequence: 1,
    };
    devices.pending = Some(Pending {
        key,
        context,
        item: EntityId(2),
        identity: SkillOperationId::new([1; 16]).unwrap(),
        phase: Phase::Capturing(3),
        quote: None,
        ticket: None,
        proposal_seen: false,
        request_retry: None,
        use_action: true,
    });
    let error = |correlation| PlayerSnapshotOutcome {
        correlation,
        result: Err(bace_simulation::CharacterRegistrationError::DurabilityPending),
    };
    assert!(devices.accept_capture(error(4), 0).is_err());
    assert!(matches!(
        devices.pending.as_ref().unwrap().phase,
        Phase::Capturing(3)
    ));
    assert!(devices.accept_capture(error(3), 0).is_ok());
    assert!(matches!(
        devices.pending.as_ref().unwrap().phase,
        Phase::Capture
    ));
}

#[test]
fn authoritative_confirmation_must_match_prepared_device_and_action() {
    let mut runtime = SkillDeviceRuntime::new();
    let context = ActionContext {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
        sequence: 9,
    };
    let quote = Quote {
        quote: bace_simulation::SkillDeviceConfirmation {
            token: 0,
            actor: context.actor,
            item: EntityId(2),
            expires: 0,
            device: PreparedSkillDevice::Lower(18),
        },
        prompt: "source".into(),
        name: "Gem".into(),
        activation_talk: None,
    };
    runtime.pending = Some(Pending {
        key: SessionKey {
            id: 1,
            generation: 1,
        },
        context,
        item: EntityId(2),
        identity: SkillOperationId::new([1; 16]).unwrap(),
        phase: Phase::Submitted,
        quote: Some(quote.clone()),
        ticket: None,
        proposal_seen: false,
        request_retry: None,
        use_action: true,
    });
    let mut actual = quote.quote;
    actual.token = 1;
    actual.expires = 1800;
    let outcome = |q| SkillDeviceOutcome {
        context: Some(context),
        result: Ok(SkillDeviceResult::Confirmation(q)),
    };
    assert!(
        runtime
            .accept(outcome(bace_simulation::SkillDeviceConfirmation {
                device: PreparedSkillDevice::Specialize(18),
                ..actual
            }))
            .is_err()
    );
    assert!(matches!(
        runtime.pending.as_ref().unwrap().phase,
        Phase::Submitted
    ));
    assert!(runtime.accept(outcome(actual)).is_ok());
    assert!(matches!(
        runtime.pending.as_ref().unwrap().phase,
        Phase::Prompt(_)
    ));
    assert!(runtime.accept(outcome(actual)).is_err());
}

#[test]
fn authored_activation_effects_are_not_silently_ignored() {
    let mut w = item(62);
    w.properties.ints = vec![
        Property { id: 185, value: 1 },
        Property { id: 186, value: 18 },
    ];
    assert!(preparation::device(&w).is_ok());
    w.properties.ints.push(Property { id: 369, value: 50 });
    assert!(preparation::device(&w).is_ok());
    assert_eq!(
        crate::player_assets::prepare_item_activation_requirements(&w)
            .unwrap()
            .level,
        Some(50)
    );
    w.properties.ints.pop();
    for property in [
        Property {
            id: 83,
            value: 0x1002,
        },
        Property { id: 119, value: 0 },
    ] {
        w.properties.ints.push(property);
        assert!(preparation::device(&w).is_err());
        w.properties.ints.pop();
    }
    w.properties.ints.push(Property { id: 280, value: 7 });
    assert!(
        preparation::device(&w).is_err(),
        "group needs authored duration"
    );
    w.properties.floats.push(Property {
        id: 167,
        value: 30.0,
    });
    assert!(preparation::device(&w).is_ok());
    assert_eq!(preparation::cooldown_seconds(&w).unwrap(), Some(30.0));
    w.properties.floats[0].value = 86_401.0;
    assert!(preparation::device(&w).is_err());
}

#[test]
fn ace_use_then_talk_response_is_prepared_and_ordered_before_use_done() {
    // Pinned ACE WorldObject_Use.OnActivate: ActOnUse, OnTalk, then the
    // caller's UseDone. ActivationResponse.Use|Talk is 0x2|0x10; string 17
    // is ActivationTalk and Broadcast chat type is zero.
    let mut w = item(62);
    w.properties.ints = vec![
        Property { id: 185, value: 1 },
        Property { id: 186, value: 18 },
        Property {
            id: 83,
            value: 0x12,
        },
    ];
    assert!(
        preparation::device(&w).is_err(),
        "missing talk must hold Use"
    );
    w.properties.strings.push(Property {
        id: 17,
        value: "The gem hums.".into(),
    });
    assert_eq!(
        preparation::device(&w).unwrap(),
        Some(PreparedSkillDevice::Specialize(18))
    );
    let talk = activation_response::talk(&w, 4096).unwrap();
    assert_eq!(talk.as_deref(), Some("The gem hums."));
    let quote = Quote {
        quote: bace_simulation::SkillDeviceConfirmation {
            token: 1,
            actor: EntityId(1),
            item: EntityId(2),
            expires: 1800,
            device: PreparedSkillDevice::Specialize(18),
        },
        prompt: "Specialize?".into(),
        name: "Gem".into(),
        activation_talk: talk,
    };
    let mut steps = output::prompt_steps(&quote).unwrap();
    steps.push(bace_replication::InventoryProjection::Simple(
        bace_wire::SimpleGameEvent::UseDone(0),
    ));
    let binding = bace_gameplay_api::CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
    };
    let batch = bace_replication::EventSequencer::new(binding, 1)
        .project_inventory(
            binding,
            &steps,
            &mut BTreeMap::new(),
            output::objects(4096),
            output::limits(4096),
        )
        .unwrap();
    assert_eq!(batch.messages.len(), 3);
    let opcode =
        |index: usize| u32::from_le_bytes(batch.messages[index].bytes[..4].try_into().unwrap());
    assert_eq!(opcode(0), bace_wire::opcode::GameMessageOpcode::GameEvent.0);
    assert_eq!(
        opcode(1),
        bace_wire::opcode::GameMessageOpcode::ServerMessage.0
    );
    assert_eq!(opcode(2), bace_wire::opcode::GameMessageOpcode::GameEvent.0);
    let text = b"The gem hums.";
    assert!(
        batch.messages[1]
            .bytes
            .windows(text.len())
            .any(|bytes| bytes == text)
    );
    w.properties.ints[2].value = 0x1002;
    assert!(
        preparation::device(&w).is_err(),
        "spell side effect stays held"
    );
}

#[test]
fn inactive_source_skips_all_response_preparation_and_only_projects_use_done() {
    let mut w = item(62);
    w.properties.ints = vec![
        Property { id: 119, value: 0 },
        Property {
            id: 83,
            value: 0x1002,
        },
    ];
    assert!(activation_response::inactive(&w));
    // Even an otherwise unsupported spell response is never executed when
    // Active is false; the generic OnActivate returns before checking it.
    assert!(preparation::device(&w).is_err());
    let binding = bace_gameplay_api::CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
    };
    let batch = bace_replication::EventSequencer::new(binding, 1)
        .project_inventory(
            binding,
            &[bace_replication::InventoryProjection::Simple(
                bace_wire::SimpleGameEvent::UseDone(0),
            )],
            &mut BTreeMap::new(),
            output::objects(4096),
            output::limits(4096),
        )
        .unwrap();
    assert_eq!(batch.messages.len(), 1);
    let envelope = bace_wire::GameEventEnvelope::decode(&batch.messages[0].bytes, 4096).unwrap();
    assert_eq!(envelope.event, bace_wire::opcode::GameEventType::UseDone);
}
