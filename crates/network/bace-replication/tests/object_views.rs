use bace_gameplay_api::visibility::*;
use bace_replication::{
    BatchLimits, ObjectProjection, SequenceKind as K, Sequences, physics_sequences,
    project_accepted_server_motion, project_server_motion,
};
use bace_types::EntityId;
use bace_wire::*;
fn limits() -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 128,
        max_motion_commands: 64,
        max_string_bytes: 128,
    }
}
fn source() -> ObjectDescription {
    ObjectDescription {
        object_id: 2,
        model: ObjectModel::default(),
        physics: PhysicsDescription {
            state: 0x404410,
            options: PhysicsOptions::default(),
            sequences: physics_sequences(&Sequences::new(10).unwrap()),
        },
        game: ObjectGameData {
            name: "fixture".into(),
            class_id: 1,
            icon_id: 0x06000001,
            item_type: 16,
            description_flags: 0,
            options: ObjectGameOptions::default(),
        },
    }
}
fn view() -> AcceptedObjectView {
    AcceptedObjectView {
        entity: EntityId(2),
        cell: 0x10100001,
        position: [1., 2., 3.],
        velocity: [0.; 3],
        heading_radians: 0.,
        grounded: true,
        epoch: 0,
        held: false,
        motion: Ok(Some(AcceptedObjectMotion {
            autonomous: false,
            style: 0x8000003c,
            forward_motion: 0x41000003,
            forward_rate: 1.,
            sidestep_rate: 0.,
            turn_rate: 0.,
            actions: vec![AcceptedMotionAction {
                domain: AcceptedMotionDomain::Physical,
                owner: 10,
                sequence: 1,
                motion: 0x40000068,
                speed: 1.5,
            }],
            goal: None,
        })),
    }
}
#[test]
fn frozen_accepted_action_uses_the_canonical_server_motion_counter() {
    let accepted = view();
    let movement = project_accepted_server_motion(&accepted).unwrap();
    let MotionBody::State { state, .. } = &movement.body else {
        panic!("accepted action became a different movement body");
    };
    assert_eq!(state.commands[0].raw_command, 0x68);
    assert_eq!(state.commands[0].speed, 1.5);
    let mut sequences = Sequences::new(10).unwrap();
    let packet = project_server_motion(
        accepted.entity.0,
        &movement,
        &mut sequences,
        BatchLimits {
            max_messages: 1,
            max_bytes: 4096,
            max_message_bytes: 4096,
            max_string_bytes: 128,
        },
    )
    .unwrap();
    assert_eq!(&packet.bytes[4..8], &accepted.entity.0.to_le_bytes());
    assert_eq!(sequences.current(K::Motion, 0), 2);
    let mut unsupported = accepted;
    unsupported
        .motion
        .as_mut()
        .unwrap()
        .as_mut()
        .unwrap()
        .autonomous = true;
    assert!(project_accepted_server_motion(&unsupported).is_err());
}
#[test]
fn source_action_counter_is_assigned_once_and_failed_projection_is_atomic() {
    let mut counters = Sequences::new(10).unwrap();
    let source = source();
    let mut bad = limits();
    bad.max_message_bytes = 1;
    assert!(ObjectProjection::prepare(&source, view(), None, &mut counters, bad).is_err());
    assert_eq!(counters.current(K::Motion, 0), 1);
    let first = ObjectProjection::prepare(&source, view(), None, &mut counters, limits()).unwrap();
    assert_eq!(counters.current(K::Motion, 0), 2);
    let mut next = view();
    next.position[0] = 5.;
    next.velocity = [3., 0., 0.];
    let second =
        ObjectProjection::prepare(&source, next, Some(&first), &mut counters, limits()).unwrap();
    assert_eq!(counters.current(K::Motion, 0), 2);
    assert_eq!(counters.current(K::ObjectPosition, 0), 1);
    let updates = second.refresh_messages(limits()).unwrap();
    let position = PositionUpdate::decode(&updates[1].bytes).unwrap();
    assert_eq!(position.pack.position.origin, [5., 2., 3.]);
    assert_eq!(position.pack.velocity, Some([3., 0., 0.]));
    assert_eq!(position.pack.position_sequence, 1);
    let Some(PhysicsMovement::Motion(motion)) = &second.description.physics.options.movement else {
        panic!()
    };
    let MotionBody::State { state, .. } = &motion.body else {
        panic!()
    };
    assert_eq!(state.commands[0].sequence, 2);
    assert_eq!(state.commands[0].speed, 1.5);
    assert_eq!(
        first.create,
        ObjectProjection::prepare(
            &source,
            view(),
            Some(&first),
            &mut Sequences::new(10).unwrap(),
            limits()
        )
        .unwrap()
        .create
    );
}
#[test]
fn held_body_is_frozen_without_faking_idle_or_erasing_velocity() {
    let mut counters = Sequences::new(10).unwrap();
    let first =
        ObjectProjection::prepare(&source(), view(), None, &mut counters, limits()).unwrap();
    let mut held = view();
    held.held = true;
    held.velocity = [1., 2., 3.];
    let result =
        ObjectProjection::prepare(&source(), held, Some(&first), &mut counters, limits()).unwrap();
    assert_ne!(result.description.physics.state & 0x1000000, 0);
    assert_eq!(
        result.description.physics.options.velocity,
        Some([1., 2., 3.])
    );
    assert_eq!(
        result.description.physics.options.movement,
        first.description.physics.options.movement
    );
    assert_eq!(counters.current(K::ObjectState, 0), 1);
}
#[test]
fn accepted_goal_and_teleport_are_projected_from_their_owner() {
    let mut value = view();
    value.epoch = 3;
    value.motion.as_mut().unwrap().as_mut().unwrap().goal = Some(AcceptedMovementGoal {
        control_owner: 4,
        control_sequence: 5,
        goal: ServerMovementGoal::MoveToObject {
            target: EntityId(7),
            parameters: AcceptedMoveParameters {
                flags: 0x1f09eff0,
                distance_to_object: 0.5,
                min_distance: 0.1,
                fail_distance: 15.,
                speed: 1.5,
                walk_run_threshold: 15.,
                desired_heading: 0.,
            },
            run_rate: 1.2,
        },
        target_position: Some((0x10100002, [9., 8., 7.])),
    });
    let mut counters = Sequences::new(10).unwrap();
    let result =
        ObjectProjection::prepare(&source(), value, None, &mut counters, limits()).unwrap();
    assert_eq!(result.description.physics.sequences.teleport, 3);
    let Some(PhysicsMovement::Motion(m)) = &result.description.physics.options.movement else {
        panic!()
    };
    assert!(
        matches!(&m.body,MotionBody::MoveToObject{target:7,cell:0x10100002,origin,parameters,run_rate} if *origin==[9.,8.,7.]&&parameters.flags==0x1f09eff0&&*run_rate==1.2)
    );
}

#[test]
fn canonical_portal_flags_survive_immutable_initial_blueprint_without_repeating_state_update() {
    let source = source();
    let mut counters = Sequences::new(10).unwrap();
    let first = ObjectProjection::prepare(&source, view(), None, &mut counters, limits()).unwrap();
    let before = counters.current(K::ObjectState, 0);
    let materialized = ObjectProjection::prepare_with_public_state(
        &source,
        view(),
        Some(&first),
        &mut counters,
        limits(),
        false,
        Some(8),
    )
    .unwrap();
    assert_eq!(materialized.description.physics.state, 8);
    assert_eq!(source.physics.state, 0x404410);
    assert_eq!(
        counters.current(K::ObjectState, 0),
        before,
        "portal already projected its state stamp"
    );
    assert!(
        materialized
            .updates
            .iter()
            .all(|m| u32::from_le_bytes(m.bytes[..4].try_into().unwrap()) != 0xf74b)
    );
}
