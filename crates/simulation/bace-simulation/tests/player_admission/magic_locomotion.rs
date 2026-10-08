//! Authored geometry/animation tests for the authenticated locomotion lane during
//! casting. These exercise accepted physics; raw positions are never supplied.
use super::*;
use bace_gameplay_api::locomotion::*;
fn casting_kernel() -> (Kernel, CharacterBinding) {
    let (mut kernel, mut input) = fixture();
    let mut traits: Vec<_> = input
        .state
        .character
        .progression
        .trait_states()
        .map(|(progress, details)| TraitState {
            progress,
            details: details.unwrap(),
        })
        .collect();
    for id in [22, 24] {
        traits.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(id),
                experience_spent: 0,
                advancement: SkillAdvancement::Trained,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.,
            },
        });
        input.skills.inputs.push((id, input.skills.inputs[0].1));
    }
    for state in &mut traits {
        if state.progress.target == ProgressionTarget::Skill(33) {
            state.details = TraitDetails::Skill {
                initial_level: 900,
                resistance_at_last_check: 0,
                last_used_time: 0.,
            };
        }
    }
    let rank = RankTable::new(&[0, 10, 100]).unwrap();
    input.state.character.progression = CharacterProgression::with_state(
        &traits,
        Arc::new(ProgressionTables {
            attributes: rank.clone(),
            vitals: rank.clone(),
            trained_skills: rank.clone(),
            specialized_skills: rank,
        }),
        0,
        4,
    )
    .unwrap();
    input.state.character.ui.as_mut().unwrap().known_spells = vec![100];
    input.caster.known_spells.insert(100);
    input.caster.components_required = false;
    let mut locomotion = (*fixture_locomotion()).clone();
    locomotion.profile.style = 0x80000049;
    locomotion.profile.walk.velocity = Vec3::new(0., 3., 0.);
    locomotion.profile.run.velocity = Vec3::new(0., 6., 0.);
    let locomotion = Arc::new(locomotion);
    input.locomotion = locomotion.clone();
    input.locomotion_styles = vec![locomotion.clone()];
    input
        .actor
        .body
        .submit_animated_locomotion(0, 0, locomotion, Default::default(), 1.)
        .unwrap();
    let binding = input.binding;
    kernel
        .admit_player(input)
        .unwrap_or_else(|(e, _)| panic!("{e:?}"));
    kernel.character_entered(binding).unwrap();
    kernel
        .configure_magic_random(bace_random::RandomRoot::new([7; 32], 1).unwrap())
        .unwrap();
    kernel
        .register_magic_spell(bace_simulation::PreparedMagicSpell {
            spell: bace_magic::PreparedSpell {
                id: 100,
                school: bace_magic::MagicSchool::Life,
                power: 1,
                base_mana: 10,
                range_constant: 30.,
                range_per_skill: 0.,
                harmful: false,
                resistable: false,
                effect: bace_magic::SpellEffect::Boost {
                    vital: bace_magic::Vital::Health,
                    minimum: 10,
                    maximum: 10,
                },
            },
            gestures: vec![bace_simulation::PreparedCastGesture {
                gesture: bace_magic::CastGesture {
                    motion: 0x4000002b,
                    minimum_seconds: 0.1,
                },
                duration_seconds: 0.,
                motion_chain: Some(cast_chain()),
            }],
            components: vec![],
            component_modifiers: vec![],
            component_loss: 0.,
            fast_resistable_pk_spell: false,
        })
        .unwrap();
    (kernel, binding)
}
fn cast_chain() -> Arc<bace_motion::PreparedMotionChain> {
    use bace_motion::*;
    let ready = SourceMotionState {
        style: 0x80000049,
        substate: 0x41000003,
        speed: 1.,
    };
    let clip = |frames| ExecutionClip {
        animation: 0x03000001,
        frame_count: frames,
        low: 0,
        high: frames as i32 - 1,
        framerate: 30.,
        frames: vec![RootFrame::default(); frames as usize].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![clip(2)], 0, 0, physics, ready.substate, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: true,
            })
            .unwrap(),
    );
    Arc::new(
        PreparedMotionChain::prepare(vec![clip(90), clip(2)], 1, 1, physics, 0x4000002b, 2.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(stop)
            .unwrap(),
    )
}
fn context(binding: CharacterBinding, sequence: u32) -> ActionContext {
    ActionContext {
        actor: binding.actor,
        account: binding.account,
        session: binding.session,
        sequence,
    }
}
fn begin(kernel: &mut Kernel, binding: CharacterBinding) {
    kernel
        .enqueue(Command::Cast {
            context: context(binding, 1),
            request: CastRequest::Targeted {
                target: binding.actor,
                spell: 100,
            },
        })
        .unwrap();
    kernel.step().unwrap();
    assert!(matches!(
        kernel.take_cast_outcome().unwrap().result,
        Ok(CastChange::Started { .. })
    ));
    while kernel.take_magic_event().is_some() {}
}
#[test]
fn animated_state_during_cast_preserves_slide_and_displacement_fizzles_once() {
    let (mut kernel, binding) = casting_kernel();
    begin(&mut kernel, binding);
    let before = kernel
        .world()
        .body(binding.actor)
        .unwrap()
        .accepted()
        .position();
    kernel
        .enqueue(Command::Locomotion(LocomotionCommand {
            correlation: 2,
            context: context(binding, 2),
            teleport: 0,
            request: LocomotionRequest::State(RawLocomotionState {
                style: 0x80000049,
                current_hold: 2,
                forward: (0x45000005, 0, 1.),
                sidestep: (0, 0, 1.),
                turn: (0, 0, 1.),
            }),
        }))
        .unwrap();
    kernel.step().unwrap();
    assert!(kernel.take_locomotion_outcome().unwrap().result.is_ok());
    for _ in 0..35 {
        kernel.step().unwrap();
        while kernel.take_magic_event().is_some() {}
    }
    let after = kernel
        .world()
        .body(binding.actor)
        .unwrap()
        .accepted()
        .position();
    assert!(
        after.y > before.y + 1.,
        "accepted animated slide must not be lost behind zero cast root: {before:?} -> {after:?}"
    );
    assert_eq!(
        kernel
            .world()
            .vital(binding.actor, EntityVital::Mana)
            .unwrap()
            .current,
        50
    );
    for _ in 0..80 {
        kernel.step().unwrap();
        while kernel.take_magic_event().is_some() {}
    }
    let terminal = kernel.take_cast_outcome().unwrap();
    assert!(
        matches!(terminal.result, Ok(CastChange::Completed { .. })),
        "GDLE movement fizzle ends successfully without applying the spell: {terminal:?}"
    );
    assert_eq!(
        kernel
            .world()
            .vital(binding.actor, EntityVital::Mana)
            .unwrap()
            .current,
        45
    );
}
#[test]
fn authenticated_jump_during_cast_debits_stamina_once_and_cancel_preserves_mana() {
    let (mut kernel, binding) = casting_kernel();
    begin(&mut kernel, binding);
    kernel
        .enqueue(Command::Locomotion(LocomotionCommand {
            correlation: 2,
            context: context(binding, 2),
            teleport: 0,
            request: LocomotionRequest::Jump { extent: 1. },
        }))
        .unwrap();
    kernel.step().unwrap();
    let jumped = kernel.take_locomotion_outcome().unwrap();
    assert!(jumped.result.as_ref().unwrap().stamina.is_some());
    let stamina = kernel
        .world()
        .vital(binding.actor, EntityVital::Stamina)
        .unwrap()
        .current;
    assert!(stamina < 50);
    kernel
        .enqueue(Command::Cast {
            context: context(binding, 3),
            request: CastRequest::Cancel,
        })
        .unwrap();
    for _ in 0..100 {
        kernel.step().unwrap();
        while kernel.take_magic_event().is_some() {}
    }
    assert!(matches!(
        kernel.take_cast_outcome().unwrap().result,
        Ok(CastChange::Cancelled { .. })
    ));
    assert_eq!(
        kernel
            .world()
            .vital(binding.actor, EntityVital::Mana)
            .unwrap()
            .current,
        50
    );
    assert_eq!(
        kernel
            .world()
            .vital(binding.actor, EntityVital::Stamina)
            .unwrap()
            .current,
        stamina
    );
}
