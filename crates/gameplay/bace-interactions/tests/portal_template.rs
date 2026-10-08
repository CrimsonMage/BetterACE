use bace_interactions::*;
#[test]
fn referenced_definition_needs_no_world_identity_and_instantiation_requires_real_identity_and_pose()
{
    let position = PortalPosition {
        cell: 0x12340001,
        origin: [1., 2., 3.],
        rotation: [1., 0., 0., 0.],
    };
    let definition = PortalTemplate {
        template: 900,
        original_template: None,
        kind: PortalKind::Portal,
        destination: Some(position),
        minimum_level: 5,
        maximum_level: 100,
        restrictions: 1,
        no_tie: false,
        ignore_pk_timer: false,
        account_requirement: 0,
    };
    definition.validate().unwrap();
    assert!(definition.instantiate(0, position).is_err());
    let mut invalid = position;
    invalid.cell = 0;
    assert!(definition.instantiate(42, invalid).is_err());
    let anchor = definition.instantiate(42, position).unwrap();
    assert_eq!(anchor.entity, 42);
    assert_eq!(anchor.definition(), definition);
    let access = PortalAccess {
        level: 4,
        pk_status: 2,
        pk_recent: false,
        olthoi: false,
        vitae: false,
        account_15_days: true,
        entitlement: 0,
        quest_allowed: true,
        teleporting: false,
        recently_teleported: false,
        ignore_restrictions: false,
        enforce_maximum_level: true,
    };
    assert_eq!(definition.check(access), Err(PortalError::TooLow));
    assert_eq!(anchor.check(access), definition.check(access));
}
