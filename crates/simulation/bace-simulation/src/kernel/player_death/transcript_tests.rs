//! Committed owner event order without a game client or native DAT assets.
use crate::{PlayerDeathEvent, PlayerDeathReceipt};
use bace_inventory::ItemPlace;

#[test]
fn committed_death_emits_started_corpse_respawn_with_frozen_identity_and_vitals() {
    let mut kernel = super::tests::fixture();
    let prepared = super::tests::prepare(&mut kernel);
    kernel
        .prepare_player_death(prepared)
        .map_err(|error| error.0)
        .unwrap();
    let ticket = kernel.take_player_death_proposal().unwrap();
    let receipt = PlayerDeathReceipt {
        operation: ticket.operation,
        actor: ticket.actor,
        after_revision: ticket.after_revision,
        inventory: crate::InventoryReceipt {
            operation: ticket.inventory.operation,
            revisions: ticket
                .inventory
                .proposal
                .changes
                .iter()
                .map(|change| (change.after.id, change.after.revision))
                .collect(),
        },
    };
    assert!(kernel.take_player_death_event().is_none());
    kernel
        .confirm_player_death_committed_at(&receipt, 100_000)
        .unwrap();
    let Some(PlayerDeathEvent::Started {
        operation,
        actor,
        num_deaths,
        accepted,
        ..
    }) = kernel.take_player_death_event()
    else {
        panic!("committed death start");
    };
    assert_eq!(
        (operation, actor, num_deaths),
        (ticket.operation, ticket.actor, 1)
    );
    if let Ok(view) = accepted {
        assert_eq!(view.entity, ticket.actor);
    }
    assert_eq!(
        kernel
            .inventory_item(bace_types::EntityId(20))
            .unwrap()
            .place,
        ItemPlace::Contained {
            container: ticket.actor,
            slot: 0,
            equipped: 0,
        },
        "inventory remains held until the scheduled corpse transition"
    );
    for _ in 0..31 {
        kernel.step().unwrap();
    }
    let Some(PlayerDeathEvent::Corpse {
        operation,
        actor,
        corpse,
        accepted_corpse,
        accepted_player,
    }) = kernel.take_player_death_event()
    else {
        panic!("corpse creation after accepted death timing");
    };
    assert_eq!(
        (operation, actor, corpse),
        (ticket.operation, ticket.actor, ticket.corpse)
    );
    assert_eq!(kernel.world.corpse(corpse).unwrap().operation, operation);
    assert!(
        matches!(kernel.inventory_item(bace_types::EntityId(20)).unwrap().place,
        ItemPlace::Contained { container, .. } if container == corpse)
    );
    assert_eq!(accepted_corpse.unwrap().entity, corpse);
    assert_eq!(accepted_player.unwrap().entity, actor);
    for _ in 0..90 {
        kernel.step().unwrap();
    }
    let Some(PlayerDeathEvent::Respawned {
        operation,
        actor,
        vitals,
        vital_revision,
        accepted,
        ..
    }) = kernel.take_player_death_event()
    else {
        panic!("respawn after corpse presentation stage");
    };
    assert_eq!((operation, actor), (ticket.operation, ticket.actor));
    assert_eq!(vitals, [71; 3]);
    assert!(vital_revision > 0);
    assert_eq!(accepted.unwrap().entity, actor);
    assert!(!kernel.player_death_pending(actor));
}
