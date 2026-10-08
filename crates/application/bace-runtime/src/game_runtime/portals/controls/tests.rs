use super::*;
fn ready(actor: u32) -> PortalResolution {
    PortalResolution::ClientReady {
        context: bace_gameplay_api::ActionContext {
            actor: EntityId(actor),
            account: bace_types::AccountId(u64::from(actor)),
            session: bace_gameplay_api::SessionId(u64::from(actor)),
            sequence: 107,
        },
        operation: 0,
        epoch: 0,
    }
}
#[test]
fn rejected_client_ready_cannot_poison_another_players_portal_lane() {
    let mut runtime = PortalRuntime::new();
    let first = ready(1);
    let second = ready(2);
    runtime.controls.insert(8, first.clone());
    runtime.controls.insert(9, second.clone());
    let failure = runtime
        .accept_control_outcome(PortalResolutionOutcome {
            correlation: 8,
            resolution: first,
            result: Err(bace_interactions::PortalError::Invalid),
        })
        .unwrap();
    assert_eq!(failure.unwrap().0.actor, EntityId(1));
    assert!(runtime.unmatched_resolution.is_none());
    assert!(!runtime.controls.contains_key(&8));
    assert_eq!(runtime.controls.get(&9), Some(&second));
    assert!(
        runtime
            .accept_control_outcome(PortalResolutionOutcome {
                correlation: 9,
                resolution: second,
                result: Ok(())
            })
            .unwrap()
            .is_none()
    );
    assert!(runtime.controls.is_empty());
}
#[test]
fn transient_ready_pressure_retains_exact_action_and_correlation() {
    let mut runtime = PortalRuntime::new();
    let action = ready(1);
    runtime.controls.insert(8, action.clone());
    runtime
        .accept_control_outcome(PortalResolutionOutcome {
            correlation: 8,
            resolution: action.clone(),
            result: Err(bace_interactions::PortalError::Capacity),
        })
        .unwrap();
    assert_eq!(runtime.controls.get(&8), Some(&action));
    assert!(runtime.retry_controls.contains(&8));
    assert!(runtime.unmatched_resolution.is_none());
}
#[test]
fn login_complete_uses_reliable_sequence_after_mixed_action_traffic() {
    let binding = bace_gameplay_api::CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(1),
    };
    for inner in [0, 1, 108, u32::MAX] {
        let bytes = [0xf7b1u32, inner, 0xa1].map(u32::to_le_bytes).concat();
        let request = bace_wire::WorldControlRequest::decode(&bytes, 1024).unwrap();
        let context = ready_context(binding, 107, request).unwrap();
        assert_eq!(context.sequence, 107);
        assert_eq!(
            (context.actor, context.account, context.session),
            (binding.actor, binding.account, binding.session)
        );
    }
}
