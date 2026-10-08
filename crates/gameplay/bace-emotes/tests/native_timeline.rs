use bace_content::{Emote, EmoteAction};
use bace_emotes::*;
use bace_gameplay_api::*;
use bace_types::EntityId;
use std::{collections::BTreeMap, sync::Arc};
struct Host {
    post: f64,
    hold: bool,
}
impl NativeEmoteHost for Host {
    fn facts(&self, _: EntityId) -> Option<NpcActorFacts> {
        Some(NpcActorFacts {
            player: true,
            creature: true,
        })
    }
    fn draw(&mut self) -> Result<f64, NpcFailure> {
        Ok(0.0)
    }
    fn query(&mut self, _: NpcContext, _: NpcQuery) -> Result<NpcQueryValue, NpcFailure> {
        Err(NpcFailure::Unsupported)
    }
    fn text(&mut self, _: NpcContext, text: &str, _: Option<&str>) -> Result<String, NpcFailure> {
        Ok(text.into())
    }
    fn execute(&mut self, _: NpcContext, op: NpcOperation) -> Result<NpcCompletion, NpcFailure> {
        if self.hold {
            return Ok(NpcCompletion::Pending { ticket: 9 });
        }
        if matches!(op, NpcOperation::Give { .. }) {
            return Ok(NpcCompletion::Detached {
                ticket: 9,
                post_delay: 0.0,
            });
        }
        Ok(NpcCompletion::Applied {
            post_delay: if matches!(
                op,
                NpcOperation::Text {
                    kind: NpcTextKind::Say,
                    ..
                }
            ) {
                0.0
            } else {
                self.post
            },
        })
    }
}
fn program(delay: f32) -> Arc<NativeProgram> {
    let action = |code, pre, msg: &str| EmoteAction {
        r#type: code,
        delay: pre,
        message: Some(msg.into()),
        ..Default::default()
    };
    Arc::new(
        NativeProgram::prepare(
            vec![
                Emote {
                    category: 7,
                    probability: 1.0,
                    actions: vec![
                        action(1, delay, "act"),
                        action(67, delay, "nested"),
                        action(10, delay, "tell"),
                    ],
                    ..Default::default()
                },
                Emote {
                    category: 32,
                    quest: Some("nested".into()),
                    probability: 1.0,
                    actions: vec![action(8, 0.25, "say")],
                    ..Default::default()
                },
            ],
            NativeLimits::default(),
        )
        .unwrap(),
    )
}
#[test]
fn pinned_unchanged_enqueue_methods_match_actual_execution_delays_and_nested_order() {
    let mut groups: BTreeMap<String, Vec<(u32, f64)>> = BTreeMap::new();
    for line in include_str!("fixtures/native_timeline.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        if p[1] == "busy" {
            assert_eq!(&p[2..], ["False", "0"]);
            continue;
        }
        groups
            .entry(p[0].into())
            .or_default()
            .push((p[1].parse().unwrap(), p[2].parse().unwrap()));
    }
    for (name, expected) in groups {
        let p: Vec<_> = name.split(':').collect();
        let delay: f32 = p[1].parse().unwrap();
        let mut host = Host {
            post: p[2].parse().unwrap(),
            hold: false,
        };
        let mut manager = NativeEmoteManager::new(program(delay));
        manager
            .trigger(
                &NativeTrigger {
                    category: 7,
                    ..Default::default()
                },
                NpcContext {
                    source: EntityId(1),
                    target: Some(EntityId(2)),
                    operation: 1,
                },
                0.0,
                &mut host,
            )
            .unwrap();
        let mut actual = Vec::new();
        let mut now = 0.0;
        loop {
            let state = manager.checkpoint();
            let due = state.work.iter().map(|w| w.due).min_by(f64::total_cmp);
            if due != Some(0.0) {
                break;
            }
            if let NativeStep::Executed { action } = manager.step(0.0, &mut host).unwrap() {
                actual.push((action, 0.0));
            }
        }
        if p[3] == "True" {
            now = 3.0;
        }
        for _ in 0..50 {
            if !manager.busy() {
                break;
            }
            let next = manager
                .checkpoint()
                .work
                .iter()
                .map(|w| w.due)
                .min_by(f64::total_cmp)
                .unwrap();
            now = f64::max(now, next);
            if let NativeStep::Executed { action } = manager.step(now, &mut host).unwrap() {
                actual.push((action, now));
            }
        }
        assert_eq!(actual, expected, "{name}");
        assert!(!manager.busy());
    }
}
#[test]
fn checkpoint_keeps_pending_stage_and_detached_owner_ticket_distinct() {
    let program = program(0.25);
    let mut vm = NativeEmoteManager::new(program.clone());
    let mut host = Host {
        post: 0.5,
        hold: true,
    };
    vm.trigger(
        &NativeTrigger {
            category: 7,
            ..Default::default()
        },
        NpcContext {
            source: EntityId(1),
            target: None,
            operation: 7,
        },
        8.0,
        &mut host,
    )
    .unwrap();
    assert_eq!(
        vm.step(8.25, &mut host).unwrap(),
        NativeStep::Pending { ticket: 9 }
    );
    assert!(vm.checkpoint().work.is_empty());
    let snapshot = vm.checkpoint();
    let restored = NativeEmoteManager::restore(program.clone(), snapshot.clone()).unwrap();
    assert_eq!(restored.checkpoint(), snapshot);
    let mut bad = snapshot;
    bad.pending[0].row.due = f64::NAN;
    assert!(NativeEmoteManager::restore(program.clone(), bad).is_err());
    host.hold = false;
    vm.detach_pending(9, 0.0, 8.25, &mut host).unwrap();
    let state = vm.checkpoint();
    assert_eq!(state.detached, vec![9]);
    assert!(state.pending.is_empty());
    assert_eq!(state.work[0].due, 8.5);
    let mut restored = NativeEmoteManager::restore(program, state).unwrap();
    assert!(restored.complete_detached(10).is_err());
    restored.complete_detached(9).unwrap();
    assert!(restored.busy());
}

#[test]
fn immediate_nested_calls_and_detached_chain_match_original_busy_order() {
    let expected = |name: &str| {
        include_str!("fixtures/native_immediate.csv")
            .lines()
            .filter(|l| l.starts_with(name))
            .map(|l| l.split(',').nth(1).unwrap().parse::<u32>().unwrap())
            .collect::<Vec<_>>()
    };
    let action = |code, msg: &str| EmoteAction {
        r#type: code,
        message: Some(msg.into()),
        weenie_class_id: Some(1),
        ..Default::default()
    };
    let set = |category, key: Option<&str>, actions| Emote {
        category,
        quest: key.map(str::to_owned),
        probability: 1.0,
        actions,
        ..Default::default()
    };
    let program = Arc::new(
        NativeProgram::prepare(
            vec![
                set(7, None, vec![action(67, "nested"), action(1, "parent")]),
                set(
                    32,
                    Some("nested"),
                    vec![action(8, "child1"), action(10, "child2")],
                ),
            ],
            NativeLimits::default(),
        )
        .unwrap(),
    );
    let mut vm = NativeEmoteManager::new(program);
    let mut host = Host {
        post: 0.0,
        hold: false,
    };
    let context = NpcContext {
        source: EntityId(1),
        target: Some(EntityId(2)),
        operation: 1,
    };
    vm.trigger(
        &NativeTrigger {
            category: 7,
            ..Default::default()
        },
        context,
        0.0,
        &mut host,
    )
    .unwrap();
    let mut trace = Vec::new();
    while vm.busy() {
        if let NativeStep::Executed { action } = vm.step(0.0, &mut host).unwrap() {
            trace.push(action);
        }
    }
    assert_eq!(trace, expected("nested_zero"));
    let program = Arc::new(
        NativeProgram::prepare(
            vec![
                set(7, None, vec![action(3, "give"), action(10, "after")]),
                set(4, None, vec![action(1, "next")]),
            ],
            NativeLimits::default(),
        )
        .unwrap(),
    );
    let mut vm = NativeEmoteManager::new(program);
    vm.trigger(
        &NativeTrigger {
            category: 7,
            ..Default::default()
        },
        context,
        0.0,
        &mut host,
    )
    .unwrap();
    assert_eq!(
        vm.step(0.0, &mut host).unwrap(),
        NativeStep::Pending { ticket: 9 }
    );
    let mut trace = vec![3];
    if let NativeStep::Executed { action } = vm.step(0.0, &mut host).unwrap() {
        trace.push(action);
    }
    assert!(!vm.busy());
    assert!(vm.has_detached());
    vm.trigger(
        &NativeTrigger {
            category: 4,
            ..Default::default()
        },
        NpcContext {
            operation: 2,
            ..context
        },
        0.0,
        &mut host,
    )
    .unwrap();
    if let NativeStep::Executed { action } = vm.step(0.0, &mut host).unwrap() {
        trace.push(action);
    }
    vm.complete_detached(9).unwrap();
    trace.push(103);
    assert_eq!(trace, expected("detached_zero"));
}
