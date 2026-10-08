use super::*;
fn action(sequence: u32) -> PendingSocialAction {
    PendingSocialAction {
        context: ActionContext {
            session: SessionId(9),
            account: AccountId(1),
            actor: EntityId(0x50000001),
            sequence,
        },
        request: SocialRequest::AccountSquelch {
            enabled: true,
            name: "Offline".into(),
        },
    }
}
fn fixture() -> (SocialLookupService, Receiver<Work>, SyncSender<Completion>) {
    let (sender, work) = mpsc::sync_channel(1);
    let (completion, receiver) = mpsc::sync_channel(1);
    (
        SocialLookupService {
            sender,
            receiver,
            worker: std::thread::spawn(|| {}),
            stop: Arc::new(AtomicBool::new(false)),
            pending: BTreeMap::new(),
            capacity: 1,
            next: 0,
        },
        work,
        completion,
    )
}
#[test]
fn resolved_action_retains_session_barrier_and_account_identity_until_exact_enqueue() {
    let (mut service, work, completion) = fixture();
    service.begin(action(7)).unwrap();
    let job = work.recv().unwrap();
    assert!(service.pending_session(SessionId(9)));
    assert!(service.begin(action(8)).is_err());
    completion
        .send(Completion {
            correlation: job.correlation,
            result: Ok(Some(PlayerIdentity {
                object_id: 0x50000002,
                account_id: 22,
                name: "Offline".into(),
            })),
        })
        .unwrap();
    service.poll(1).unwrap();
    assert_eq!(service.flush(1, |command| Err(Box::new(command))), 0);
    assert!(service.pending_session(SessionId(9)));
    assert_eq!(
        service.flush(1, |command| {
            match command {
                Command::SocialResolved {
                    context,
                    request,
                    identity,
                } => {
                    assert_eq!(context.sequence, 7);
                    assert!(matches!(
                        request,
                        SocialRequest::AccountSquelch { enabled: true, .. }
                    ));
                    let identity = identity.unwrap();
                    assert_eq!(identity.character, EntityId(0x50000002));
                    assert_eq!(identity.account, AccountId(22));
                }
                _ => panic!("wrong command"),
            };
            Ok(())
        }),
        1
    );
    assert!(!service.has_pending());
    let (pending, result) = service.stop();
    assert!(pending.is_empty());
    result.unwrap();
}
#[test]
fn read_failure_retains_original_action_for_explicit_retry_or_shutdown_recovery() {
    let (mut service, work, completion) = fixture();
    service.begin(action(7)).unwrap();
    let first = work.recv().unwrap();
    completion
        .send(Completion {
            correlation: first.correlation,
            result: Err("read timeout".into()),
        })
        .unwrap();
    service.poll(1).unwrap();
    assert_eq!(service.failure(SessionId(9)), Some("read timeout"));
    assert_eq!(
        service.flush(1, |_| panic!("failed read cannot dispatch")),
        0
    );
    service.retry(SessionId(9)).unwrap();
    let retry = work.recv().unwrap();
    assert_eq!(retry.correlation, first.correlation);
    assert_eq!(retry.query, first.query);
    let (pending, result) = service.stop();
    result.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].context.sequence, 7);
}
