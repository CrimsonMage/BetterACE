use super::*;
fn apply(kernel: &mut Kernel, action: MagicResourceAction) -> Result<MagicResourceResult, E> {
    kernel
        .handle_magic_resource(MagicResourceCommand {
            correlation: 1,
            binding: None,
            action,
        })
        .unwrap();
    let outcome = kernel.magic_resources.outcomes.pop_front().unwrap();
    Arc::try_unwrap(outcome).ok().unwrap().result
}
#[test]
fn projectile_and_portal_identity_batches_are_atomic_across_owners() {
    let mut kernel = crate::kernel::magic_components_fixture::component_kernel(64);
    let first = EntityId(0x80000001);
    let second = EntityId(0x80000002);
    let third = EntityId(0x80000003);
    assert!(matches!(
        apply(
            &mut kernel,
            MagicResourceAction::SupplyProjectileIds(vec![first, second])
        ),
        Ok(MagicResourceResult::ProjectileIds { remaining: 2, .. })
    ));
    assert_eq!(
        apply(
            &mut kernel,
            MagicResourceAction::SupplyPortalIds(vec![first, third])
        )
        .err(),
        Some(E::InvalidState)
    );
    assert!(matches!(
        apply(
            &mut kernel,
            MagicResourceAction::SupplyPortalIds(Vec::new())
        ),
        Ok(MagicResourceResult::PortalIds { remaining: 0, .. })
    ));
    assert!(
        apply(
            &mut kernel,
            MagicResourceAction::SupplyPortalIds(vec![third])
        )
        .is_ok()
    );
    assert_eq!(
        apply(
            &mut kernel,
            MagicResourceAction::SupplyProjectileIds(vec![
                EntityId(0x80000004),
                EntityId(0x80000004)
            ])
        )
        .err(),
        Some(E::InvalidState)
    );
    assert!(matches!(
        apply(
            &mut kernel,
            MagicResourceAction::SupplyProjectileIds(Vec::new())
        ),
        Ok(MagicResourceResult::ProjectileIds { remaining: 2, .. })
    ));
    assert_eq!(
        apply(
            &mut kernel,
            MagicResourceAction::SupplyProjectileIds(vec![EntityId(1)])
        )
        .err(),
        Some(E::InvalidState)
    );
}
