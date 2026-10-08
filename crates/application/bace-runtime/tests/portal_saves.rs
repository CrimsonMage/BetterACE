use bace_content::{Property, SecondaryAttribute, WeenieV1};
use bace_entity::{EntityVital, VitalMutation};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::portal_saves::{PortalSavePlayer, freeze_portal_service};
use bace_simulation::{PortalServiceEffect, PortalServiceOrigin, PortalServiceTicket};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
use bace_types::{CellId, EntityId};
const ACTOR: EntityId = EntityId(0x50000001);
fn saved() -> PlayerSaveV6 {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "portal_fixture".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.secondary_attributes = vec![Property {
        id: 5,
        value: SecondaryAttribute {
            init_level: 100,
            level_from_cp: 0,
            cp_spent: 0,
            current_level: 100,
        },
    }];
    PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: ACTOR.0,
            template_revision: 1,
            mutation_revision: 7,
            state,
        },
        account_id: 1,
        name: "Portal Fixture".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}
fn ticket() -> PortalServiceTicket {
    PortalServiceTicket {
        origin: PortalServiceOrigin::Spell,
        operation: 11,
        cast: 12,
        actor: ACTOR,
        cast_actor: ACTOR,
        before_revision: 7,
        after_revision: 8,
        participants: vec![(ACTOR, 7, 8)],
        mana: Some(VitalMutation {
            actor: ACTOR,
            vital: EntityVital::Mana,
            before: 100,
            after: 70,
        }),
        effect: PortalServiceEffect::Teleport(vec![bace_world::WorldTeleport {
            actor: ACTOR,
            expected_epoch: 0,
            destination: CellId(0x12340001),
            position: bace_geometry::Vec3::new(12., 15., 8.),
            heading: 1.,
        }]),
    }
}
fn input(saved: &PlayerSaveV6) -> PortalSavePlayer<'_> {
    PortalSavePlayer {
        saved,
        version: 5,
        lease: CharacterLease {
            character_id: ACTOR.0,
            epoch: 3,
            state: OwnershipState::Online,
        },
    }
}
#[test]
fn portal_destination_and_mana_share_one_recoverable_checkpoint() {
    let saved = saved();
    let ticket = ticket();
    let result = freeze_portal_service(4, &ticket, &[input(&saved)]).unwrap();
    assert_eq!(result.operation.operation_id, "portal:4:11");
    let restored = PlayerSaveV6::decode(&result.operation.snapshots[0].bytes).unwrap();
    assert_eq!(restored.player.entity.mutation_revision, 8);
    assert_eq!(
        restored.player.entity.state.properties.secondary_attributes[0]
            .value
            .current_level,
        70
    );
    let position = &restored.player.entity.state.properties.positions[0].value;
    assert_eq!(position.obj_cell_id, 0x12340001);
    assert_eq!(position.position_z, 8.);
    assert_eq!(result.receipt.revisions, vec![(ACTOR, 8)]);
    assert_eq!(saved.player.entity.mutation_revision, 7);
    assert!(saved.player.entity.state.properties.positions.is_empty());
}
#[test]
fn stale_or_incomplete_portal_tickets_cannot_freeze_partial_success() {
    let saved = saved();
    let mut t = ticket();
    t.mana.as_mut().unwrap().before = 99;
    assert!(freeze_portal_service(4, &t, &[input(&saved)]).is_err());
    t = ticket();
    t.participants.push((ACTOR, 7, 8));
    assert!(freeze_portal_service(4, &t, &[input(&saved)]).is_err());
    t = ticket();
    t.after_revision = 9;
    assert!(freeze_portal_service(4, &t, &[input(&saved)]).is_err());
    t = ticket();
    t.origin = PortalServiceOrigin::Binding;
    assert!(freeze_portal_service(4, &t, &[input(&saved)]).is_err());
    t = ticket();
    if let PortalServiceEffect::Teleport(targets) = &mut t.effect {
        targets.push(targets[0]);
    }
    assert!(freeze_portal_service(4, &t, &[input(&saved)]).is_err());
}
#[test]
fn resource_free_world_source_checkpoints_only_actual_player_participants() {
    let saved = saved();
    let mut t = ticket();
    t.cast_actor = EntityId(0x70000001);
    t.mana = None;
    let result = freeze_portal_service(4, &t, &[input(&saved)]).unwrap();
    assert_eq!(result.operation.snapshots.len(), 1);
    assert_eq!(result.operation.snapshots[0].object_id, ACTOR.0);
    let restored = PlayerSaveV6::decode(&result.operation.snapshots[0].bytes).unwrap();
    assert_eq!(
        restored.player.entity.state.properties.secondary_attributes[0]
            .value
            .current_level,
        100
    );
    assert_eq!(
        restored.player.entity.state.properties.positions[0]
            .value
            .obj_cell_id,
        0x12340001
    );
    t.cast_actor = ACTOR;
    assert!(
        freeze_portal_service(4, &t, &[input(&saved)]).is_err(),
        "player source cannot omit its resource ownership"
    );
}
