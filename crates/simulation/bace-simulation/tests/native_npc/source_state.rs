use super::*;
#[test]
fn live_source_property_and_my_quest_survive_checkpoint_without_reapplying() {
    let actions = vec![
        EmoteAction {
            r#type: 54,
            stat: Some(999),
            amount: Some(7),
            ..Default::default()
        },
        EmoteAction {
            r#type: 81,
            message: Some("flag".into()),
            ..Default::default()
        },
    ];
    let program = Arc::new(
        NativeProgram::prepare(vec![set(7, None, actions)], NativeLimits::default()).unwrap(),
    );
    let mut original = kernel(8);
    original
        .register_native_npc(EntityId(2), program.clone(), 3.0)
        .unwrap();
    original
        .start_npc_emote(
            EntityId(2),
            Some(EntityId(2)),
            NativeTrigger {
                category: 7,
                ..Default::default()
            },
            [7; 16],
            41,
            false,
        )
        .unwrap();
    original.step().unwrap();
    let property = original.take_npc_proposal().unwrap();
    let snapshot = original.preview_npc_owner_completion(&property).unwrap();
    assert_eq!(
        snapshot
            .properties
            .as_ref()
            .unwrap()
            .get(PropertyFamily::Int, 999),
        Some(&PropertyValue::Int(7))
    );
    assert_eq!(
        original
            .world()
            .properties(EntityId(2))
            .unwrap()
            .get(PropertyFamily::Int, 999),
        None
    );
    let mut restored = kernel(8);
    restored
        .register_native_npc(EntityId(2), program.clone(), 3.0)
        .unwrap();
    restored.restore_npc_source(snapshot).unwrap();
    restored
        .acknowledge_npc_recovery_ready(EntityId(2))
        .unwrap();
    restored.step().unwrap();
    let quest = restored.take_npc_proposal().unwrap();
    assert!(matches!(
        quest.effect,
        NpcEffect::Quest {
            actor: EntityId(2),
            aggregate: None,
            ..
        }
    ));
    let snapshot = restored.preview_npc_owner_completion(&quest).unwrap();
    assert_eq!(snapshot.source_quests.as_ref().unwrap().1[0].0, "FLAG");
    let expected = snapshot.source_quests.clone();
    let mut again = kernel(8);
    again
        .register_native_npc(EntityId(2), program, 3.0)
        .unwrap();
    again.restore_npc_source(snapshot).unwrap();
    again.acknowledge_npc_recovery_ready(EntityId(2)).unwrap();
    again.step().unwrap();
    assert!(again.take_npc_proposal().is_none());
    let final_state = again.checkpoint_npc_source(EntityId(2)).unwrap();
    assert_eq!(final_state.source_quests, expected);
    assert_eq!(
        final_state
            .properties
            .unwrap()
            .get(PropertyFamily::Int, 999),
        Some(&PropertyValue::Int(7))
    );
}

#[test]
fn recovery_registration_cannot_execute_before_snapshot_and_participants_are_ready() {
    let mut k = kernel(8);
    let outcome = k.dispatch_npc_service(bace_simulation::NpcServiceCommand {
        correlation: 1,
        action: bace_simulation::NpcServiceAction::RegisterRecovery {
            actor: EntityId(2),
            program: Arc::new(
                NativeProgram::prepare(vec![set(7, None, vec![])], NativeLimits::default())
                    .unwrap(),
            ),
            use_radius: 3.0,
            properties: None,
        },
    });
    assert!(outcome.result.is_ok());
    assert!(
        k.start_npc_emote(
            EntityId(2),
            Some(EntityId(1)),
            NativeTrigger {
                category: 7,
                ..Default::default()
            },
            [2; 16],
            42,
            false
        )
        .is_err()
    );
    assert!(k.discard_idle_npc_sources().is_err());
}
