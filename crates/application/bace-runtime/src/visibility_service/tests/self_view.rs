use super::*;
fn self_stage(
    service: &mut VisibilityService,
    tick: u64,
    epoch: u16,
    motion: AcceptedObjectMotion,
) {
    let correlation = service.next().unwrap();
    let key = key(1);
    let o = service.observers.get_mut(&key).unwrap();
    o.query = Some(correlation);
    let binding = o.binding;
    service
        .accept_visibility(VisibilityOutcome {
            correlation,
            result: Ok(VisibilitySnapshot {
                binding,
                observer_epoch: epoch,
                tick,
                candidates: vec![],
            }),
        })
        .unwrap();
    let o = service.observers.get_mut(&key).unwrap();
    let request = o.view_request.take().unwrap();
    assert_eq!(request.entities, vec![binding.actor]);
    o.view_query = Some(request.correlation);
    service
        .accept_views_inner(
            &ObjectViewOutcome {
                correlation: request.correlation,
                result: Ok(ObjectViewSnapshot {
                    binding,
                    observer_epoch: epoch,
                    tick,
                    views: vec![(
                        binding.actor,
                        Ok(AcceptedObjectView {
                            entity: binding.actor,
                            cell: 0x10100001,
                            position: [tick as f32, 0., 0.],
                            velocity: [0.; 3],
                            heading_radians: 0.,
                            grounded: true,
                            epoch,
                            held: false,
                            motion: Ok(Some(motion)),
                        }),
                    )],
                }),
            },
            None,
        )
        .unwrap();
}
fn ready() -> AcceptedObjectMotion {
    AcceptedObjectMotion {
        autonomous: false,
        style: 0x80000049,
        forward_motion: 0x41000003,
        forward_rate: 1.,
        sidestep_rate: 0.,
        turn_rate: 0.,
        actions: vec![],
        goal: None,
    }
}
fn motion_packet(messages: &[Vec<u8>]) -> &[u8] {
    messages
        .iter()
        .find(|m| {
            u32::from_le_bytes(m[..4].try_into().unwrap())
                == bace_wire::opcode::GameMessageOpcode::Motion.0
        })
        .unwrap()
}
#[test]
fn lone_player_gets_server_cast_move_death_and_epoch_without_self_create() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service
        .register_object(1, 1, 0, description(1), vec![])
        .unwrap();
    self_stage(&mut service, 0, 0, ready());
    assert!(!service.knows(key(1), EntityId(1)));
    let initial = acknowledge(&mut service, key(1));
    assert!(
        initial
            .iter()
            .all(|m| u32::from_le_bytes(m[..4].try_into().unwrap()) != 0xf745)
    );
    let mut cast = ready();
    cast.actions.push(AcceptedMotionAction {
        domain: AcceptedMotionDomain::Casting,
        owner: 7,
        sequence: 1,
        motion: 0x13000132,
        speed: 2.,
    });
    self_stage(&mut service, 1, 0, cast);
    let messages = acknowledge(&mut service, key(1));
    assert_eq!(motion_packet(&messages)[14], 0);
    assert_eq!(
        service.objects[&EntityId(1)]
            .sequences
            .current(bace_replication::SequenceKind::Motion, 0),
        2
    );
    let mut movement = ready();
    movement.goal = Some(AcceptedMovementGoal {
        control_owner: 7,
        control_sequence: 1,
        goal: ServerMovementGoal::MoveToObject {
            target: EntityId(2),
            parameters: AcceptedMoveParameters {
                flags: 0x1f09eff0,
                distance_to_object: 1.,
                min_distance: 0.1,
                fail_distance: 15.,
                speed: 1.5,
                walk_run_threshold: 15.,
                desired_heading: 0.,
            },
            run_rate: 1.2,
        },
        target_position: Some((0x10100001, [5., 5., 0.])),
    });
    self_stage(&mut service, 2, 0, movement);
    let messages = acknowledge(&mut service, key(1));
    assert_eq!(motion_packet(&messages)[16], 6);
    let mut death = ready();
    death.forward_motion = 0x40000011;
    self_stage(&mut service, 3, 0, death);
    let messages = acknowledge(&mut service, key(1));
    assert_eq!(motion_packet(&messages)[14], 0);
    let Some(PhysicsMovement::Motion(m)) = &service.objects[&EntityId(1)]
        .projection
        .as_ref()
        .unwrap()
        .description
        .physics
        .options
        .movement
    else {
        panic!()
    };
    assert!(matches!(&m.body,MotionBody::State{state,..} if state.forward_command==Some(0x11)));
    service.request_self_correction(key(1)).unwrap();
    service.request_self_correction(key(1)).unwrap();
    self_stage(&mut service, 4, 1, ready());
    let messages = acknowledge(&mut service, key(1));
    let position = messages
        .iter()
        .find_map(|m| PositionUpdate::decode(m).ok())
        .unwrap();
    assert_eq!(position.pack.teleport_sequence, 1);
    assert_eq!(position.pack.force_position_sequence, 1);
    assert!(!service.knows(key(1), EntityId(1)));
}
