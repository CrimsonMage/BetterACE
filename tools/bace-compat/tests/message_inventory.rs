//! Independent ACE inventory/trade input readers and event serializers.
use bace_wire::*;
use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap()
}
fn bytes(value: &Value) -> Vec<u8> {
    let value = value["bytes"].as_str().unwrap();
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
#[test]
fn all_nineteen_inventory_requests_match_pinned_handlers_and_preserve_signed_values() {
    for vector in fixture()["vectors"]["inventory"]["actions"]
        .as_array()
        .unwrap()
    {
        let source = bytes(vector);
        let envelope = GameActionEnvelope::decode(&source, 256).unwrap();
        let request = InventoryRequest::decode(envelope.action, envelope.payload, 256, 8).unwrap();
        let captured = match &request.action {
            InventoryAction::PutInContainer {
                item_id,
                container_id,
                placement,
            } => json!({"item_id":item_id,"container_id":container_id,"placement":placement}),
            InventoryAction::Wield { item_id, location } => {
                json!({"item_id":item_id,"location":location})
            }
            InventoryAction::Drop(object_id)
            | InventoryAction::Use(object_id)
            | InventoryAction::OpenTrade(object_id)
            | InventoryAction::StopViewing(object_id) => json!({"object_id":object_id}),
            InventoryAction::UseWithTarget {
                source_id,
                target_id,
            } => json!({"source_id":source_id,"target_id":target_id}),
            InventoryAction::Merge {
                source_id,
                target_id,
                amount,
            } => json!({"source_id":source_id,"target_id":target_id,"amount":amount}),
            InventoryAction::SplitToContainer {
                stack_id,
                container_id,
                placement,
                amount,
            } => {
                json!({"stack_id":stack_id,"container_id":container_id,"placement":placement,"amount":amount})
            }
            InventoryAction::SplitToWorld { stack_id, amount } => {
                json!({"stack_id":stack_id,"amount":amount})
            }
            InventoryAction::SplitToWield {
                stack_id,
                location,
                amount,
            } => json!({"stack_id":stack_id,"location":location,"amount":amount}),
            InventoryAction::Give {
                target_id,
                item_id,
                amount,
            } => json!({"target_id":target_id,"item_id":item_id,"amount":amount}),
            InventoryAction::Buy { vendor_id, items }
            | InventoryAction::Sell { vendor_id, items } => {
                json!({"vendor_id":vendor_id,"items":items.iter().map(|item|json!({"amount":item.amount,"object_id":item.object_id})).collect::<Vec<_>>()})
            }
            InventoryAction::AddToTrade { item_id, slot } => json!({"item_id":item_id,"slot":slot}),
            InventoryAction::AcceptTrade(value) => {
                json!({"partner_id":value.partner_id,"trade_stamp":value.trade_stamp,"status":value.status,"initiator_id":value.initiator_id,"initiator_accepts":value.initiator_accepts,"partner_accepts":value.partner_accepts})
            }
            InventoryAction::CloseTrade
            | InventoryAction::DeclineTrade
            | InventoryAction::ResetTrade => json!({"empty":true}),
        };
        assert_eq!(captured, vector["captured"]);
        assert_eq!(
            request.trailing_bytes as u64,
            vector["trailing_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            (source.len() - request.trailing_bytes) as u64,
            vector["consumed"].as_u64().unwrap()
        );
        let consumed = envelope.payload.len() - request.trailing_bytes;
        for length in 0..consumed {
            assert!(
                InventoryRequest::decode(envelope.action, &envelope.payload[..length], 256, 8)
                    .is_err()
            );
        }
    }
}
#[test]
fn fourteen_inventory_and_trade_events_match_official_serializers() {
    let fixture = fixture();
    let events = &fixture["vectors"]["inventory"]["events"];
    let items = [
        ContainerEntry {
            object_id: 0x80000003,
            container_type: 2,
        },
        ContainerEntry {
            object_id: 0x80000002,
            container_type: 1,
        },
        ContainerEntry {
            object_id: 0x80000001,
            container_type: 0,
        },
    ];
    for (name, event) in [
        (
            "put_world",
            InventoryEvent::PutInWorld {
                object_id: 0x80000001,
            },
        ),
        (
            "put_container",
            InventoryEvent::PutInContainer {
                object_id: 0x80000001,
                container_id: 0x50000010,
                placement: 3,
                container_type: 0,
            },
        ),
        (
            "wield",
            InventoryEvent::Wield {
                object_id: 0x80000001,
                location: 0x87654321,
            },
        ),
        (
            "view",
            InventoryEvent::ViewContents {
                container_id: 0x50000010,
                items: &items,
            },
        ),
        (
            "close_container",
            InventoryEvent::CloseContainer {
                container_id: 0x50000010,
            },
        ),
        (
            "save_failed",
            InventoryEvent::SaveFailed {
                item_id: 0x80000001,
                error: 0x36,
            },
        ),
        (
            "register_trade",
            InventoryEvent::RegisterTrade {
                initiator_id: 0x50000001,
                partner_id: 0x50000002,
            },
        ),
        (
            "add_trade",
            InventoryEvent::AddToTrade {
                object_id: 0x80000001,
                side: 2,
            },
        ),
        (
            "accept_trade",
            InventoryEvent::AcceptTrade { who: 0x50000002 },
        ),
        (
            "decline_trade",
            InventoryEvent::DeclineTrade { who: 0x50000002 },
        ),
        (
            "reset_trade",
            InventoryEvent::ResetTrade { who: 0x50000002 },
        ),
        ("close_trade", InventoryEvent::CloseTrade { reason: 0x51 }),
        ("clear_acceptance", InventoryEvent::ClearTradeAcceptance),
        (
            "trade_failure",
            InventoryEvent::TradeFailure {
                object_id: 0x80000001,
                reason: 0x36,
            },
        ),
    ] {
        assert_eq!(
            event.encode(0x50000001, 42, 8, 1024).unwrap(),
            bytes(&events[name]),
            "{name}"
        );
        assert_eq!(events[name]["group"], 9);
    }
}
