use super::*;
use crate::inventory_equipment_output::EquipmentVisibilityUpdate;
fn update(service: &VisibilityService) -> EquipmentVisibilityUpdate {
    let mut child = (*description(4)).clone();
    child.physics.options.parent = Some(PhysicsParent {
        object_id: 3,
        location: 1,
    });
    EquipmentVisibilityUpdate {
        actor: EntityId(3),
        operation: 10,
        before_revision: 4,
        after_revision: 5,
        incarnation: 1,
        instance_sequence: service.objects[&EntityId(3)]
            .blueprint
            .description
            .physics
            .sequences
            .instance,
        model: ObjectModel::default(),
        children: vec![PhysicsChild {
            object_id: 4,
            location: 1,
        }],
        descriptions: vec![Arc::new(child)],
    }
}
#[test]
fn exact_equipment_handoff_preserves_pose_and_pinned_reliable_before_images() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(1), binding(1)).unwrap();
    service.bind(key(2), binding(2)).unwrap();
    service
        .register_object(1, 1, 0, description(3), vec![])
        .unwrap();
    stage(&mut service, key(1), 1, &[3], 2.);
    let old = service.objects[&EntityId(3)].blueprint.clone();
    let pose = service.objects[&EntityId(3)]
        .projection
        .as_ref()
        .unwrap()
        .view
        .clone();
    let change = update(&service);
    service.replace_equipment_blueprint(&change).unwrap();
    assert_eq!(
        service.objects[&EntityId(3)]
            .projection
            .as_ref()
            .unwrap()
            .view,
        pose
    );
    assert!(old.children.is_empty());
    assert!(
        service.observers[&key(1)]
            .publication
            .as_ref()
            .unwrap()
            .sent[&EntityId(3)]
            .blueprint
            .children
            .is_empty()
    );
    assert_eq!(acknowledge(&mut service, key(1)).len(), 1);
    let revision = service.objects[&EntityId(3)].blueprint.revision;
    service.replace_equipment_blueprint(&change).unwrap();
    assert_eq!(service.objects[&EntityId(3)].blueprint.revision, revision);
    // Both a known root and a newly entering observer receive its new child.
    for observer in [key(1), key(2)] {
        stage(&mut service, observer, 2, &[3], 3.);
        let output = acknowledge(&mut service, observer);
        assert!(
            output
                .iter()
                .any(
                    |bytes| u32::from_le_bytes(bytes[0..4].try_into().unwrap()) == 0xf745
                        && u32::from_le_bytes(bytes[4..8].try_into().unwrap()) == 4
                )
        );
        assert_eq!(
            service.objects[&EntityId(3)]
                .projection
                .as_ref()
                .unwrap()
                .description
                .physics
                .options
                .position
                .unwrap()
                .origin,
            [3., 0., 0.]
        );
    }
    let mut unequip = change.clone();
    unequip.operation += 1;
    unequip.before_revision = 8;
    unequip.after_revision = 9;
    unequip.children.clear();
    unequip.descriptions.clear();
    service.replace_equipment_blueprint(&unequip).unwrap();
    stage(&mut service, key(1), 3, &[3], 4.);
    let output = acknowledge(&mut service, key(1));
    assert_eq!(
        output[0],
        ObjectControl::Delete {
            object_id: 4,
            instance_sequence: change.descriptions[0].physics.sequences.instance
        }
        .encode()
    );
    assert!(service.replace_equipment_blueprint(&change).is_err());
}
#[test]
fn stale_or_mismatched_attachments_do_not_replace_any_blueprint() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service
        .register_object(1, 1, 0, description(3), vec![])
        .unwrap();
    let valid = update(&service);
    let old = service.objects[&EntityId(3)].blueprint.clone();
    for error in 0..5 {
        let mut candidate = valid.clone();
        match error {
            0 => candidate.incarnation = 2,
            1 => candidate.instance_sequence = 1,
            2 => candidate.after_revision = candidate.before_revision,
            3 => candidate.children[0].location = 2,
            _ => candidate
                .descriptions
                .push(candidate.descriptions[0].clone()),
        }
        assert!(service.replace_equipment_blueprint(&candidate).is_err());
        assert!(Arc::ptr_eq(&old, &service.objects[&EntityId(3)].blueprint));
    }
    service.replace_equipment_blueprint(&valid).unwrap();
    let mut duplicate = valid.clone();
    duplicate.after_revision += 1;
    assert!(service.replace_equipment_blueprint(&duplicate).is_err());
}
