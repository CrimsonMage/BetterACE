use super::*;
fn item(stack: u32, revision: u64) -> bace_inventory::InventoryItem {
    bace_inventory::InventoryItem {
        id: EntityId(3),
        revision,
        template: 100,
        stack_key: 1,
        place: bace_inventory::ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0x800000,
        },
        stack,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0x800000,
        incompatible_wield: 0,
        wield_requirements_met: true,
        structure: None,
    }
}
#[test]
fn committed_ammo_updates_public_child_once_and_last_shot_removes_future_create_ghost() {
    let mut service = VisibilityService::new(limits()).unwrap();
    service.bind(key(2), binding(2)).unwrap();
    let mut parent = (*description(1)).clone();
    parent.physics.options.children = vec![
        PhysicsChild {
            object_id: 3,
            location: 1,
        },
        PhysicsChild {
            object_id: 4,
            location: 2,
        },
    ];
    let mut ammo = (*description(3)).clone();
    ammo.physics.options.parent = Some(PhysicsParent {
        object_id: 1,
        location: 1,
    });
    ammo.game.options.stack_size = Some(2);
    ammo.game.options.max_stack_size = Some(100);
    ammo.game.options.value = Some(2);
    ammo.game.options.burden = Some(2);
    let mut other = (*description(4)).clone();
    other.physics.options.parent = Some(PhysicsParent {
        object_id: 1,
        location: 2,
    });
    let children = vec![Arc::new(ammo), Arc::new(other)];
    service
        .register_object(1, 1, 0, Arc::new(parent.clone()), children.clone())
        .unwrap();
    stage(&mut service, key(2), 0, &[1], 1.);
    let first = bace_inventory::ItemChange {
        before: Some(item(2, 5)),
        after: item(1, 6),
    };
    service
        .apply_ammunition_child(binding(1), 10, &first)
        .unwrap();
    let current = &service.objects[&EntityId(1)].blueprint;
    assert_eq!(current.description.model, parent.model);
    assert_eq!(current.children[1], children[1]);
    assert_eq!(current.children[0].game.options.stack_size, Some(1));
    assert_eq!(
        service.observers[&key(2)]
            .publication
            .as_ref()
            .unwrap()
            .sent[&EntityId(1)]
            .blueprint
            .children[0]
            .game
            .options
            .stack_size,
        Some(2),
        "queued old Create stays immutable"
    );
    acknowledge(&mut service, key(2));
    stage(&mut service, key(2), 1, &[1], 1.);
    let updated = acknowledge(&mut service, key(2));
    assert!(
        updated
            .iter()
            .any(|bytes| u32::from_le_bytes(bytes[4..8].try_into().unwrap()) == 3)
    );
    let mut after = item(0, 7);
    after.place = bace_inventory::ItemPlace::Removed;
    let last = bace_inventory::ItemChange {
        before: Some(item(1, 6)),
        after,
    };
    service
        .apply_ammunition_child(binding(1), 11, &last)
        .unwrap();
    let revision = service.objects[&EntityId(1)].blueprint.revision;
    service
        .apply_ammunition_child(binding(1), 11, &last)
        .unwrap();
    assert_eq!(service.objects[&EntityId(1)].blueprint.revision, revision);
    let update = crate::inventory_equipment_output::EquipmentVisibilityUpdate {
        actor: EntityId(1),
        operation: 9,
        before_revision: 0,
        after_revision: 1,
        incarnation: 1,
        instance_sequence: 0,
        model: parent.model.clone(),
        children: parent.physics.options.children.clone(),
        descriptions: children,
    };
    service.replace_equipment_blueprint(&update).unwrap();
    service.replace_equipment_blueprint(&update).unwrap();
    assert_eq!(
        service.objects[&EntityId(1)]
            .blueprint
            .description
            .physics
            .options
            .children,
        vec![PhysicsChild {
            object_id: 4,
            location: 2
        }],
        "late equipment metadata cannot resurrect consumed ammo"
    );
    stage(&mut service, key(2), 2, &[1], 1.);
    let output = acknowledge(&mut service, key(2));
    assert!(
        output.contains(
            &ObjectControl::Delete {
                object_id: 3,
                instance_sequence: 0,
            }
            .encode()
        )
    );
    service.bind(key(3), binding(9)).unwrap();
    stage(&mut service, key(3), 3, &[1], 1.);
    let output = acknowledge(&mut service, key(3));
    assert_eq!(
        output
            .iter()
            .map(|bytes| u32::from_le_bytes(bytes[4..8].try_into().unwrap()))
            .collect::<Vec<_>>(),
        vec![1, 4]
    );
}
