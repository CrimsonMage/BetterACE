#[allow(dead_code, unused_imports, reason = "shared full magic owner fixture")]
mod magic_common;
#[path = "recall_destinations/recall_motion.rs"]
mod recall_motion;
use bace_interactions::{PortalAccess, PortalLinks, PortalPosition, RecallError, RecallKind};
use bace_simulation::{PreparedRecallLocations, RecallCommand, RecallEvent};
use magic_common::*;

fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(7),
    }
}
fn position(x: f32) -> PortalPosition {
    PortalPosition {
        cell: 1,
        origin: [x, 4., 0.5],
        rotation: [1., 0., 0., 0.],
    }
}
fn prepared() -> Kernel {
    prepared_in_mode(1)
}
fn prepared_in_mode(mode: u32) -> Kernel {
    let mut k = kernel_in_mode(64, mode);
    k.configure_recall_random(
        Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
        7,
    )
    .unwrap();
    k.register_recall_locations(Arc::new(PreparedRecallLocations {
        marketplace: position(3.),
        pk_arena: std::array::from_fn(|i| position(i as f32 + 1.)),
        pkl_arena: std::array::from_fn(|i| position(i as f32 + 6.)),
    }))
    .unwrap();
    k.register_portal_links(
        EntityId(1),
        PortalLinks::new(0, &[(4, position(4.))], &[]).unwrap(),
        PortalAccess {
            level: 275,
            pk_status: 4,
            pk_recent: false,
            olthoi: false,
            vitae: false,
            account_15_days: true,
            entitlement: u32::MAX,
            quest_allowed: true,
            teleporting: false,
            recently_teleported: false,
            ignore_restrictions: false,
            enforce_maximum_level: true,
        },
    )
    .unwrap();
    k
}

#[test]
fn inspecting_cold_destinations_preserves_live_recall_randomness_and_state() {
    let mut inspected = prepared();
    let mut untouched = prepared();
    let before = inspected
        .world()
        .actor_state(EntityId(1))
        .unwrap()
        .1
        .position();
    let mana = inspected
        .world()
        .vital(EntityId(1), EntityVital::Mana)
        .unwrap();
    let initial = inspected.read_player_snapshot(binding()).unwrap();
    let revision = initial.character().progression().revision();
    for _ in 0..20 {
        let snapshot = inspected.read_player_snapshot(binding()).unwrap();
        let destinations = snapshot.recall_destinations();
        assert_eq!(
            destinations.candidates(RecallKind::Lifestone),
            Ok([position(4.)].as_slice())
        );
        assert_eq!(
            destinations.candidates(RecallKind::Marketplace),
            Ok([position(3.)].as_slice())
        );
        assert_eq!(
            destinations.candidates(RecallKind::House),
            Err(RecallError::NoHouse)
        );
        assert_eq!(
            destinations.candidates(RecallKind::AllegianceHometown),
            Err(RecallError::NoAllegiance)
        );
        assert_eq!(
            destinations.candidates(RecallKind::AllegianceHousing),
            Err(RecallError::NoAllegiance)
        );
        assert_eq!(
            destinations.candidates(RecallKind::PkArena).unwrap(),
            std::array::from_fn::<_, 5, _>(|i| position(i as f32 + 1.))
        );
        assert_eq!(
            destinations.candidates(RecallKind::PklArena).unwrap(),
            std::array::from_fn::<_, 5, _>(|i| position(i as f32 + 6.))
        );
        assert_eq!(snapshot.character().progression().revision(), revision);
    }
    assert_eq!(
        inspected
            .world()
            .actor_state(EntityId(1))
            .unwrap()
            .1
            .position(),
        before
    );
    assert_eq!(
        inspected
            .world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap(),
        mana
    );
    assert!(inspected.take_recall_event().is_none());
    for k in [&mut inspected, &mut untouched] {
        k.apply_recall_command(RecallCommand::Start {
            context: context(1),
            kind: RecallKind::PkArena,
            animation_seconds: 1. / 30.,
        })
        .unwrap();
        assert!(matches!(
            k.take_recall_event(),
            Some(RecallEvent::Started { .. })
        ));
        for _ in 0..3 {
            step(k);
        }
    }
    let a = inspected
        .take_portal_proposal()
        .expect("inspected recall staged");
    let b = untouched
        .take_portal_proposal()
        .expect("untouched recall staged");
    assert_eq!(a.operation, b.operation);
    assert_eq!(a.effect, b.effect);
}
