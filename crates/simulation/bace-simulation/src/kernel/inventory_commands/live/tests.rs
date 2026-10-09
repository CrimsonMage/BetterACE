use super::*;
use bace_inventory::{InventoryAuthority, InventoryContainer, InventoryItem, InventoryView};

const ACTOR: EntityId = EntityId(2);
const CHEST: EntityId = EntityId(0x8000_0020);
const ITEM: EntityId = EntityId(0x8000_0021);

fn container(id: EntityId, owner: Option<EntityId>, open: bool) -> InventoryContainer {
    InventoryContainer {
        id,
        revision: 1,
        root_owner: owner,
        slots: 8,
        pack_slots: 0,
        burden_limit: 1000,
        accessible: true,
        open,
        generation: 7,
    }
}

fn item() -> InventoryItem {
    InventoryItem {
        id: ITEM,
        structure: None,
        revision: 1,
        template: 100,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: CHEST,
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}

fn kernel(open: bool) -> Kernel {
    let mut kernel = Kernel::new(bace_world::World::default(), 16).unwrap();
    kernel
        .register_inventory_container(container(CHEST, None, open))
        .unwrap();
    kernel
        .register_inventory_container(container(ACTOR, Some(ACTOR), true))
        .unwrap();
    kernel.register_inventory_item(item()).unwrap();
    kernel
}

#[test]
fn live_source_view_uses_current_open_generation_and_proposal_revalidates_it() {
    let request = InventoryRequest::Move {
        item: ITEM,
        container: ACTOR,
        placement: 0,
    };
    let open_kernel = kernel(true);
    assert_eq!(
        open_kernel.inventory_live_source_view(ACTOR, request, Some(CHEST)),
        Ok(Some(7))
    );
    let items = [item()];
    let containers = [
        container(CHEST, None, true),
        container(ACTOR, Some(ACTOR), true),
    ];
    let mut authority = InventoryAuthority {
        actor: ACTOR,
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: false,
        source_view: Some(6),
        destination_view: None,
        new_item: None,
    };
    assert_eq!(
        bace_inventory::propose_inventory(
            request,
            authority,
            InventoryView {
                items: &items,
                containers: &containers,
            }
        ),
        Err(E::StaleView)
    );
    authority.source_view = open_kernel
        .inventory_live_source_view(ACTOR, request, Some(CHEST))
        .unwrap();
    assert!(
        bace_inventory::propose_inventory(
            request,
            authority,
            InventoryView {
                items: &items,
                containers: &containers,
            }
        )
        .is_ok()
    );

    let closed = kernel(false);
    assert_eq!(
        closed.inventory_live_source_view(ACTOR, request, Some(CHEST)),
        Err(E::AccessDenied)
    );
}
