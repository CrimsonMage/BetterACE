use bace_gameplay_api::{InventoryRejection as E, InventoryRequest as R};
use bace_inventory::*;
use bace_types::EntityId as Id;
fn item(id: u32, place: ItemPlace) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: Id(id),
        revision: 1,
        template: 100,
        stack_key: 10,
        place,
        stack: 10,
        maximum_stack: 100,
        unit_burden: 2,
        unit_value: 3,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 1,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
fn container() -> InventoryContainer {
    InventoryContainer {
        id: Id(1),
        revision: 1,
        root_owner: Some(Id(1)),
        slots: 10,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    }
}
fn authority() -> InventoryAuthority {
    InventoryAuthority {
        actor: Id(1),
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: true,
        source_view: None,
        destination_view: None,
        new_item: Some(Id(0x80000001)),
    }
}
fn place(slot: u32) -> ItemPlace {
    ItemPlace::Contained {
        container: Id(1),
        slot,
        equipped: 0,
    }
}
#[test]
fn burden_follows_nested_container_owner_during_corpse_transfer() {
    let mut corpse = container();
    corpse.id = Id(2);
    corpse.root_owner = None;
    let mut root = item(10, place(0));
    root.stack = 1;
    root.unit_burden = 5;
    root.is_container = true;
    let child = item(
        11,
        ItemPlace::Contained {
            container: Id(10),
            slot: 0,
            equipped: 0,
        },
    );
    let containers = [container(), corpse];
    assert_eq!(
        InventoryView {
            items: &[root.clone(), child.clone()],
            containers: &containers
        }
        .actor_burden(Id(1))
        .unwrap(),
        25
    );
    root.place = ItemPlace::Contained {
        container: Id(2),
        slot: 0,
        equipped: 0,
    };
    assert_eq!(
        InventoryView {
            items: &[root, child],
            containers: &containers
        }
        .actor_burden(Id(1))
        .unwrap(),
        0
    );
}
#[test]
fn pickup_inserts_and_drop_closes_slot_gap_without_mutating_inputs() {
    let items = [
        item(10, place(0)),
        item(11, place(1)),
        item(12, ItemPlace::World),
    ];
    let containers = [container()];
    let result = propose_inventory(
        R::Move {
            item: Id(12),
            container: Id(1),
            placement: 1,
        },
        authority(),
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    assert_eq!(
        result
            .changes
            .iter()
            .find(|c| c.after.id == Id(11))
            .unwrap()
            .after
            .place,
        place(2)
    );
    assert_eq!(
        result
            .changes
            .iter()
            .find(|c| c.after.id == Id(12))
            .unwrap()
            .after
            .place,
        place(1)
    );
    assert!(result.requires_pickup_motion);
    assert_eq!(items[1].place, place(1));
    let result = propose_inventory(
        R::Drop { item: Id(10) },
        authority(),
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    assert_eq!(
        result
            .changes
            .iter()
            .find(|c| c.after.id == Id(11))
            .unwrap()
            .after
            .place,
        place(0)
    );
}
#[test]
fn split_merge_conserve_stack_and_change_values_with_checked_revisions() {
    let items = [item(0x80000010, place(0)), item(11, place(1))];
    let containers = [container()];
    let split = propose_stack_split(
        R::SplitToContainer {
            item: Id(0x80000010),
            container: Id(1),
            placement: 1,
            amount: 3,
        },
        authority(),
        StackSplitPreparation {
            fresh: InventoryItem {
                revision: 0,
                ..item(0x80000001, place(1))
            },
            source_stackable: true,
            source_stuck: false,
            source_vendor: false,
            destination_corpse: false,
        },
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    assert_eq!(
        split
            .changes
            .iter()
            .find(|c| c.after.id == Id(0x80000010))
            .unwrap()
            .after
            .stack,
        7
    );
    assert_eq!(
        split
            .changes
            .iter()
            .find(|c| c.after.id == Id(0x80000001))
            .unwrap()
            .after
            .stack,
        3
    );
    let merge = propose_inventory(
        R::Merge {
            source: Id(0x80000010),
            target: Id(11),
            amount: 10,
        },
        authority(),
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    assert_eq!(
        merge
            .changes
            .iter()
            .find(|c| c.after.id == Id(0x80000010))
            .unwrap()
            .after
            .place,
        ItemPlace::Removed
    );
    assert_eq!(
        merge
            .changes
            .iter()
            .find(|c| c.after.id == Id(11))
            .unwrap()
            .after
            .stack,
        20
    );
}
#[test]
fn equipment_swap_returns_conflicting_piece_to_pack_atomically() {
    let items = [
        item(10, place(0)),
        item(
            11,
            ItemPlace::Contained {
                container: Id(1),
                slot: 1,
                equipped: 1,
            },
        ),
    ];
    let containers = [container()];
    let result = propose_inventory(
        R::Equip {
            item: Id(10),
            location: 1,
        },
        authority(),
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    assert_eq!(
        result
            .changes
            .iter()
            .find(|c| c.after.id == Id(11))
            .unwrap()
            .after
            .place,
        place(0)
    );
    assert_eq!(result.actor_burden, 40);
}
#[test]
fn geometry_attunement_capacity_and_stale_views_fail_atomically() {
    let mut i = item(10, place(0));
    i.attuned = true;
    let items = [i];
    let containers = [container()];
    assert_eq!(
        propose_inventory(
            R::Drop { item: Id(10) },
            authority(),
            InventoryView {
                items: &items,
                containers: &containers
            }
        ),
        Err(E::Attuned)
    );
    let items = [item(10, ItemPlace::World)];
    assert_eq!(
        propose_inventory(
            R::Move {
                item: Id(10),
                container: Id(1),
                placement: 0
            },
            InventoryAuthority {
                geometry_ready: false,
                ..authority()
            },
            InventoryView {
                items: &items,
                containers: &containers
            }
        ),
        Err(E::MissingGeometry)
    );
    let containers = [InventoryContainer {
        slots: 0,
        ..container()
    }];
    assert_eq!(
        propose_inventory(
            R::Move {
                item: Id(10),
                container: Id(1),
                placement: 0
            },
            authority(),
            InventoryView {
                items: &items,
                containers: &containers
            }
        ),
        Err(E::Capacity)
    );
}
#[test]
fn slot_insert_and_remove_match_unchanged_ace_container_statements() {
    for line in include_str!("fixtures/placement.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let parts: Vec<_> = line.split(',').collect();
        let mut items = vec![item(10, place(0)), item(11, place(1)), item(12, place(0))];
        items[2].pack_slot = true;
        let containers = [container()];
        let request = if parts[0] == "add" {
            let mut add = item(13, ItemPlace::World);
            add.pack_slot = parts[1] == "1";
            items.push(add);
            R::Move {
                item: Id(13),
                container: Id(1),
                placement: parts[2].parse().unwrap(),
            }
        } else {
            R::Drop {
                item: Id(parts[2].parse().unwrap()),
            }
        };
        let proposal = propose_inventory(
            request,
            authority(),
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )
        .unwrap();
        for c in proposal.changes {
            if let Some(i) = items.iter_mut().find(|i| i.id == c.after.id) {
                *i = c.after
            } else {
                items.push(c.after)
            }
        }
        items.sort_by_key(|i| i.id);
        let actual = items
            .iter()
            .filter_map(|i| match i.place {
                ItemPlace::Contained { slot, .. } => Some(format!("{}:{slot}", i.id.0)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(actual, parts[3], "{line}");
    }
}
#[test]
fn native_take_consumes_inventory_first_atomically_and_closes_multiple_gaps() {
    let mut items = vec![
        item(10, place(0)),
        item(11, place(1)),
        item(12, place(2)),
        item(
            13,
            ItemPlace::Contained {
                container: Id(1),
                slot: 1,
                equipped: 1,
            },
        ),
    ];
    items[2].template = 999;
    let containers = [container()];
    let result = propose_take_template(
        Id(1),
        100,
        None,
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.after.id == Id(10) && c.after.place == ItemPlace::Removed)
    );
    assert!(
        result
            .changes
            .iter()
            .any(|c| c.after.id == Id(11) && c.after.place == ItemPlace::Removed)
    );
    assert_eq!(
        result
            .changes
            .iter()
            .find(|c| c.after.id == Id(12))
            .unwrap()
            .after
            .place,
        place(0)
    );
    assert!(!result.changes.iter().any(|c| c.after.id == Id(13)));
}
#[test]
fn spell_components_reserve_all_requirements_and_burn_only_frozen_subset() {
    let mut items = [item(10, place(0)), item(11, place(1)), item(12, place(2))];
    items[0].template = 200;
    items[1].template = 201;
    items[2].template = 202;
    let containers = [container()];
    let view = || InventoryView {
        items: &items,
        containers: &containers,
    };
    assert!(has_required_components(Id(1), &[(200, 2), (201, 3)], view()).unwrap());
    let proposal =
        propose_component_use(Id(1), &[(200, 2), (201, 3)], &[(201, 1)], view()).unwrap();
    assert!(proposal.participants.iter().any(|(id, _)| *id == Id(10)));
    assert_eq!(proposal.changes.len(), 1);
    assert_eq!(proposal.changes[0].after.id, Id(11));
    assert_eq!(proposal.changes[0].after.stack, 9);
    let no_burn = propose_component_use(Id(1), &[(200, 2)], &[], view()).unwrap();
    assert!(no_burn.changes.is_empty());
    assert!(!no_burn.participants.is_empty());
    assert_eq!(
        propose_component_use(Id(1), &[(200, 11)], &[(200, 1)], view()),
        Err(E::Requirements)
    );
    assert_eq!(
        propose_component_use(Id(1), &[(200, 2)], &[(201, 1)], view()),
        Err(E::InvalidCount)
    );
}
#[test]
fn world_attuned_pickup_is_allowed_but_even_owned_house_storage_is_outside_carried_inventory() {
    let mut bound = item(10, ItemPlace::World);
    bound.attuned = true;
    let items = [bound.clone()];
    let containers = [container()];
    assert!(
        propose_inventory(
            R::Move {
                item: Id(10),
                container: Id(1),
                placement: 0
            },
            authority(),
            InventoryView {
                items: &items,
                containers: &containers
            }
        )
        .is_ok()
    );
    bound.place = place(0);
    let items = [bound];
    let containers = [
        container(),
        InventoryContainer {
            id: Id(99),
            root_owner: None,
            open: true,
            generation: 3,
            ..container()
        },
    ];
    assert_eq!(
        propose_inventory(
            R::Move {
                item: Id(10),
                container: Id(99),
                placement: 0
            },
            InventoryAuthority {
                destination_view: Some(3),
                ..authority()
            },
            InventoryView {
                items: &items,
                containers: &containers
            }
        ),
        Err(E::Attuned)
    );
}

#[test]
fn pet_device_consumes_structure_without_consuming_the_item_stack() {
    let mut device = item(100, place(0));
    device.structure = Some(3);
    let containers = [container()];
    let items = [device.clone()];
    let proposal = bace_inventory::propose_pet_charge(
        Id(1),
        device.id,
        false,
        InventoryView {
            items: &items,
            containers: &containers,
        },
    )
    .unwrap();
    let changed = proposal
        .changes
        .iter()
        .find(|c| c.after.id == device.id)
        .unwrap();
    assert_eq!(changed.after.structure, Some(2));
    assert_eq!(changed.after.stack, device.stack);
    assert!(changed.after.active_pet);
    assert_eq!(changed.after.revision, device.revision + 1);
    assert_eq!(items[0], device);
}

#[test]
fn split_uses_fresh_template_and_all_failures_preserve_source() {
    let mut source = item(0x80000010, place(0));
    source.unit_value = 99;
    source.unit_burden = 7;
    source.stack_key = 1234;
    let items = [source.clone()];
    let containers = [container()];
    let prepared = StackSplitPreparation {
        fresh: InventoryItem {
            revision: 0,
            ..item(0x80000001, place(1))
        },
        source_stackable: true,
        source_stuck: false,
        source_vendor: false,
        destination_corpse: false,
    };
    let request = |amount| R::SplitToContainer {
        item: source.id,
        container: Id(1),
        placement: 1,
        amount,
    };
    let view = || InventoryView {
        items: &items,
        containers: &containers,
    };
    assert_eq!(
        propose_inventory(request(3), authority(), view()),
        Err(E::InvalidState)
    );
    for amount in [i32::MIN, -1, 0, 10, 11, i32::MAX] {
        assert_eq!(
            propose_stack_split(request(amount), authority(), prepared.clone(), view()),
            Err(E::InvalidCount)
        );
    }
    let proposal = propose_stack_split(request(3), authority(), prepared.clone(), view()).unwrap();
    let fresh = &proposal
        .changes
        .iter()
        .find(|c| c.before.is_none())
        .unwrap()
        .after;
    assert_eq!(
        (
            fresh.stack,
            fresh.unit_value,
            fresh.unit_burden,
            fresh.stack_key
        ),
        (3, 3, 2, 10)
    );
    let remainder = &proposal
        .changes
        .iter()
        .find(|c| c.after.id == source.id)
        .unwrap()
        .after;
    assert_eq!(
        (
            remainder.stack,
            remainder.unit_value,
            remainder.unit_burden,
            remainder.stack_key
        ),
        (7, 99, 7, 1234)
    );
    assert_eq!(proposal.actor_burden, 55);
    assert!(!proposal.requires_pickup_motion);
    let mut missing_id = authority();
    missing_id.new_item = None;
    assert_eq!(
        propose_stack_split(request(3), missing_id, prepared.clone(), view()),
        Err(E::Capacity)
    );
    let full = [InventoryContainer {
        slots: 1,
        ..container()
    }];
    assert_eq!(
        propose_stack_split(
            request(3),
            authority(),
            prepared.clone(),
            InventoryView {
                items: &items,
                containers: &full
            }
        ),
        Err(E::Capacity)
    );
    let no_drop = InventoryAuthority {
        drop_validated: false,
        ..authority()
    };
    assert_eq!(
        propose_stack_split(
            R::SplitToWorld {
                item: source.id,
                amount: 3
            },
            no_drop,
            prepared.clone(),
            view()
        ),
        Err(E::MissingGeometry)
    );
    assert_eq!(items[0], source);
}

#[test]
fn split_acquisition_uses_whole_source_burden_and_rejects_corpse_vendor_and_attuned_wield() {
    let mut source = item(0x80000010, ItemPlace::World);
    let mut prepared = StackSplitPreparation {
        fresh: InventoryItem {
            revision: 0,
            ..item(0x80000001, place(0))
        },
        source_stackable: true,
        source_stuck: false,
        source_vendor: false,
        destination_corpse: false,
    };
    let request = R::SplitToContainer {
        item: source.id,
        container: Id(1),
        placement: 0,
        amount: 1,
    };
    let narrow = [InventoryContainer {
        burden_limit: 19,
        ..container()
    }];
    assert_eq!(
        propose_stack_split(
            request,
            authority(),
            prepared.clone(),
            InventoryView {
                items: &[source.clone()],
                containers: &narrow
            }
        ),
        Err(E::Burden)
    );
    let containers = [container()];
    prepared.destination_corpse = true;
    assert_eq!(
        propose_stack_split(
            request,
            authority(),
            prepared.clone(),
            InventoryView {
                items: &[source.clone()],
                containers: &containers
            }
        ),
        Err(E::AccessDenied)
    );
    prepared.destination_corpse = false;
    prepared.source_vendor = true;
    assert_eq!(
        propose_stack_split(
            request,
            authority(),
            prepared.clone(),
            InventoryView {
                items: &[source.clone()],
                containers: &containers
            }
        ),
        Err(E::AccessDenied)
    );
    prepared.source_vendor = false;
    source.place = place(0);
    source.attuned = true;
    assert_eq!(
        propose_stack_split(
            R::SplitToWield {
                item: source.id,
                location: 1,
                amount: 1
            },
            authority(),
            prepared,
            InventoryView {
                items: &[source],
                containers: &containers
            }
        ),
        Err(E::Attuned)
    );
}
