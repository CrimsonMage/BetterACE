use super::*;
#[tokio::test]
async fn exact_program_acknowledgment_precedes_cast_and_rejection_recaptures() {
    let (_cluster, _directory, mut runtime) = crate::game_runtime::tests::fixture::fixture().await;
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
        sequence: 5,
    };
    let retained = || Pending {
        key: SessionKey {
            id: 1,
            generation: 7,
        },
        context,
        token: 9,
        phase: Phase::Native(Box::new(NativePending {
            request: StaffRequest::Line("castspell 100".into()),
            command: Box::new(StaffCommand {
                token: 9,
                action: bace_gameplay_api::staff::StaffAction::CastSpell {
                    context,
                    target: None,
                    spell: 100,
                    sudo: false,
                },
            }),
            phase: NativePhase::Awaiting,
        })),
    };
    runtime.staff.pending = Some(retained());
    assert!(
        runtime
            .accept_staff_native_outcome(&StaffEvent::Outcome {
                token: 10,
                actor: Some(context.actor),
                result: Ok(())
            })
            .is_err()
    );
    assert!(matches!(
        runtime.staff.pending.as_ref().unwrap().phase,
        Phase::Native(_)
    ));
    assert!(
        runtime
            .accept_staff_native_outcome(&StaffEvent::Outcome {
                token: 9,
                actor: Some(context.actor),
                result: Ok(())
            })
            .unwrap()
    );
    let Phase::Command(command) = &runtime.staff.pending.as_ref().unwrap().phase else {
        panic!("cast still pending owner execution")
    };
    assert_eq!(command.token, 9);
    assert!(
        matches!(command.action,bace_gameplay_api::staff::StaffAction::CastSpell{context:actual,spell:100,..} if actual==context)
    );
    runtime.staff.pending = Some(retained());
    assert!(
        runtime
            .accept_staff_native_outcome(&StaffEvent::Outcome {
                token: 9,
                actor: Some(context.actor),
                result: Err(bace_gameplay_api::staff::StaffError::Stale)
            })
            .unwrap()
    );
    assert!(matches!(
        runtime.staff.pending.as_ref().unwrap().phase,
        Phase::Capture(_)
    ));
    assert!(runtime.staff.failure.is_none());
    runtime.staff.pending = Some(retained());
    assert!(
        runtime
            .accept_staff_native_outcome(&StaffEvent::Outcome {
                token: 9,
                actor: Some(context.actor),
                result: Err(bace_gameplay_api::staff::StaffError::Busy)
            })
            .unwrap()
    );
    assert!(matches!(
        runtime.staff.pending.as_ref().unwrap().phase,
        Phase::Capture(_)
    ));
    assert!(runtime.staff.failure.is_some());
    // No native program/cast was submitted in this isolated receipt regression.
    runtime.staff.pending = None;
    runtime.staff.failure = None;
    runtime.quiesce(Duration::ZERO).unwrap();
}
