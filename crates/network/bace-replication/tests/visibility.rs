use bace_replication::*;
use bace_types::EntityId;
#[test]
fn recall_at_forget_deadline_cannot_erase_renewed_visibility() {
    let mut view = Visibility::new(2).unwrap();
    let player = EntityId(7);
    assert_eq!(view.observe(player, 0).unwrap(), VisibilityChange::Create);
    let old = view.forget_later(player, 0, 25_000).unwrap().unwrap();
    assert_eq!(
        view.observe(player, 25_000).unwrap(),
        VisibilityChange::Retained
    );
    assert!(!view.commit_forget(old, 25_000).unwrap());
    assert!(view.knows(player));
    let next = view.forget_later(player, 25_001, 25_000).unwrap().unwrap();
    assert_eq!(
        view.forget_later(player, 30_000, 25_000).unwrap(),
        Some(next)
    );
    assert!(view.commit_forget(next, 50_001).unwrap());
    assert_eq!(
        view.observe(player, 50_001).unwrap(),
        VisibilityChange::Create
    );
    assert!(!view.commit_forget(next, 50_002).unwrap());
    assert!(view.knows(player));
}
#[test]
fn independent_observers_keep_mutual_knowledge_on_reentry() {
    let mut a = Visibility::new(1).unwrap();
    let mut b = Visibility::new(1).unwrap();
    a.observe(EntityId(2), 0).unwrap();
    b.observe(EntityId(1), 0).unwrap();
    let old = a.forget_later(EntityId(2), 0, 25_000).unwrap().unwrap();
    b.forget_later(EntityId(1), 0, 25_000).unwrap();
    a.observe(EntityId(2), 24_999).unwrap();
    b.observe(EntityId(1), 24_999).unwrap();
    assert!(!a.commit_forget(old, 25_000).unwrap());
    assert!(a.knows(EntityId(2)) && b.knows(EntityId(1)));
    assert_eq!(
        a.observe(EntityId(3), 25_000),
        Err(ReplicationError::Capacity)
    );
}
#[test]
fn counters_do_not_alias_high_property_ids_and_bound_growth() {
    let mut seq = Sequences::new(2).unwrap();
    assert_eq!(seq.advance(SequenceKind::PropertyInt, 0x10000).unwrap(), 0);
    assert_eq!(seq.advance(SequenceKind::PropertyInt64, 0).unwrap(), 0);
    assert_eq!(seq.advance(SequenceKind::PropertyInt64, 0).unwrap(), 1);
    assert_eq!(
        seq.advance(SequenceKind::PropertyBool, 0),
        Err(ReplicationError::Capacity)
    );
}
