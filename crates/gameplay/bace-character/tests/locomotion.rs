use bace_character::*;
#[test]
fn carrying_capacity_and_refused_jump_boundaries() {
    let load = encumbrance(100, 1, 18000).unwrap();
    assert_eq!(load.capacity, 18000);
    assert_eq!(load.maximum_inventory_burden, 54000);
    assert_eq!(load.modifier, 1.0);
    assert_eq!(encumbrance(100, 1, 36000).unwrap().modifier, 0.0);
    let jump = JumpInput {
        skill: 100,
        burden: 1.0,
        scale: 1.0,
        extent: 1.0,
        stamina: 13,
        pk_timer_active: false,
    };
    assert_eq!(jump_proposal(jump), Err(LocomotionError::TooTired));
    assert_eq!(
        jump_proposal(JumpInput {
            stamina: 14,
            ..jump
        })
        .unwrap()
        .remaining_stamina,
        0
    );
    assert_eq!(
        jump_proposal(JumpInput {
            extent: f32::NAN,
            ..jump
        }),
        Err(LocomotionError::InvalidInput)
    );
    assert!(encumbrance(u32::MAX, 5, 0).is_err());
}
