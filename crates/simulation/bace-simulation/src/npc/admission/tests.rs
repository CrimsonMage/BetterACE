use super::*;

fn staged() -> Npcs {
    let mut owner = Npcs::new(8);
    let actor = EntityId(0x8000_0001);
    owner.sources.insert(
        actor,
        Source {
            admission_hold: None,
            admission_bound: false,
            admission: Some(NpcScriptIdentity {
                template: 10,
                program_hash: [2; 32],
                content_generation: [3; 32],
            }),
            manager: NativeEmoteManager::new(Arc::new(
                NativeProgram::prepare(vec![], bace_emotes::NativeLimits::default()).unwrap(),
            )),
            random: None,
            use_radius: 1.,
            clock_offset: 0.,
            event_id: [0; 16],
            key_version: 0,
            active_operation: 0,
            invocations: BTreeMap::new(),
            recovery_ready: false,
            journal_hold: None,
            held_logical_now: None,
        },
    );
    owner.source_order.push(actor);
    owner.quests.insert(actor, QuestRegistry::new(8).unwrap());
    owner
}

#[test]
fn staged_script_rolls_back_but_ready_script_remains_owned() {
    let actor = EntityId(0x8000_0001);
    let mut owner = staged();
    owner.sources.get_mut(&actor).unwrap().recovery_ready = true;
    assert_eq!(owner.rollback_admitted(actor), Err(NpcFailure::Conflict));
    assert!(owner.sources.contains_key(&actor));
    owner.sources.get_mut(&actor).unwrap().recovery_ready = false;
    owner.rollback_admitted(actor).unwrap();
    assert!(!owner.sources.contains_key(&actor));
    assert!(!owner.source_order.contains(&actor));
    assert!(!owner.quests.contains_key(&actor));
}
