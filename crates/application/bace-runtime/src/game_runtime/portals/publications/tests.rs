use super::*;

#[test]
fn only_the_exact_reliable_peer_receipt_releases_publication_fence() {
    let key = SessionKey {
        id: 3,
        generation: 8,
    };
    let mut p = PortalPublications::default();
    let messages = vec![(10, vec![0x51, 0xf7, 0, 0, 1, 0])];
    let command = p.retain(EntityId(5), key, messages.clone(), None).unwrap();
    let NetworkCommand::SendReliableBatch {
        correlation,
        messages: output,
        ..
    } = command
    else {
        panic!("tracked batch required");
    };
    assert_eq!(output, messages);
    // Taking the command into an adapter queue does not consume retained state.
    assert_eq!(p.pending[&correlation]._messages, messages);
    assert!(
        p.resolve(
            SessionKey {
                generation: 9,
                ..key
            },
            correlation
        )
        .is_err()
    );
    assert!(p.resolve(key, correlation + 1).is_err());
    assert_eq!(p.pending.len(), 1);
    p.resolve(key, correlation).unwrap();
    assert!(p.pending.is_empty());
    assert!(p.resolve(key, correlation).is_err());
}

#[test]
fn pressure_does_not_consume_publication_identity_or_replace_exact_bytes() {
    let key = SessionKey {
        id: 3,
        generation: 8,
    };
    let mut p = PortalPublications::default();
    for value in 0..CAPACITY {
        p.retain(EntityId(5), key, vec![(10, vec![value as u8])], None)
            .unwrap();
    }
    assert!(!p.has_room());
    assert!(
        p.retain(EntityId(5), key, vec![(10, vec![255])], None)
            .is_err()
    );
    assert_eq!(p.next, CAPACITY as u64);
    assert_eq!(p.pending[&(PREFIX | 1)]._messages, vec![(10, vec![0])]);
    p.resolve(key, PREFIX | 1).unwrap();
    assert!(p.has_room());
}

#[test]
fn materialization_identity_waits_for_the_exact_reliable_peer_receipt() {
    let key = SessionKey {
        id: 8,
        generation: 11,
    };
    let actor = EntityId(0x5000_0101);
    let mut p = PortalPublications::default();
    let NetworkCommand::SendReliableBatch { correlation, .. } = p
        .retain(actor, key, vec![(10, vec![1])], Some((77, 3)))
        .unwrap()
    else {
        panic!("reliable batch")
    };
    assert!(
        p.resolve(
            SessionKey {
                generation: 12,
                ..key
            },
            correlation
        )
        .is_err()
    );
    assert_eq!(p.pending.len(), 1);
    assert_eq!(p.resolve(key, correlation).unwrap(), Some((actor, 77, 3)));
    assert!(p.resolve(key, correlation).is_err());
}
