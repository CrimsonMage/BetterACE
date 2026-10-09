//! The listing codec is checked against the pinned ACE ApproachVendor fixture in
//! bace-compat/tests/message_objects.rs; this checks canonical session ordering.
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer, InventoryProjection, SessionProjectionError};
use bace_types::{AccountId, EntityId};
use bace_wire::{
    CombatEffect, GameEventEnvelope, ObjectCodecLimits, SimpleGameEvent, VendorListing,
};
use std::collections::BTreeMap;

fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(1),
        account: AccountId(2),
        actor: EntityId(0x5000_0001),
    }
}

fn objects() -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 1024,
    }
}

fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 3,
        max_bytes: 8192,
        max_message_bytes: 4096,
        max_string_bytes: 1024,
    }
}

fn listing() -> VendorListing {
    VendorListing {
        vendor_id: 0x5000_0020,
        merchandise_types: 1,
        minimum_value: 0,
        maximum_value: u32::MAX,
        deal_magical: false,
        buy_price: 1.0,
        sell_price: 1.0,
        alternate_currency: None,
        items: Vec::new(),
    }
}

#[test]
fn private_pickup_listing_use_done_preflight_as_one_ordered_batch() {
    let binding = binding();
    let listing = listing();
    let steps = [
        InventoryProjection::Effect(CombatEffect::Sound {
            object_id: binding.actor.0,
            sound_id: 1,
            volume: 1.0,
        }),
        InventoryProjection::VendorListing(&listing),
        InventoryProjection::Simple(SimpleGameEvent::UseDone(0)),
    ];
    let mut events = EventSequencer::new(binding, 42);
    let mut items = BTreeMap::new();
    let mut short = limits();
    short.max_messages = 2;
    assert_eq!(
        events.project_inventory(binding, &steps, &mut items, objects(), short),
        Err(SessionProjectionError::Limit)
    );
    assert_eq!(events.next_sequence(), 42);

    let batch = events
        .project_inventory(binding, &steps, &mut items, objects(), limits())
        .unwrap();
    assert_eq!(batch.messages.len(), 3);
    assert_eq!(
        batch.messages.iter().map(|m| m.queue).collect::<Vec<_>>(),
        [10, 9, 9]
    );
    assert_eq!(
        batch.messages[1].bytes,
        listing
            .encode(binding.actor.0, 42, 1024, objects())
            .unwrap()
    );
    let listed = GameEventEnvelope::decode(&batch.messages[1].bytes, 4096).unwrap();
    let done = GameEventEnvelope::decode(&batch.messages[2].bytes, 4096).unwrap();
    assert_eq!((listed.sequence, done.sequence), (42, 43));
    assert_eq!(events.next_sequence(), 44);
}

#[test]
fn invalid_listing_keeps_session_sequence_and_reliable_batch_unpublished() {
    let binding = binding();
    let mut listing = listing();
    listing.items.push(bace_wire::VendorListingItem {
        quantity: -2,
        object_id: 1,
        game: bace_wire::ObjectGameData {
            name: "invalid".into(),
            class_id: 1,
            icon_id: 1,
            item_type: 1,
            description_flags: 0,
            options: Default::default(),
        },
    });
    let mut events = EventSequencer::new(binding, 9);
    assert!(
        events
            .project_inventory(
                binding,
                &[InventoryProjection::VendorListing(&listing)],
                &mut BTreeMap::new(),
                objects(),
                limits(),
            )
            .is_err()
    );
    assert_eq!(events.next_sequence(), 9);
}
