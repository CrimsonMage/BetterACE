use super::*;
use crate::{PlayerDeathEvent, PlayerDeathReceipt, PreparedPlayerNoCorpse};
use bace_character::VitalFormula;
use bace_entity::{EntityProperties, PropertyFamily, PropertyValue};

const ACTOR: EntityId = EntityId(0x50000001);

#[test]
fn no_corpse_zero_drop_rejects_stale_pose_then_adopts_once_and_respawns() {
    let mut kernel = super::tests::fixture();
    kernel
        .world
        .register_properties(
            ACTOR,
            EntityProperties::restore_snapshot(
                1,
                vec![(PropertyFamily::Bool, 29, PropertyValue::Bool(true))],
            )
            .unwrap(),
        )
        .unwrap();
    kernel
        .world
        .combatant_mut(ACTOR)
        .unwrap()
        .damage(100)
        .unwrap();
    kernel.begin_player_death(ACTOR, None).unwrap();
    let PlayerDeathEvent::Prepare { operation, .. } = kernel.take_player_death_event().unwrap()
    else {
        panic!("Prepare absent")
    };
    let accepted = kernel.accepted_portal_position(ACTOR).unwrap();
    let position = bace_content::Position {
        obj_cell_id: accepted.cell,
        position_x: accepted.origin[0],
        position_y: accepted.origin[1],
        position_z: accepted.origin[2],
        rotation_w: accepted.rotation[0],
        rotation_x: accepted.rotation[1],
        rotation_y: accepted.rotation[2],
        rotation_z: accepted.rotation[3],
    };
    let prepared = PreparedPlayerNoCorpse {
        operation,
        actor: ACTOR,
        existing: vec![],
        fresh_items: vec![],
        fresh_containers: vec![],
        world_roots: vec![],
        accepted_position: position,
        animation_seconds: 0.,
        vital_formulas: [VitalFormula {
            enabled: false,
            divisor: 0,
            attribute1: 0,
            attribute2: 0,
        }; 3],
        equipped_health: vec![],
        instantiation: Some(bace_interactions::PortalPosition {
            cell: 1,
            origin: [10., 10., 0.5],
            rotation: [1., 0., 0., 0.],
        }),
    };
    let mut stale = prepared;
    stale.accepted_position.position_x += 1.;
    let (error, stale) = kernel.prepare_player_no_corpse(stale).unwrap_err();
    assert_eq!(error, E::Stale);
    assert!(kernel.take_player_death_proposal().is_none());
    assert!(kernel.inventory.reserved(ACTOR));
    let mut prepared = *stale;
    prepared.accepted_position.position_x -= 1.;
    kernel
        .prepare_player_no_corpse(prepared)
        .map_err(|e| e.0)
        .unwrap();
    let ticket = kernel.take_player_death_proposal().unwrap();
    assert!(ticket.no_corpse.as_ref().unwrap().world_roots.is_empty());
    assert_eq!(ticket.corpse, EntityId(0));
    assert!(ticket.inventory.proposal.changes.is_empty());
    let receipt = PlayerDeathReceipt {
        operation,
        actor: ACTOR,
        after_revision: ticket.after_revision,
        inventory: crate::InventoryReceipt {
            operation: ticket.inventory.operation,
            revisions: vec![],
        },
    };
    assert_eq!(
        kernel.confirm_player_death_committed_at(&receipt, 1),
        Err(E::Receipt)
    );
    kernel
        .confirm_player_death_committed_at(&receipt, 0)
        .unwrap();
    kernel
        .confirm_player_death_committed_at(&receipt, 0)
        .unwrap();
    assert!(matches!(
        kernel.take_player_death_event(),
        Some(PlayerDeathEvent::Started { .. })
    ));
    for _ in 0..31 {
        kernel.step().unwrap();
    }
    let event = kernel.take_player_death_event().unwrap();
    assert!(
        matches!(event, PlayerDeathEvent::WorldDrops { operation: op, ref roots, .. } if op == operation && roots.is_empty())
    );
    assert!(kernel.world.contains_identity(ACTOR));
    assert_eq!(kernel.world.combatant(ACTOR).unwrap().health(), 0);
    for _ in 0..90 {
        kernel.step().unwrap();
    }
    assert!(
        matches!(kernel.take_player_death_event(), Some(PlayerDeathEvent::Respawned { operation: op, .. }) if op == operation)
    );
    assert_eq!(kernel.world.combatant(ACTOR).unwrap().health(), 71);
}

#[test]
fn selected_pack_container_freezes_nested_unchanged_forest_before_receipt() {
    use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
    let mut kernel = super::tests::fixture();
    kernel
        .world
        .register_properties(
            ACTOR,
            EntityProperties::restore_snapshot(
                1,
                vec![(PropertyFamily::Bool, 29, PropertyValue::Bool(true))],
            )
            .unwrap(),
        )
        .unwrap();
    let root = EntityId(21);
    let nested = EntityId(22);
    let child = EntityId(23);
    let grandchild = EntityId(24);
    for id in [root, nested] {
        kernel
            .register_inventory_container(InventoryContainer {
                id,
                revision: 0,
                root_owner: None,
                slots: 40,
                pack_slots: 10,
                burden_limit: 100000,
                accessible: true,
                open: true,
                generation: 1,
            })
            .unwrap();
    }
    let item = |id, container, slot, pack_slot, is_container| InventoryItem {
        id,
        revision: 0,
        template: id.0 + 100,
        stack_key: u64::from(id.0),
        place: ItemPlace::Contained {
            container,
            slot,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot,
        is_container,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: false,
        structure: None,
    };
    for row in [
        item(root, ACTOR, 1, true, true),
        item(nested, root, 0, true, true),
        item(child, root, 1, false, false),
        item(grandchild, nested, 0, false, false),
    ] {
        kernel.register_inventory_item(row).unwrap();
    }
    kernel
        .world
        .combatant_mut(ACTOR)
        .unwrap()
        .damage(100)
        .unwrap();
    kernel.begin_player_death(ACTOR, None).unwrap();
    let PlayerDeathEvent::Prepare { operation, .. } = kernel.take_player_death_event().unwrap()
    else {
        panic!("NoCorpse Prepare absent")
    };
    let accepted = kernel.accepted_portal_position(ACTOR).unwrap();
    let position = bace_content::Position {
        obj_cell_id: accepted.cell,
        position_x: accepted.origin[0],
        position_y: accepted.origin[1],
        position_z: accepted.origin[2],
        rotation_w: accepted.rotation[0],
        rotation_x: accepted.rotation[1],
        rotation_y: accepted.rotation[2],
        rotation_z: accepted.rotation[3],
    };
    let body = bace_physics::Body::spawn(
        kernel.world.scene(CellId(accepted.cell)).unwrap(),
        bace_geometry::Vec3::new(accepted.origin[0], accepted.origin[1], accepted.origin[2]),
        0.5,
        bace_motion::Capabilities {
            speed: 0.,
            jump_impulse: 0.,
        },
    )
    .unwrap();
    kernel
        .prepare_player_no_corpse(PreparedPlayerNoCorpse {
            operation,
            actor: ACTOR,
            existing: vec![root],
            fresh_items: vec![],
            fresh_containers: vec![],
            world_roots: vec![bace_entity::Actor {
                id: root,
                cell: CellId(accepted.cell),
                body,
            }],
            accepted_position: position,
            animation_seconds: 0.,
            vital_formulas: [VitalFormula {
                enabled: false,
                divisor: 0,
                attribute1: 0,
                attribute2: 0,
            }; 3],
            equipped_health: vec![],
            instantiation: Some(bace_interactions::PortalPosition {
                cell: 1,
                origin: [10., 10., 0.5],
                rotation: [1., 0., 0., 0.],
            }),
        })
        .map_err(|e| e.0)
        .unwrap();
    let ticket = kernel.take_player_death_proposal().unwrap();
    let plan = ticket.no_corpse.unwrap();
    assert_eq!(plan.world_roots, vec![root]);
    assert_eq!(
        plan.descendants
            .iter()
            .map(|d| (d.id, d.parent, d.slot, d.pack_slot, d.revision))
            .collect::<Vec<_>>(),
        vec![
            (child, root, 1, false, 1),
            (nested, root, 0, true, 1),
            (grandchild, nested, 0, false, 1),
        ]
    );
    assert!(plan.descendants.iter().all(|descendant| {
        ticket.inventory.proposal.changes.iter().any(|change| {
            change.after.id == descendant.id
                && change.before.as_ref().is_some_and(|before| {
                    before.place == change.after.place
                        && before.revision + 1 == change.after.revision
                })
        })
    }));
}
