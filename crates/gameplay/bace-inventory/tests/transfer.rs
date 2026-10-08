use bace_inventory::{ContainerState, ItemState, TransferError, propose_transfer};
#[test]
fn proposals_fence_ownership_revision_and_capacity() {
    let item = ItemState {
        item: 1,
        owner: 2,
        revision: 3,
        stack: 5,
        unit_burden: 10,
        attuned: false,
    };
    let container = ContainerState {
        owner: 4,
        revision: 8,
        slots_used: 0,
        slots_max: 1,
        burden: 50,
        burden_max: 100,
    };
    let proposal = propose_transfer(item, 2, container, true).unwrap();
    assert_eq!(proposal.next_item_revision, 4);
    assert_eq!(proposal.next_container_revision, 9);
    assert_eq!(proposal.transferred_burden, 50);
    assert_eq!(item.owner, 2);
    assert_eq!(
        propose_transfer(item, 5, container, true),
        Err(TransferError::WrongOwner)
    );
    assert_eq!(
        propose_transfer(
            ItemState {
                attuned: true,
                ..item
            },
            2,
            container,
            true
        ),
        Err(TransferError::Attuned)
    );
    assert_eq!(
        propose_transfer(
            item,
            2,
            ContainerState {
                burden: 51,
                ..container
            },
            true
        ),
        Err(TransferError::Burden)
    );
    assert_eq!(
        propose_transfer(
            item,
            2,
            ContainerState {
                slots_used: 1,
                ..container
            },
            true
        ),
        Err(TransferError::Full)
    );
}
