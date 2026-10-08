use super::*;
use bace_gameplay_api::SessionId;
use bace_types::AccountId;
use bace_wire::*;
fn key(id: u16) -> SessionKey {
    SessionKey { id, generation: 1 }
}
fn binding(id: u32) -> CharacterBinding {
    CharacterBinding {
        session: SessionId(u64::from(id)),
        account: AccountId(u64::from(id)),
        actor: EntityId(id),
    }
}
fn limits() -> VisibilityLimits {
    VisibilityLimits {
        observers: 4,
        objects: 16,
        known_per_observer: 8,
        batch_bytes: 256,
        retained_bytes: 8192,
        codec: ObjectCodecLimits {
            max_message_bytes: 256,
            max_model_entries: 32,
            max_children: 8,
            max_restrictions: 8,
            max_motion_commands: 8,
            max_string_bytes: 64,
        },
    }
}
fn description(id: u32) -> Arc<ObjectDescription> {
    Arc::new(ObjectDescription {
        object_id: id,
        model: ObjectModel::default(),
        physics: PhysicsDescription {
            state: 0,
            options: PhysicsOptions::default(),
            sequences: bace_replication::physics_sequences(&Sequences::new(10).unwrap()),
        },
        game: ObjectGameData {
            name: "fixture".into(),
            class_id: 1,
            icon_id: 0x06000001,
            item_type: 16,
            description_flags: 0,
            options: ObjectGameOptions::default(),
        },
    })
}

#[test]
fn staff_name_reads_only_the_accepted_visible_blueprint() {
    let mut service = VisibilityService::new(limits()).unwrap();
    assert_eq!(service.registered_object_name(EntityId(2)), None);
    let mut object = (*description(2)).clone();
    object.game.name = "Drudge".into();
    service
        .register_object(1, 1, 0, Arc::new(object), vec![])
        .unwrap();
    assert_eq!(service.registered_object_name(EntityId(2)), Some("Drudge"));
    assert_eq!(service.registered_object_name(EntityId(3)), None);
}

#[test]
fn generated_blueprint_create_and_retirement_wait_for_observer_receipts() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    let generated = EntityId(2);
    stage(&mut service, key(1), 0, &[generated.0], 1.);
    assert!(acknowledge(&mut service, key(1)).is_empty());
    assert!(!service.knows(key(1), generated));

    let blueprint = crate::visibility_assets::PreparedVisibilityObject {
        incarnation: 7,
        revision: 1,
        description: description(generated.0),
        children: vec![],
    };
    blueprint.register(&mut service, 1).unwrap();
    stage(&mut service, key(1), 1, &[generated.0], 1.);
    assert!(!service.knows(key(1), generated));
    assert!(service.observers[&key(1)].publication.is_some());
    let created = acknowledge(&mut service, key(1));
    assert_eq!(
        u32::from_le_bytes(created[0][..4].try_into().unwrap()),
        0xf745
    );
    assert!(service.knows(key(1), generated));

    service.retire_object(generated, 2).unwrap();
    assert_eq!(service.registered_object_name(generated), None);
    let budget = service.limits.retained_bytes;
    service.retained_bytes = budget;
    service.publish_retirement(key(1)).unwrap();
    assert!(service.knows(key(1), generated));
    assert_eq!(service.observers[&key(1)].retirements.len(), 1);
    assert!(service.observers[&key(1)].publication.is_none());
    service.retained_bytes = 0;
    service.publish_retirement(key(1)).unwrap();
    let deleted = acknowledge(&mut service, key(1));
    assert_eq!(
        deleted,
        vec![
            ObjectControl::Delete {
                object_id: generated.0,
                instance_sequence: 0,
            }
            .encode()
        ]
    );
    assert!(!service.knows(key(1), generated));
}
fn stage(service: &mut VisibilityService, key: SessionKey, tick: u64, ids: &[u32], x: f32) {
    let correlation = service.next().unwrap();
    let o = service.observers.get_mut(&key).unwrap();
    o.query = Some(correlation);
    let binding = o.binding;
    service
        .accept_visibility(VisibilityOutcome {
            correlation,
            result: Ok(VisibilitySnapshot {
                binding,
                observer_epoch: 0,
                tick,
                candidates: ids
                    .iter()
                    .map(|id| VisibilityCandidate {
                        entity: EntityId(*id),
                        distance_squared: 1.,
                    })
                    .collect(),
            }),
        })
        .unwrap();
    let o = service.observers.get_mut(&key).unwrap();
    let request = o.view_request.take().unwrap();
    o.view_query = Some(request.correlation);
    service
        .accept_views_inner(
            &ObjectViewOutcome {
                correlation: request.correlation,
                result: Ok(ObjectViewSnapshot {
                    binding,
                    observer_epoch: 0,
                    tick,
                    views: request
                        .entities
                        .iter()
                        .map(|id| {
                            (
                                *id,
                                Ok(AcceptedObjectView {
                                    entity: *id,
                                    cell: 0x10100001,
                                    position: [x, 0., 0.],
                                    velocity: [0.; 3],
                                    heading_radians: 0.,
                                    grounded: true,
                                    epoch: 0,
                                    held: false,
                                    motion: Ok(None),
                                }),
                            )
                        })
                        .collect(),
                }),
            },
            None,
        )
        .unwrap();
}
fn acknowledge(service: &mut VisibilityService, key: SessionKey) -> Vec<Vec<u8>> {
    let mut output = Vec::new();
    while service.observers[&key].publication.is_some() {
        let correlation = service.next().unwrap();
        let p = service
            .observers
            .get_mut(&key)
            .unwrap()
            .publication
            .as_mut()
            .unwrap();
        output.extend(
            p.messages
                .front()
                .unwrap()
                .iter()
                .map(|(_, bytes)| bytes.clone()),
        );
        p.inflight = Some(correlation);
        assert!(service.reliable_admission(key, correlation, true).unwrap());
    }
    output
}
#[test]
fn reliable_receipts_gate_both_observers_and_position_uses_one_counter_owner() {
    let mut service = VisibilityService::new(limits()).unwrap();
    for id in [1, 2] {
        service.bind(key(id), binding(u32::from(id))).unwrap();
    }
    service
        .register_object(1, 1, 0, description(3), vec![])
        .unwrap();
    stage(&mut service, key(1), 0, &[3], 1.);
    stage(&mut service, key(2), 0, &[3], 1.);
    assert!(!service.knows(key(1), EntityId(3)));
    assert!(!service.knows(key(2), EntityId(3)));
    let first = acknowledge(&mut service, key(1));
    assert!(service.knows(key(1), EntityId(3)));
    assert!(!service.knows(key(2), EntityId(3)));
    assert_eq!(first, acknowledge(&mut service, key(2)));
    stage(&mut service, key(1), 1, &[3], 2.);
    stage(&mut service, key(2), 1, &[3], 2.);
    let first = acknowledge(&mut service, key(1));
    assert_eq!(first, acknowledge(&mut service, key(2)));
    let position = first
        .iter()
        .find_map(|bytes| PositionUpdate::decode(bytes).ok())
        .unwrap();
    assert_eq!(position.pack.position.origin, [2., 0., 0.]);
    assert_eq!(position.pack.position_sequence, 1);
}
#[test]
fn chunk_pressure_stale_generation_and_negative_receipt_retain_knowledge() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    for id in 2..=5 {
        service
            .register_object(1, 1, 0, description(id), vec![])
            .unwrap();
    }
    stage(&mut service, key(1), 0, &[2, 3, 4, 5], 1.);
    let p = service
        .observers
        .get_mut(&key(1))
        .unwrap()
        .publication
        .as_mut()
        .unwrap();
    assert!(p.messages.len() > 1);
    p.inflight = Some(77);
    assert!(
        !service
            .reliable_admission(
                SessionKey {
                    id: 1,
                    generation: 2
                },
                77,
                true
            )
            .unwrap()
    );
    assert!(!service.reliable_admission(key(1), 78, true).unwrap());
    service.reliable_admission(key(1), 77, true).unwrap();
    assert!(!service.knows(key(1), EntityId(2)));
    let p = service
        .observers
        .get_mut(&key(1))
        .unwrap()
        .publication
        .as_mut()
        .unwrap();
    p.inflight = Some(79);
    assert_eq!(
        service.reliable_admission(key(1), 79, false),
        Err(VisibilityServiceError::Rejected)
    );
    assert!(!service.knows(key(1), EntityId(2)));
    assert!(service.pending());
    service.unbind(key(1));
    assert_eq!(service.retained_bytes, 0);
    assert!(!service.reliable_admission(key(1), 79, true).unwrap());
}
#[test]
fn incarnation_reuse_deletes_old_instance_before_fresh_create() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service
        .register_object(1, 1, 0, description(2), vec![])
        .unwrap();
    stage(&mut service, key(1), 0, &[2], 1.);
    acknowledge(&mut service, key(1));
    let mut second = (*description(2)).clone();
    second.physics.sequences.instance = 7;
    service
        .register_object(2, 1, 1, Arc::new(second), vec![])
        .unwrap();
    stage(&mut service, key(1), 1, &[2], 2.);
    let messages = acknowledge(&mut service, key(1));
    assert_eq!(
        messages[0],
        ObjectControl::Delete {
            object_id: 2,
            instance_sequence: 0
        }
        .encode()
    );
    assert_eq!(
        u32::from_le_bytes(messages[1][..4].try_into().unwrap()),
        0xf745
    );
    assert!(service.knows(key(1), EntityId(2)));
}
#[test]
fn reset_waits_for_existing_publication_then_removes_before_recreating() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service
        .register_object(1, 1, 0, description(2), vec![])
        .unwrap();
    stage(&mut service, key(1), 0, &[2], 1.);
    assert_eq!(
        service.reset_observer(key(1), 1),
        Err(VisibilityServiceError::Busy)
    );
    assert_eq!(service.observers[&key(1)].reset_requested, Some(1));
    acknowledge(&mut service, key(1));
    assert!(service.knows(key(1), EntityId(2)));
    service.reset_observer(key(1), 1).unwrap();
    assert!(service.knows(key(1), EntityId(2)));
    let messages = acknowledge(&mut service, key(1));
    assert_eq!(
        messages,
        vec![
            ObjectControl::Delete {
                object_id: 2,
                instance_sequence: 0
            }
            .encode()
        ]
    );
    assert!(!service.knows(key(1), EntityId(2)));
    service.reset_observer(key(1), 1).unwrap();
    assert!(service.observers[&key(1)].publication.is_none());
    stage(&mut service, key(1), 2, &[2], 2.);
    assert!(!service.knows(key(1), EntityId(2)));
    acknowledge(&mut service, key(1));
    assert!(service.knows(key(1), EntityId(2)));
}

mod equipment;
#[test]
fn durable_gear_replaces_public_children_and_keeps_private_objects_out() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service
        .register_object(1, 1, 0, description(2), vec![])
        .unwrap();
    stage(&mut service, key(1), 0, &[2], 1.);
    acknowledge(&mut service, key(1));
    let mut child = (*description(9)).clone();
    child.physics.options.parent = Some(PhysicsParent {
        object_id: 2,
        location: 1,
    });
    child.physics.options.movement = Some(PhysicsMovement::AnimationFrame(1));
    let update = crate::inventory_equipment_output::EquipmentVisibilityUpdate {
        actor: EntityId(2),
        operation: 10,
        before_revision: 0,
        after_revision: 1,
        incarnation: 1,
        instance_sequence: 0,
        model: description(2).model.clone(),
        children: vec![PhysicsChild {
            object_id: 9,
            location: 1,
        }],
        descriptions: vec![Arc::new(child)],
    };
    service.replace_equipment_blueprint(&update).unwrap();
    service.replace_equipment_blueprint(&update).unwrap();
    stage(&mut service, key(1), 1, &[2], 1.);
    let messages = acknowledge(&mut service, key(1));
    assert_eq!(
        u32::from_le_bytes(messages[0][..4].try_into().unwrap()),
        0xf745
    );
    assert_eq!(u32::from_le_bytes(messages[0][4..8].try_into().unwrap()), 9);
    let mut removal = update;
    removal.operation = 11;
    removal.before_revision = 1;
    removal.after_revision = 2;
    removal.children.clear();
    removal.descriptions.clear();
    service.replace_equipment_blueprint(&removal).unwrap();
    stage(&mut service, key(1), 2, &[2], 1.);
    let messages = acknowledge(&mut service, key(1));
    assert_eq!(
        messages[0],
        ObjectControl::Delete {
            object_id: 9,
            instance_sequence: 0
        }
        .encode()
    );
    removal.operation = 12;
    removal.before_revision = 2;
    removal.after_revision = 3;
    removal.descriptions = vec![description(10)];
    assert_eq!(
        service.replace_equipment_blueprint(&removal),
        Err(VisibilityServiceError::InvalidDescription)
    );
}
mod ammunition;
mod projectiles;
mod self_view;
