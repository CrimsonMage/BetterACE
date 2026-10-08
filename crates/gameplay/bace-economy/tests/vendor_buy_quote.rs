//! Source statements: pinned ACE Vendor.BuyItems_ValidateTransaction validates
//! positive ItemProfile amounts before GUID lookup; ItemProfileToWorldObjects
//! partitions against MaxStackSize; WorldObject.SetStackSize multiplies Int15.
use bace_economy::{
    VendorBuyQuoteError as E, VendorBuyRequest, VendorDefaultOffer, VendorStock, VendorStockItem,
    quote_default_buy,
};

fn stock() -> VendorStock {
    let mut stock = VendorStock::new(4).unwrap();
    stock.mark_lazy_loaded().unwrap();
    let proposal = stock
        .propose_lazy(VendorStockItem {
            id: 10,
            template: 100,
            stack: None,
            maximum_stack: Some(3),
        })
        .unwrap();
    stock.commit_lazy(proposal).unwrap();
    stock
}

fn offer(id: u32) -> VendorDefaultOffer {
    VendorDefaultOffer {
        stock_id: id,
        template: 100,
        value: 4,
        stack_unit_value: 4,
        maximum_stack: Some(3),
        promissory_note: false,
        vendor_service: false,
        unique: false,
    }
}

#[test]
fn source_order_duplicates_unknown_guid_and_stack_partition() {
    let quote = quote_default_buy(
        &stock(),
        &[
            VendorBuyRequest {
                stock_id: 99,
                amount: 5,
            },
            VendorBuyRequest {
                stock_id: 10,
                amount: 5,
            },
            VendorBuyRequest {
                stock_id: 10,
                amount: 1,
            },
        ],
        &[offer(10)],
        Some(0.5),
        8,
    )
    .unwrap();
    assert_eq!(quote.stock_revision, 2);
    assert_eq!(quote.lines.len(), 2);
    assert_eq!(quote.lines[0].stacks, [3, 2]);
    assert_eq!(quote.lines[0].cost, 10); // ceil(12*.5-.1)+ceil(8*.5-.1)
    assert_eq!(quote.lines[1].stacks, [1]);
    assert_eq!(quote.lines[1].cost, 2);
    assert_eq!(quote.total_cost, 12);
}

#[test]
fn invalid_unknown_amount_and_unsupported_effects_fail_before_quote() {
    let request = [VendorBuyRequest {
        stock_id: 99,
        amount: 0,
    }];
    assert_eq!(
        quote_default_buy(&stock(), &request, &[offer(10)], None, 8),
        Err(E::Invalid)
    );
    let request = [VendorBuyRequest {
        stock_id: 10,
        amount: 1,
    }];
    let mut service = offer(10);
    service.vendor_service = true;
    assert_eq!(
        quote_default_buy(&stock(), &request, &[service], None, 8),
        Err(E::Unsupported)
    );
    let mut unique = offer(10);
    unique.unique = true;
    assert_eq!(
        quote_default_buy(&stock(), &request, &[unique], None, 8),
        Err(E::Unsupported)
    );
    let mut wrong = offer(10);
    wrong.template = 101;
    assert_eq!(
        quote_default_buy(&stock(), &request, &[wrong], None, 8),
        Err(E::Invalid),
        "offer projection must match the accepted stock identity and template"
    );
}

#[test]
fn bounded_instances_and_checked_currency_total() {
    let request = [VendorBuyRequest {
        stock_id: 10,
        amount: i32::MAX,
    }];
    assert_eq!(
        quote_default_buy(&stock(), &request, &[offer(10)], None, 1024),
        Err(E::Capacity)
    );
    let request = [VendorBuyRequest {
        stock_id: 10,
        amount: 2,
    }];
    let mut huge = offer(10);
    huge.maximum_stack = None;
    huge.value = i32::MAX;
    assert_eq!(
        quote_default_buy(&stock(), &request, &[huge], Some(1.0), 2),
        Err(E::Overflow)
    );
}
