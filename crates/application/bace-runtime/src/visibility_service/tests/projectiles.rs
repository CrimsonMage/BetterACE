use super::*;
fn launch() -> Arc<AcceptedProjectileLaunch> {
    Arc::new(AcceptedProjectileLaunch {
        tick: 1,
        view: AcceptedObjectView {
            entity: EntityId(3),
            cell: 0x10100001,
            position: [12., 4., 2.],
            velocity: [20., 0., 3.],
            heading_radians: -std::f32::consts::FRAC_PI_2,
            grounded: false,
            epoch: 0,
            held: false,
            motion: Ok(None),
        },
        observers: vec![ProjectileLaunchObserver {
            binding: binding(1),
            epoch: 0,
            distance_squared: 64.,
        }],
    })
}
#[test]
fn removed_before_cold_reply_projectile_still_creates_from_launch_then_retires() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service.bind(key(2), binding(2)).unwrap();
    service
        .register_object(1, 1, 0, description(4), vec![])
        .unwrap();
    stage(&mut service, key(1), 10, &[4], 1.);
    acknowledge(&mut service, key(1));
    service
        .register_object(1, 1, 1, description(3), vec![])
        .unwrap();
    let snapshot = launch();
    assert!(
        !service
            .publish_projectile_launch_inner(snapshot.clone(), None)
            .unwrap()
    );
    assert!(!service.knows(key(1), EntityId(3)));
    assert!(
        service.knows(key(1), EntityId(4)),
        "single-object evidence preserves current scene"
    );
    let messages = acknowledge(&mut service, key(1));
    // Default fixture model is the source four-byte model header. Physics has
    // position+velocity only, in the independently golden-tested wire order.
    let bytes = &messages[0];
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 3);
    assert_eq!(
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
        0x8004
    );
    let floats = |offset: usize| -> [f32; 3] {
        std::array::from_fn(|n| {
            f32::from_le_bytes(
                bytes[offset + n * 4..offset + n * 4 + 4]
                    .try_into()
                    .unwrap(),
            )
        })
    };
    assert_eq!(floats(24), [12., 4., 2.]);
    assert_eq!(floats(52), [20., 0., 3.]);
    assert!(
        service
            .publish_projectile_launch_inner(snapshot, None)
            .unwrap()
    );
    assert!(service.knows(key(1), EntityId(3)));
    assert!(
        !service.knows(key(2), EntityId(3)),
        "audience is exact launch PVS"
    );
    service.retire_object(EntityId(3), 2).unwrap();
    let o = service.observers.get_mut(&key(1)).unwrap();
    let (id, tick) = o.retirements.pop_front().unwrap();
    let delta = o.knowledge.stage_retirement(id, tick).unwrap().clone();
    let mut output = Vec::new();
    delivery::append_deletes(&mut output, &o.sent[&id].blueprint);
    service
        .publish(key(1), delta.ticket, output, BTreeMap::new(), delta.removes)
        .unwrap();
    assert!(service.knows(key(1), EntityId(3)));
    acknowledge(&mut service, key(1));
    assert!(!service.knows(key(1), EntityId(3)));
    assert!(service.knows(key(1), EntityId(4)));
}
#[test]
fn launch_waits_for_prior_reliable_work_and_never_crosses_reentry_generation() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service
        .register_object(1, 1, 0, description(4), vec![])
        .unwrap();
    stage(&mut service, key(1), 0, &[4], 1.);
    service
        .register_object(1, 1, 1, description(3), vec![])
        .unwrap();
    let snapshot = launch();
    assert!(
        !service
            .publish_projectile_launch_inner(snapshot.clone(), None)
            .unwrap()
    );
    assert!(service.observers[&key(1)].launches.contains(&EntityId(3)));
    acknowledge(&mut service, key(1));
    assert!(
        !service
            .publish_projectile_launch_inner(snapshot.clone(), None)
            .unwrap()
    );
    assert!(!service.knows(key(1), EntityId(3)));
    service.unbind(key(1));
    let newer = SessionKey {
        id: 1,
        generation: 2,
    };
    let mut rebound = binding(1);
    rebound.session = SessionId(2);
    service.bind(newer, rebound).unwrap();
    assert!(
        service
            .publish_projectile_launch_inner(snapshot, None)
            .unwrap()
    );
    assert!(!service.knows(newer, EntityId(3)));
    assert!(service.observers[&newer].publication.is_none());
}

#[test]
fn generic_birth_includes_public_children_and_never_creates_self() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service.bind(key(3), binding(3)).unwrap();
    let mut parent = (*description(3)).clone();
    parent.physics.options.children = vec![PhysicsChild {
        object_id: 5,
        location: 1,
    }];
    let mut child = (*description(5)).clone();
    child.physics.options.parent = Some(PhysicsParent {
        object_id: 3,
        location: 1,
    });
    service
        .register_object(1, 1, 1, Arc::new(parent), vec![Arc::new(child)])
        .unwrap();
    let mut snapshot = (*launch()).clone();
    snapshot.observers.push(ProjectileLaunchObserver {
        binding: binding(3),
        epoch: 0,
        distance_squared: 0.,
    });
    let snapshot: Arc<AcceptedObjectBirth> = Arc::new(snapshot);
    assert!(
        !service
            .publish_projectile_launch_inner(snapshot.clone(), None)
            .unwrap()
    );
    assert!(service.observers[&key(3)].publication.is_none());
    let output = acknowledge(&mut service, key(1));
    assert_eq!(
        output
            .iter()
            .map(|bytes| u32::from_le_bytes(bytes[4..8].try_into().unwrap()))
            .collect::<Vec<_>>(),
        vec![3, 5]
    );
    assert!(
        service
            .publish_projectile_launch_inner(snapshot, None)
            .unwrap()
    );
}
