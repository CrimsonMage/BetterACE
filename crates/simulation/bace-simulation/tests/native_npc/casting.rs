use super::*;
use bace_gameplay_api::{CastChange, CastOrigin, ServerCastOutcome};
#[test]
fn cast_row_requires_exact_terminal_magic_outcome() {
    let mut k = magic_common::kernel(32);
    k.configure_npc_services(
        Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
        1000,
        vec![],
        bace_world_events::Events::prepare(vec![], false).unwrap(),
    )
    .unwrap();
    k.register_magic_spell(magic_common::spell(
        100,
        bace_magic::SpellEffect::Boost {
            vital: bace_magic::Vital::Health,
            minimum: 3,
            maximum: 3,
        },
    ))
    .unwrap();
    k.register_native_npc(
        EntityId(2),
        Arc::new(
            NativeProgram::prepare(
                vec![set(
                    7,
                    None,
                    vec![EmoteAction {
                        r#type: 19,
                        spell_id: Some(100),
                        ..Default::default()
                    }],
                )],
                NativeLimits::default(),
            )
            .unwrap(),
        ),
        10.0,
    )
    .unwrap();
    k.start_npc_emote(
        EntityId(2),
        Some(EntityId(1)),
        NativeTrigger {
            category: 7,
            ..Default::default()
        },
        [8; 16],
        1,
        true,
    )
    .unwrap();
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let before = k
        .world()
        .vital(EntityId(1), bace_entity::EntityVital::Health)
        .unwrap()
        .current;
    k.begin_npc_cast(&proposal).unwrap();
    let fake = ServerCastOutcome {
        origin: CastOrigin::Emote {
            actor: EntityId(2),
            event: proposal.ticket,
            instant: true,
        },
        result: Ok(CastChange::Completed { cast: 99 }),
    };
    assert!(k.adopt_npc_cast_completion(&proposal, &fake).is_err());
    assert!(
        k.complete_npc_service(
            &proposal,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 }
        )
        .is_err()
    );
    let mut completed = None;
    for _ in 0..10 {
        magic_common::step(&mut k);
        assert!(
            k.take_server_cast_outcome().is_none(),
            "generic adapter cannot steal an NPC-owned outcome"
        );
        if let Some(outcome) = k.poll_npc_cast(&proposal).unwrap() {
            completed = Some(outcome);
            break;
        }
    }
    let completed = completed.expect("actual magic completion");
    assert!(matches!(completed.result, Ok(CastChange::Completed { .. })));
    assert_eq!(
        k.poll_npc_cast(&proposal).unwrap(),
        Some(completed.clone()),
        "undelivered completion retained"
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), bace_entity::EntityVital::Health)
            .unwrap()
            .current,
        (before + 3).min(100)
    );
    k.adopt_npc_cast_completion(&proposal, &completed).unwrap();
    assert!(k.adopt_npc_cast_completion(&proposal, &completed).is_err());
}
