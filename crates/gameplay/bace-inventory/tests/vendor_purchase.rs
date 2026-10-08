use bace_gameplay_api::InventoryRejection as Error;
use bace_inventory::{
    InventoryContainer, InventoryItem, InventoryView, ItemPlace, propose_vendor_purchase,
    select_vendor_currency_debits,
};
use bace_types::EntityId as Id;

const ACTOR: Id = Id(1);
const COIN: u32 = 273;

fn item(id: u32, template: u32, slot: u32, stack: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: Id(id),
        revision: 4,
        template,
        stack_key: u64::from(template),
        place: ItemPlace::Contained {
            container: ACTOR,
            slot,
            equipped: 0,
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
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: false,
    }
}

fn fresh(id: u32, template: u32) -> InventoryItem {
    let mut item = item(id, template, 99, 1);
    item.revision = 1;
    item.unit_burden = 5;
    item
}

fn root(slots: u32, burden_limit: u64) -> InventoryContainer {
    InventoryContainer {
        id: ACTOR,
        revision: 7,
        root_owner: Some(ACTOR),
        slots,
        pack_slots: 2,
        burden_limit,
        accessible: true,
        open: false,
        generation: 1,
    }
}

fn view<'a>(items: &'a [InventoryItem], containers: &'a [InventoryContainer]) -> InventoryView<'a> {
    InventoryView { items, containers }
}

#[test]
fn exact_multi_stack_debit_and_fresh_grants_share_one_proposal() {
    let items = [
        item(10, COIN, 0, 3),
        item(11, COIN, 1, 4),
        item(12, 900, 2, 1),
    ];
    let containers = [root(4, 100)];
    let prepared = [fresh(0x8000_0001, 1000), fresh(0x8000_0002, 1001)];
    let proposal = propose_vendor_purchase(
        ACTOR,
        COIN,
        5,
        &[(Id(10), 3), (Id(11), 2)],
        &prepared,
        view(&items, &containers),
    )
    .unwrap();
    let changed = |id| {
        proposal
            .changes
            .iter()
            .find(|change| change.after.id == Id(id))
            .unwrap()
    };
    assert_eq!(changed(10).after.place, ItemPlace::Removed);
    assert_eq!(changed(11).after.stack, 2);
    assert_eq!(changed(11).before.as_ref().unwrap().revision, 4);
    assert_eq!(changed(11).after.revision, 5);
    assert!(changed(0x8000_0001).before.is_none());
    assert!(changed(0x8000_0002).before.is_none());
    assert_eq!(proposal.actor_burden, 13);
    assert_eq!(
        proposal
            .participants
            .iter()
            .filter(|(id, _)| *id == ACTOR)
            .count(),
        1
    );
    assert!(proposal.participants.contains(&(Id(0x8000_0001), 0)));
    assert!(proposal.participants.contains(&(Id(0x8000_0002), 0)));
    let occupied: std::collections::BTreeSet<_> = proposal
        .changes
        .iter()
        .filter_map(|change| match change.after.place {
            ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            } if container == ACTOR => Some(slot),
            _ => None,
        })
        .collect();
    assert_eq!(occupied, [0, 1, 2, 3].into());
    assert_eq!(items[0].stack, 3);
    assert_eq!(
        prepared[0].place,
        ItemPlace::Contained {
            container: ACTOR,
            slot: 99,
            equipped: 0
        }
    );
}

#[test]
fn insufficient_or_wrong_currency_never_grants_an_item() {
    let items = [item(10, COIN, 0, 2), item(11, 500, 1, 3)];
    let containers = [root(4, 100)];
    let prepared = [fresh(0x8000_0001, 1000)];
    assert_eq!(
        propose_vendor_purchase(
            ACTOR,
            COIN,
            3,
            &[(Id(10), 2)],
            &prepared,
            view(&items, &containers)
        ),
        Err(Error::Requirements)
    );
    assert_eq!(
        propose_vendor_purchase(
            ACTOR,
            COIN,
            3,
            &[(Id(10), 2), (Id(11), 1)],
            &prepared,
            view(&items, &containers)
        ),
        Err(Error::OwnershipMismatch)
    );
    assert_eq!(items[0].stack, 2);
}

#[test]
fn capacity_and_burden_are_checked_after_payment_and_grant() {
    let items = [item(10, COIN, 0, 2)];
    let prepared = [fresh(0x8000_0001, 1000)];
    assert_eq!(
        propose_vendor_purchase(
            ACTOR,
            COIN,
            1,
            &[(Id(10), 1)],
            &prepared,
            view(&items, &[root(1, 100)])
        ),
        Err(Error::Capacity)
    );
    assert_eq!(
        propose_vendor_purchase(
            ACTOR,
            COIN,
            2,
            &[(Id(10), 2)],
            &prepared,
            view(&items, &[root(1, 4)])
        ),
        Err(Error::Burden)
    );
}

#[test]
fn duplicate_payment_or_fresh_identity_is_rejected() {
    let items = [item(10, COIN, 0, 3)];
    let containers = [root(4, 100)];
    let fresh_item = fresh(0x8000_0001, 1000);
    assert_eq!(
        propose_vendor_purchase(
            ACTOR,
            COIN,
            2,
            &[(Id(10), 1), (Id(10), 1)],
            std::slice::from_ref(&fresh_item),
            view(&items, &containers)
        ),
        Err(Error::InvalidCount)
    );
    assert_eq!(
        propose_vendor_purchase(
            ACTOR,
            COIN,
            1,
            &[(Id(10), 1)],
            &[fresh_item.clone(), fresh_item],
            view(&items, &containers)
        ),
        Err(Error::InvalidState)
    );
}

#[test]
fn currency_selector_uses_main_pack_before_ordered_side_containers() {
    let mut first_bag = item(20, 900, 0, 1);
    first_bag.is_container = true;
    let mut second_bag = item(21, 901, 2, 1);
    second_bag.is_container = true;
    let mut first_side = item(30, COIN, 0, 3);
    first_side.place = ItemPlace::Contained {
        container: Id(20),
        slot: 0,
        equipped: 0,
    };
    let mut second_side = item(31, COIN, 0, 4);
    second_side.place = ItemPlace::Contained {
        container: Id(21),
        slot: 0,
        equipped: 0,
    };
    let items = [
        first_bag,
        item(10, COIN, 1, 2),
        second_bag,
        first_side,
        second_side,
    ];
    let mut side_one = root(3, 100);
    side_one.id = Id(20);
    side_one.root_owner = None;
    let mut side_two = side_one;
    side_two.id = Id(21);
    let containers = [root(4, 100), side_one, side_two];
    assert_eq!(
        select_vendor_currency_debits(ACTOR, COIN, 6, view(&items, &containers)).unwrap(),
        [(Id(10), 2), (Id(30), 3), (Id(31), 1)]
    );
    assert_eq!(
        select_vendor_currency_debits(ACTOR, COIN, 10, view(&items, &containers)),
        Err(Error::Requirements)
    );
}
