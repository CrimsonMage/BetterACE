use super::*;
use bace_gameplay_api::SessionId;
use bace_replication::ReplicationMessage;
use bace_types::AccountId;
fn recipient(id: u16, generation: u64) -> Recipient {
    Recipient {
        key: SessionKey { id, generation },
        binding: CharacterBinding {
            session: SessionId(u64::from(id)),
            account: AccountId(1),
            actor: EntityId(u32::from(id)),
        },
    }
}
fn work(count: usize) -> rewards::RewardObserverWork {
    rewards::RewardObserverWork {
        sequence: 5,
        actor: EntityId(7),
        messages: (0..count)
            .map(|n| ReplicationMessage {
                queue: 9,
                bytes: (n as u32).to_le_bytes().to_vec(),
            })
            .collect(),
        reset_visibility: false,
    }
}
fn fields(command: &NetworkCommand) -> (SessionKey, u64, Vec<(u16, Vec<u8>)>) {
    let NetworkCommand::SendReliableBatch {
        key,
        correlation,
        messages,
    } = command
    else {
        panic!("tracked command")
    };
    (*key, *correlation, messages.clone())
}
#[test]
fn full_preserves_exact_bytes_and_correlation_then_exact_receipt_advances_once() {
    let mut router = ObserverOutputRuntime::new();
    let work = work(5);
    router
        .begin(
            &work,
            vec![recipient(1, 4), recipient(2, 9)],
            None,
            20,
            2,
            8,
        )
        .unwrap();
    let (command, end) = router.batch(&work, 8).unwrap().unwrap();
    let original = fields(&command);
    assert_eq!(end, 2);
    router.pending.as_mut().unwrap().ready = Some((command, end)); // channel Full returns ownership
    let next = router.next;
    let (command, end) = router.batch(&work, 8).unwrap().unwrap();
    assert_eq!(fields(&command), original);
    assert_eq!(router.next, next);
    router.pending.as_mut().unwrap().inflight = Some(Inflight {
        correlation: original.1,
        end,
    });
    assert!(router.batch(&work, 8).unwrap().is_none());
    assert!(
        router
            .acknowledge(recipient(1, 5).key, original.1, true, 5)
            .is_err()
    );
    assert!(
        router
            .acknowledge(original.0, original.1 + 1, true, 5)
            .is_err()
    );
    assert_eq!(router.pending.as_ref().unwrap().offset, 0);
    router.acknowledge(original.0, original.1, true, 5).unwrap();
    assert_eq!(router.pending.as_ref().unwrap().offset, 2);
    assert!(router.acknowledge(original.0, original.1, true, 5).is_err());
    let (command, end) = router.batch(&work, 8).unwrap().unwrap();
    let f = fields(&command);
    assert_eq!(
        f.2,
        vec![
            (9, 2u32.to_le_bytes().to_vec()),
            (9, 3u32.to_le_bytes().to_vec())
        ]
    );
    router.pending.as_mut().unwrap().inflight = Some(Inflight {
        correlation: f.1,
        end,
    });
    assert!(router.acknowledge(f.0, f.1, false, 5).unwrap());
    assert_eq!(router.pending.as_ref().unwrap().recipient, 1);
    assert_eq!(router.pending.as_ref().unwrap().offset, 0);
    assert_eq!(
        fields(&router.batch(&work, 8).unwrap().unwrap().0).0,
        recipient(2, 9).key
    );
    assert!(router.holds(EntityId(7)));
}
#[test]
fn audience_and_message_limits_reject_before_any_fanout() {
    let mut router = ObserverOutputRuntime::new();
    assert!(
        router
            .begin(
                &work(1),
                vec![recipient(1, 1), recipient(1, 1)],
                None,
                0,
                2,
                8
            )
            .is_err()
    );
    assert!(!router.has_pending());
    let mut invalid = work(1);
    invalid.messages[0].bytes = vec![0; 9];
    assert!(
        router
            .begin(&invalid, vec![recipient(1, 1)], None, 0, 1, 8)
            .is_err()
    );
    assert!(!router.has_pending());
    assert_eq!(router.next, PREFIX);
    router.begin(&work(1), vec![], None, 0, 1, 8).unwrap();
    assert!(router.batch(&work(1), 8).unwrap().is_none());
}

#[test]
fn closed_generation_discards_only_its_unsent_retry_and_never_redirects_it() {
    let mut router = ObserverOutputRuntime::new();
    let work = work(2);
    router
        .begin(
            &work,
            vec![recipient(1, 4), recipient(2, 9)],
            None,
            20,
            2,
            8,
        )
        .unwrap();
    let (command, end) = router.batch(&work, 8).unwrap().unwrap();
    router.pending.as_mut().unwrap().ready = Some((command, end));
    router.skip_recipient().unwrap();
    let (command, end) = router.batch(&work, 8).unwrap().unwrap();
    let (key, correlation, messages) = fields(&command);
    assert_eq!(key, recipient(2, 9).key);
    assert_eq!(
        messages,
        vec![
            (9, 0u32.to_le_bytes().to_vec()),
            (9, 1u32.to_le_bytes().to_vec())
        ]
    );
    router.pending.as_mut().unwrap().inflight = Some(Inflight { correlation, end });
    assert!(
        router.skip_recipient().is_err(),
        "admitted bytes await exact result even after disconnect"
    );
    assert_eq!(router.pending.as_ref().unwrap().recipient, 1);
}
