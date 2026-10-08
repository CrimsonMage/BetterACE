#[allow(dead_code, unused_imports)]
#[path = "magic_common/mod.rs"]
mod common;
use bace_gameplay_api::staff::{
    MapTeleportRequest, PreparedMapTeleport, StaffDestination, StaffError, StaffEvent,
    StaffPrivileges, StaffRegistration,
};
use common::*;
fn registration(admin: bool) -> StaffRegistration {
    StaffRegistration {
        binding: CharacterBinding {
            session: SessionId(7),
            account: AccountId(1),
            actor: EntityId(1),
        },
        privileges: StaffPrivileges {
            account_access: if admin { 5 } else { 2 },
            admin,
            sentinel: !admin,
            ..Default::default()
        },
    }
}
fn request() -> MapTeleportRequest {
    MapTeleportRequest {
        cell: 1,
        origin: [2., 0., 9000.],
        rotation: [1., 0., 0., 0.],
    }
}
fn prepared(k: &Kernel) -> PreparedMapTeleport {
    PreparedMapTeleport {
        requested_cell: 1,
        requested_xy: [2., 0.],
        destination: StaffDestination {
            cell: 1,
            origin: [2., 0., 0.5],
            rotation: [1., 0., 0., 0.],
        },
        entirely_water: false,
        expected_epoch: k.world().actor_state(EntityId(1)).unwrap().1.epoch(),
    }
}
#[test]
fn map_privilege_water_and_prepared_destination() {
    let mut k = kernel(8);
    k.register_staff(registration(false)).unwrap();
    assert_eq!(
        k.staff_map_teleport(context(1), request(), prepared(&k)),
        Err(StaffError::NotAuthorized)
    );
    k.refresh_staff(registration(true)).unwrap();
    let mut water = prepared(&k);
    water.entirely_water = true;
    assert_eq!(
        k.staff_map_teleport(context(1), request(), water),
        Err(StaffError::Water)
    );
    let old = k.world().actor_state(EntityId(1)).unwrap().1;
    k.staff_map_teleport(context(1), request(), prepared(&k))
        .unwrap();
    let after = k.world().actor_state(EntityId(1)).unwrap().1;
    assert_eq!(after.position(), Vec3::new(2., 0., 0.5));
    assert_ne!(after.epoch(), old.epoch());
    assert!(matches!(
        k.peek_staff_event(),
        Some(StaffEvent::Teleported { .. })
    ));
    assert_eq!(
        k.staff_map_teleport(context(1), request(), prepared(&k)),
        Err(StaffError::Stale)
    );
}
#[test]
fn heal_is_authorized_player_only_and_retains_output() {
    let mut k = kernel(8);
    k.register_staff(registration(true)).unwrap();
    k.staff_heal(context(1), EntityId(2), Some("Drudge"), false)
        .unwrap();
    assert!(matches!(
        k.take_staff_event(),
        Some(StaffEvent::Inspection { target: EntityId(2), lines, .. })
            if lines == ["You cannot heal Drudge because it is not a player."]
    ));
    k.staff_heal(context(2), EntityId(999), None, false)
        .unwrap();
    assert!(matches!(
        k.take_staff_event(),
        Some(StaffEvent::Inspection { target: EntityId(999), lines, .. })
            if lines == ["Unable to locate what you have selected."]
    ));
    k.staff_heal(context(3), EntityId(1), None, false).unwrap();
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        100
    );
    assert!(matches!(
        k.peek_staff_event(),
        Some(StaffEvent::Healed {
            target: EntityId(1),
            ..
        })
    ));
    assert!(k.take_staff_event().is_some());
    assert!(k.take_staff_event().is_none());
}
