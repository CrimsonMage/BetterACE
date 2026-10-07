use bace_wire::opcode::GameActionType as Op;
use bace_wire::*;
#[test]
fn vendor_item_counts_and_payload_budgets_are_bounded_before_allocation() {
    let mut writer = Writer::new();
    writer.u32(1);
    writer.u32(u32::MAX);
    let bytes = writer.into_bytes();
    for opcode in [Op::Buy, Op::Sell] {
        assert_eq!(
            InventoryRequest::decode(opcode, &bytes, 8, 128),
            Err(WireError::LimitExceeded)
        );
        assert_eq!(
            InventoryRequest::decode(opcode, &bytes, 8, usize::MAX),
            Err(WireError::Truncated)
        );
    }
    assert_eq!(
        InventoryRequest::decode(Op::DropItem, &[0; 4], 3, 0),
        Err(WireError::LimitExceeded)
    );
    assert!(InventoryRequest::decode(Op::Talk, &[], 0, 0).is_err());
}
#[test]
fn quantities_are_signed_untrusted_proposals_and_currency_suffix_is_not_interpreted() {
    let mut writer = Writer::new();
    for word in [1, 1, u32::MAX, 2, 0x12345678] {
        writer.u32(word);
    }
    let bytes = writer.into_bytes();
    let request = InventoryRequest::decode(Op::Buy, &bytes, 20, 1).unwrap();
    assert_eq!(request.trailing_bytes, 4);
    assert_eq!(
        request.action,
        InventoryAction::Buy {
            vendor_id: 1,
            items: vec![VendorItemRequest {
                amount: -1,
                object_id: 2
            }]
        }
    );
    for length in 0..28 {
        assert!(InventoryRequest::decode(Op::AcceptTrade, &vec![0; length], 32, 0).is_err());
    }
}
#[test]
fn container_output_and_fixed_trade_events_respect_message_caps() {
    let items = [ContainerEntry {
        object_id: 1,
        container_type: 0,
    }; 2];
    let event = InventoryEvent::ViewContents {
        container_id: 3,
        items: &items,
    };
    assert_eq!(event.encode(1, 2, 1, 1024), Err(WireError::LimitExceeded));
    let valid = event.encode(1, 2, 2, 1024).unwrap();
    assert_eq!(
        event.encode(1, 2, 2, valid.len() - 1),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        InventoryEvent::ClearTradeAcceptance.encode(1, 2, 0, 15),
        Err(WireError::LimitExceeded)
    );
}

#[test]
fn vendor_stock_masks_do_not_silently_truncate_invalid_quantities_or_overrun_limits() {
    let bounds = ObjectCodecLimits {
        max_message_bytes: 1024,
        max_model_entries: 0,
        max_children: 0,
        max_restrictions: 0,
        max_motion_commands: 0,
        max_string_bytes: 16,
    };
    let mut vendor = VendorListing {
        vendor_id: 1,
        merchandise_types: 2,
        minimum_value: 0,
        maximum_value: 100,
        deal_magical: false,
        buy_price: 0.5,
        sell_price: 1.0,
        alternate_currency: None,
        items: vec![VendorListingItem {
            quantity: -1,
            object_id: 2,
            game: ObjectGameData {
                name: "Synthetic".into(),
                class_id: 1,
                icon_id: 0x06000001,
                item_type: 1,
                description_flags: 0,
                options: ObjectGameOptions::default(),
            },
        }],
    };
    assert!(vendor.encode(1, 2, 1, bounds).is_ok());
    assert_eq!(
        vendor.encode(1, 2, 0, bounds),
        Err(WireError::LimitExceeded)
    );
    for quantity in [-2, 0x01000000, i32::MAX] {
        vendor.items[0].quantity = quantity;
        assert_eq!(
            vendor.encode(1, 2, 1, bounds),
            Err(WireError::InvalidEncoding)
        );
    }
    vendor.items[0].quantity = 5;
    let mut short = bounds;
    short.max_message_bytes = 16;
    assert_eq!(vendor.encode(1, 2, 1, short), Err(WireError::LimitExceeded));
    vendor.alternate_currency = Some(VendorCurrency {
        class_id: 3,
        amount: 1,
        plural_name: "x".repeat(17),
    });
    assert_eq!(
        vendor.encode(1, 2, 1, bounds),
        Err(WireError::LimitExceeded)
    );
}
