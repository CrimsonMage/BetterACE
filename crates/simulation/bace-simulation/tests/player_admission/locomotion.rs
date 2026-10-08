use super::*;
use bace_gameplay_api::locomotion::*;
fn admitted() -> (Kernel, CharacterBinding) {
    admitted_scaled(1.0)
}
fn admitted_scaled(scale: f64) -> (Kernel, CharacterBinding) {
    let (mut kernel, mut input) = fixture();
    let mut source = (*input.physical_source).clone();
    let mut raw = (*source.weenie).clone();
    raw.properties.floats.push(bace_content::Property {
        id: 39,
        value: scale,
    });
    source.weenie = Arc::new(raw);
    input.physical_source = Arc::new(source);
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
    let mut profile = (*fixture_locomotion()).clone();
    profile.profile.walk.velocity = Vec3::new(0., 2., 0.);
    profile.profile.run.velocity = Vec3::new(0., 4., 0.);
    profile.profile.sidestep.velocity = Vec3::new(1., 0., 0.);
    profile.profile.turn.omega = Vec3::new(0., 0., 1.);
    let profile = Arc::new(profile);
    input.locomotion = profile.clone();
    input.locomotion_styles = vec![profile.clone()];
    input
        .actor
        .body
        .submit_animated_locomotion(
            0,
            0,
            profile,
            bace_motion::LocomotionControls::default(),
            1.,
        )
        .unwrap();
    let binding = input.binding;
    kernel
        .admit_player(input)
        .unwrap_or_else(|(error, _)| panic!("{error:?}"));
    kernel.character_entered(binding).unwrap();
    (kernel, binding)
}
fn command(
    binding: CharacterBinding,
    sequence: u32,
    request: LocomotionRequest,
) -> LocomotionCommand {
    LocomotionCommand {
        correlation: u64::from(sequence),
        context: ActionContext {
            session: binding.session,
            account: binding.account,
            actor: binding.actor,
            sequence,
        },
        teleport: 0,
        request,
    }
}
fn state() -> RawLocomotionState {
    RawLocomotionState {
        style: 0x8000003d,
        current_hold: 2,
        forward: (0x45000005, 0, 1.),
        sidestep: (0, 0, 1.),
        turn: (0, 0, 1.),
    }
}
#[test]
fn accepted_raw_controls_move_through_collision_and_observations_never_teleport() {
    let (mut kernel, binding) = admitted();
    let result = kernel.apply_locomotion(command(binding, 1, LocomotionRequest::State(state())));
    assert!(result.result.is_ok(), "{result:?}");
    let initial = kernel
        .world()
        .body(binding.actor)
        .unwrap()
        .accepted()
        .position();
    for _ in 0..30 {
        kernel.step().unwrap();
    }
    let accepted = kernel
        .world()
        .body(binding.actor)
        .unwrap()
        .accepted()
        .position();
    assert!(accepted.y > initial.y + 5.);
    assert_eq!(accepted.x, initial.x);
    let output = kernel
        .apply_locomotion(command(binding, 2, LocomotionRequest::ObservePosition))
        .result
        .unwrap();
    assert_eq!(output.view.position, [accepted.x, accepted.y, accepted.z]);
    let mut invalid = state();
    invalid.forward.2 = f32::INFINITY;
    assert!(
        kernel
            .apply_locomotion(command(binding, 3, LocomotionRequest::State(invalid)))
            .result
            .is_err()
    );
    assert_eq!(
        kernel
            .world()
            .body(binding.actor)
            .unwrap()
            .accepted()
            .position(),
        accepted
    );
    for _ in 0..180 {
        kernel.step().unwrap();
    }
    assert!(
        kernel
            .world()
            .body(binding.actor)
            .unwrap()
            .accepted()
            .position()
            .y
            < 20.0
    );
}
#[test]
fn jump_debits_live_stamina_once_and_rejects_airborne_replays_and_epoch_mismatch() {
    let (mut kernel, binding) = admitted();
    kernel.step().unwrap();
    assert!(
        kernel
            .world()
            .body(binding.actor)
            .unwrap()
            .accepted()
            .grounded()
    );
    let before = kernel
        .world()
        .vital(binding.actor, EntityVital::Stamina)
        .unwrap()
        .current;
    let jump_command = command(binding, 1, LocomotionRequest::Jump { extent: 1. });
    let jump = kernel.apply_locomotion(jump_command);
    assert_eq!(jump.result.unwrap().stamina, Some((before, before - 6)));
    assert!(kernel.apply_locomotion(jump_command).result.is_err());
    assert_eq!(
        kernel
            .world()
            .vital(binding.actor, EntityVital::Stamina)
            .unwrap()
            .current,
        before - 6
    );
    kernel.step().unwrap();
    let position = kernel
        .world()
        .body(binding.actor)
        .unwrap()
        .accepted()
        .position();
    assert!(position.z > 0.);
    assert!(
        kernel
            .apply_locomotion(command(binding, 2, LocomotionRequest::Jump { extent: 1. }))
            .result
            .is_err()
    );
    assert_eq!(
        kernel
            .world()
            .vital(binding.actor, EntityVital::Stamina)
            .unwrap()
            .current,
        before - 6
    );
    let mut stale = command(binding, 3, LocomotionRequest::State(state()));
    stale.teleport = 1;
    assert_eq!(
        kernel.apply_locomotion(stale).result,
        Err(LocomotionRejection::Stale)
    );
}

#[test]
fn player_jump_uses_pinned_unit_scale_while_run_uses_visual_scale() {
    let mut vertical = Vec::new();
    let mut rates = Vec::new();
    for scale in [1.0, 2.0] {
        let (mut kernel, binding) = admitted_scaled(scale);
        kernel.step().unwrap();
        kernel
            .apply_locomotion(command(binding, 1, LocomotionRequest::State(state())))
            .result
            .unwrap();
        rates.push(
            kernel
                .world()
                .body(binding.actor)
                .unwrap()
                .locomotion_run_rate()
                .unwrap(),
        );
        let result = kernel
            .apply_locomotion(command(binding, 2, LocomotionRequest::Jump { extent: 1. }))
            .result
            .unwrap();
        assert_eq!(result.stamina, Some((50, 44)));
        kernel.step().unwrap();
        vertical.push(
            kernel
                .world()
                .body(binding.actor)
                .unwrap()
                .accepted()
                .velocity()
                .z,
        );
    }
    assert_eq!(vertical[0], vertical[1]);
    assert_eq!(rates[0], rates[1] * 2.0);
}
