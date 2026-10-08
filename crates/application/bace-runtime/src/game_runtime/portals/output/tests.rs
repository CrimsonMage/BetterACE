use super::*;
#[test]
fn original_ace_portal_flags_and_conditional_broadcasts() {
    let mut count = 0;
    for line in include_str!("../../../../tests/fixtures/portal_state.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row: Vec<u32> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let phase = if row[2] == 0 {
            PortalPhase::Teleport
        } else {
            PortalPhase::Materialize
        };
        let after = portal_state(row[0], phase, row[1] == 2);
        assert_eq!(after, row[3], "{line}");
        let broadcast = u32::from(phase == PortalPhase::Materialize || after != row[0]);
        assert_eq!(broadcast, row[4], "{line}");
        assert_eq!(portal_state(row[0], PortalPhase::Hide, row[1] == 2), row[0]);
        count += 1;
    }
    assert_eq!(count, 80);
}
#[test]
fn output_view_order_and_capacity_are_checked_before_canonical_projection() {
    let view = bace_simulation::PortalAcceptedView {
        actor: EntityId(1),
        position: bace_interactions::PortalPosition {
            cell: 1,
            origin: [0.; 3],
            rotation: [1., 0., 0., 0.],
        },
        velocity: [0.; 3],
        grounded: true,
        epoch: 1,
        cloaked: false,
    };
    assert!(validate_views(&[EntityId(1)], &[view]).is_ok());
    assert!(validate_views(&[EntityId(2)], &[view]).is_err());
    assert!(validate_views(&[EntityId(1)], &[]).is_err());
    assert!(validate_views(&[], &[]).is_err());
    assert!(validate_views(&[EntityId(1); 10], &[view; 10]).is_err());
}
