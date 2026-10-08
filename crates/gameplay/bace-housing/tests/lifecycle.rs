use bace_housing::*;
use bace_types::EntityId as Id;
fn state() -> HousingState {
    HousingState {
        house: Id(0x70000001),
        revision: 1,
        owner: None,
        allegiance_monarch: None,
        generation: 1,
        purchased_at: 0,
        period_start: 0,
        rent_due: 0,
        interval_seconds: 30 * 86400,
        maintenance_free: false,
        open: false,
        storage_open: false,
        hooks_visible: true,
        guests: vec![],
        rent: vec![HousePayment {
            template: 273,
            required: 10,
            paid: 0,
        }],
    }
}
fn actor() -> HousingActor {
    HousingActor {
        actor: Id(0x50000001),
        account: 1,
        level: 100,
        monarch: true,
        allegiance_rank: 6,
        account_age_seconds: 30 * 86400,
        previous_purchase: 0,
        owns_house: false,
        in_range: true,
    }
}
fn rules() -> PurchaseRules {
    PurchaseRules {
        minimum_level: 20,
        requires_monarch: false,
        minimum_rank: 0,
        account_age_seconds: 15 * 86400,
        cooldown_seconds: 30 * 86400,
        apartment: false,
        buy: vec![HousePayment {
            template: 273,
            required: 100,
            paid: 0,
        }],
    }
}
fn coins(count: u32) -> PaymentItem {
    PaymentItem {
        item: Id(0x80000001),
        template: 273,
        revision: 1,
        count,
        currency_value: 1,
        trade_note: false,
    }
}
#[test]
fn purchase_and_partial_rent_are_proposals_with_exact_payment_counts() {
    let input = state();
    let purchase = propose_purchase(&input, actor(), &rules(), &[coins(150)], 100).unwrap();
    assert_eq!(purchase.payments[0].count, 100);
    assert!(input.owner.is_none());
    let paid = propose_rent(&purchase.after, actor().actor, &[coins(4)]).unwrap();
    assert_eq!(paid.after.rent[0].paid, 4);
    let paid = propose_rent(&paid.after, actor().actor, &[coins(6)]).unwrap();
    let due = propose_due_rent(&paid.after, paid.after.rent_due + 1, true, true)
        .unwrap()
        .unwrap();
    assert_eq!(due.after.owner, Some(actor().actor));
    assert_eq!(due.after.rent[0].paid, 0);
    assert_eq!(due.after.rent_due, 100 + 60 * 86400);
}
#[test]
fn eviction_retains_contents_by_omitting_deletions_and_invalidates_every_old_view() {
    let owned = propose_purchase(&state(), actor(), &rules(), &[coins(100)], 100)
        .unwrap()
        .after;
    let view = owned.open_view(actor().actor, true).unwrap();
    let evicted = propose_due_rent(&owned, owned.rent_due + 1, true, true)
        .unwrap()
        .unwrap();
    assert!(evicted.after.owner.is_none());
    assert!(!evicted.after.permits(actor().actor, true));
    assert!(evicted.after.validate_view(view).is_err());
    let next = HousingActor {
        actor: Id(0x50000002),
        ..actor()
    };
    let inherited = propose_purchase(
        &evicted.after,
        next,
        &rules(),
        &[coins(100)],
        owned.rent_due + 100,
    )
    .unwrap();
    assert!(inherited.after.permits(next.actor, true));
    assert!(!inherited.after.permits(actor().actor, true));
}
#[test]
fn rent_disabled_and_maintenance_free_advance_without_eviction_offline() {
    let owned = propose_purchase(&state(), actor(), &rules(), &[coins(100)], 100)
        .unwrap()
        .after;
    let advanced = propose_due_rent(&owned, 100 + 100 * 86400, false, false)
        .unwrap()
        .unwrap();
    assert!(advanced.after.owner.is_some());
    assert_eq!(advanced.after.rent_due, 100 + 120 * 86400);
    assert!(
        propose_due_rent(&owned, owned.rent_due, true, true)
            .unwrap()
            .is_none()
    );
}
#[test]
fn permission_revocation_fences_open_storage_and_due_failures_keep_deadlines() {
    let mut owned = propose_purchase(&state(), actor(), &rules(), &[coins(100)], 100)
        .unwrap()
        .after;
    owned = propose_permission(
        &owned,
        actor().actor,
        PermissionChange::Guest {
            player: Id(0x50000002),
            storage: true,
        },
    )
    .unwrap()
    .after;
    let view = owned.open_view(Id(0x50000002), true).unwrap();
    let denied = propose_permission(&owned, actor().actor, PermissionChange::ClearStorage).unwrap();
    assert!(denied.after.validate_view(view).is_err());
    let mut queue = RentScheduler::new(2).unwrap();
    queue.schedule(owned.house, 50).unwrap();
    let ticket = queue.next_due(51).unwrap();
    assert_eq!(queue.next_due(1000), Some(ticket));
    assert!(queue.acknowledge(ticket));
    assert!(queue.next_due(1000).is_none());
}
