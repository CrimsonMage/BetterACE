use bace_gameplay_api::{
    CharacterBinding, SessionId,
    visibility::{VisibilityCandidate, VisibilitySnapshot},
};
use bace_replication::{SpatialVisibility, SpatialVisibilityError};
use bace_types::{AccountId, EntityId};
fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
    }
}
fn snapshot(tick: u64, values: &[(u32, f32)]) -> VisibilitySnapshot {
    VisibilitySnapshot {
        binding: binding(),
        observer_epoch: 0,
        tick,
        candidates: values
            .iter()
            .map(|&(id, d)| VisibilityCandidate {
                entity: EntityId(id),
                distance_squared: d,
            })
            .collect(),
    }
}
fn commit(v: &mut SpatialVisibility, s: VisibilitySnapshot) {
    let ticket = v.stage(s).unwrap().ticket;
    v.commit(ticket).unwrap();
}
#[test]
fn create_and_remove_knowledge_wait_for_exact_output_receipts() {
    let mut v = SpatialVisibility::new(binding(), 8).unwrap();
    let ticket = v.stage(snapshot(0, &[(2, 0.0)])).unwrap().ticket;
    assert!(!v.knows(EntityId(2)));
    assert!(matches!(
        v.stage(snapshot(1, &[])),
        Err((SpatialVisibilityError::Busy, _))
    ));
    assert_eq!(
        v.commit(ticket + 1),
        Err(SpatialVisibilityError::InvalidTicket)
    );
    assert!(!v.knows(EntityId(2)));
    let original = v.commit(ticket).unwrap().unwrap();
    assert_eq!(original.candidates.len(), 1);
    assert!(v.knows(EntityId(2)));
    let ticket = v.stage_retirement(EntityId(2), 1).unwrap().ticket;
    assert!(v.knows(EntityId(2)));
    assert_eq!(v.pending().unwrap().removes, vec![EntityId(2)]);
    v.commit(ticket).unwrap();
    assert!(!v.knows(EntityId(2)));
    assert_eq!(v.commit(ticket), Err(SpatialVisibilityError::InvalidTicket));
}
#[test]
fn known_reentry_bypasses_initial_clamp_and_cancels_old_forget_ticket() {
    let mut v = SpatialVisibility::new(binding(), 8).unwrap();
    let limit = 112.5f32 * 112.5;
    commit(
        &mut v,
        snapshot(0, &[(2, limit), (3, f32::from_bits(limit.to_bits() + 1))]),
    );
    assert!(v.knows(EntityId(2)));
    assert!(!v.knows(EntityId(3)));
    commit(&mut v, snapshot(1, &[]));
    assert!(!v.visible(EntityId(2)));
    commit(&mut v, snapshot(750, &[(2, 50000.0)]));
    assert!(v.visible(EntityId(2)));
    let delta = v.stage(snapshot(751, &[(2, 50000.0)])).unwrap();
    assert!(delta.removes.is_empty());
    assert!(delta.creates.is_empty());
}
#[test]
fn repeated_occlusion_does_not_postpone_expiry_and_deadline_reentry_is_new() {
    let mut v = SpatialVisibility::new(binding(), 8).unwrap();
    commit(&mut v, snapshot(0, &[(2, 0.0)]));
    commit(&mut v, snapshot(1, &[]));
    commit(&mut v, snapshot(300, &[]));
    let delta = v.stage(snapshot(751, &[(2, 0.0)])).unwrap();
    assert_eq!(delta.removes, vec![EntityId(2)]);
    assert_eq!(delta.creates, vec![EntityId(2)]);
    let ticket = delta.ticket;
    v.commit(ticket).unwrap();
    assert!(v.visible(EntityId(2)));
}
#[test]
fn binding_epoch_time_capacity_and_malformed_snapshots_leave_knowledge_unchanged() {
    let mut v = SpatialVisibility::new(binding(), 1).unwrap();
    commit(&mut v, snapshot(10, &[(2, 0.0)]));
    let mut wrong = snapshot(11, &[]);
    wrong.binding.session = SessionId(2);
    assert!(v.stage(wrong).is_err());
    assert!(v.stage(snapshot(9, &[])).is_err());
    assert!(v.stage(snapshot(11, &[(2, f32::NAN)])).is_err());
    assert!(
        v.stage(snapshot(11, &[(3, 0.0)])).is_err(),
        "old known object still consumes capacity during grace"
    );
    assert!(v.knows(EntityId(2)));
    assert!(v.visible(EntityId(2)));
    assert!(v.pending().is_none());
}

#[test]
fn independent_original_ace_clamp_and_destruction_vectors() {
    let mut count = 0;
    for row in include_str!("fixtures/visibility.csv")
        .lines()
        .filter(|r| !r.starts_with('#'))
    {
        let fields: Vec<_> = row.split(',').collect();
        let mut v = SpatialVisibility::new(binding(), 8).unwrap();
        if fields[0] == "clamp" {
            let known = fields[1] == "1";
            let distance = f32::from_bits(u32::from_str_radix(fields[2], 16).unwrap());
            if known {
                commit(&mut v, snapshot(0, &[(2, 0.0)]));
                commit(&mut v, snapshot(0, &[]));
            }
            let delta = v.stage(snapshot(1, &[(2, distance)])).unwrap();
            let creates = delta.creates.len();
            let ticket = delta.ticket;
            v.commit(ticket).unwrap();
            assert_eq!(v.visible(EntityId(2)), fields[3] == "1", "{row}");
            assert_eq!(creates, usize::from(!known && fields[3] == "1"));
        } else {
            commit(&mut v, snapshot(0, &[(2, 0.0)]));
            commit(&mut v, snapshot(0, &[]));
            if fields[1] == "1" {
                commit(&mut v, snapshot(300, &[]));
            }
            let tick = fields[2].parse().unwrap();
            let distance = f32::from_bits(u32::from_str_radix(fields[3], 16).unwrap());
            let delta = v.stage(snapshot(tick, &[(2, distance)])).unwrap();
            assert_eq!(
                delta.removes.len(),
                fields[4].parse::<usize>().unwrap(),
                "{row}"
            );
            assert_eq!(
                delta.creates.len(),
                fields[5].parse::<usize>().unwrap(),
                "{row}"
            );
            let ticket = delta.ticket;
            v.commit(ticket).unwrap();
            assert_eq!(v.visible(EntityId(2)), fields[6] == "1", "{row}");
        }
        count += 1;
    }
    assert_eq!(count, 20);
}

#[test]
fn accepted_epoch_wrap_preserves_knowledge_and_rejects_old_generation() {
    let mut v = SpatialVisibility::new(binding(), 2).unwrap();
    let mut first = snapshot(1, &[(2, 0.0)]);
    first.observer_epoch = u16::MAX;
    commit(&mut v, first);
    commit(&mut v, snapshot(2, &[(2, 0.0)]));
    assert!(v.knows(EntityId(2)));
    let mut stale = snapshot(3, &[]);
    stale.observer_epoch = u16::MAX;
    assert!(matches!(
        v.stage(stale),
        Err((SpatialVisibilityError::StaleSnapshot, _))
    ));
}
