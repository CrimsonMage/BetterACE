//! Pinned ACE WorldObject_Magic.SummonPortal creates `portalgateway` (1955)
//! and keeps the linked portal as OriginalPortal metadata.
use super::*;
use bace_content::{Property, SparseProperties, WeenieV1};
use bace_interactions::PortalPosition;
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn portal(id: u32, class_name: &str, name: &str) -> WeenieV1 {
    let mut properties = SparseProperties::default();
    properties.strings.push(Property {
        id: 1,
        value: name.into(),
    });
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: class_name.into(),
        weenie_type: 18,
        last_modified: None,
        properties,
    }
}

#[test]
fn summoned_visual_uses_gateway_record_while_original_portal_stays_linked() {
    let original_template = 0x1234;
    let gateway = portal(PORTAL_GATEWAY_TEMPLATE, "portalgateway", "Summoned Gateway");
    let original = portal(original_template, "linkedportal", "Linked Portal");
    let directory = tempfile::tempdir().unwrap();
    let built = bace_content_tools::build_world_pack(
        &[gateway, original],
        &[],
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let generation = manifest.open(directory.path(), Default::default()).unwrap();
    let position = PortalPosition {
        cell: 0x1234_0001,
        origin: [0.; 3],
        rotation: [1., 0., 0., 0.],
    };
    let effect = bace_simulation::PortalServiceEffect::Summon {
        entity: EntityId(0x8000_0001),
        template: PORTAL_GATEWAY_TEMPLATE,
        original_template,
        origin: position,
        destination: position,
        lifetime: 60.,
    };
    let bace_simulation::PortalServiceEffect::Summon {
        template,
        original_template: linked,
        ..
    } = effect
    else {
        panic!("summon effect");
    };
    let visual = load_gateway_visual(&generation, template).unwrap();
    assert_eq!(visual.weenie_id, 1955);
    assert_eq!(visual.class_name, "portalgateway");
    assert_eq!(visual.properties.strings[0].value, "Summoned Gateway");
    assert_eq!(linked, original_template);
    assert_ne!(linked, visual.weenie_id);
    assert_eq!(
        load_gateway_visual(&generation, linked).unwrap_err(),
        "summoned portal gateway template mismatch"
    );
}

#[tokio::test]
async fn summoned_gateway_create_and_lifetime_removal_wait_for_observer_receipts() {
    // ACE SummonPortal enters a portalgateway with TimeToRot. Its later
    // heartbeat deletes that object. These are synthetic accepted owner views
    // and reliable receipts, not a DAT or game-client playback.
    let (_cluster, _directory, mut runtime, key, binding) =
        super::super::tests::output_runtime().await;
    runtime.players.test_mark_entered(key, binding).unwrap();
    runtime.visibility.service.bind(key, binding).unwrap();

    let directory = tempfile::tempdir().unwrap();
    let built = bace_content_tools::build_world_pack(
        &[
            portal(PORTAL_GATEWAY_TEMPLATE, "portalgateway", "Summoned Gateway"),
            portal(0x1234, "linkedportal", "Linked Portal"),
        ],
        &[],
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let generation = manifest.open(directory.path(), Default::default()).unwrap();
    let source = load_gateway_visual(&generation, PORTAL_GATEWAY_TEMPLATE).unwrap();
    if bace_loot::ace_tables::active_id().is_none() {
        let text =
            include_str!("../../../../../../gameplay/bace-loot/data/ace-treasure-tables.toml");
        let tables = bace_content_tools::parse_treasure_table_set(text).unwrap();
        if let Err(error) = bace_loot::ace_tables::install(tables) {
            assert_eq!(bace_loot::ace_tables::active_id(), Some(1), "{error}");
        }
    }
    assert_eq!(bace_loot::ace_tables::active_id(), Some(1));

    let entity = EntityId(0x8000_0001);
    let operation = 77;
    let sequences = bace_replication::Sequences::new(10).unwrap();
    let description = crate::player_entry::prepare_entry_object(
        entity.0,
        &source,
        crate::player_entry::PreparedEntryModel {
            model: bace_wire::ObjectModel::default(),
            icon_override: None,
        },
        crate::player_entry::EntryObjectState {
            is_player: false,
            is_creature: false,
            physics_state: 0,
            position: None,
            movement: None,
            parent: None,
            children: vec![],
            velocity: [0.; 3],
            acceleration: [0.; 3],
            omega: [0.; 3],
            sequences: bace_replication::physics_sequences(&sequences),
            admin_vision: false,
            change_no_draw: false,
            cloak_status: 0,
        },
    )
    .unwrap();
    assert_eq!(description.game.class_id, PORTAL_GATEWAY_TEMPLATE);
    let blueprint = crate::visibility_assets::PreparedVisibilityObject {
        incarnation: operation,
        revision: generation.revision().max(1),
        description: Arc::new(description),
        children: vec![],
    };
    let position = PortalPosition {
        cell: 0x1234_0001,
        origin: [1., 2., 3.],
        rotation: [1., 0., 0., 0.],
    };
    let ticket = PortalServiceTicket {
        origin: bace_simulation::PortalServiceOrigin::Spell,
        operation,
        cast: 78,
        actor: binding.actor,
        cast_actor: binding.actor,
        before_revision: 1,
        after_revision: 2,
        participants: vec![(binding.actor, 1, 2)],
        mana: None,
        effect: bace_simulation::PortalServiceEffect::Summon {
            entity,
            template: PORTAL_GATEWAY_TEMPLATE,
            original_template: 0x1234,
            origin: position,
            destination: position,
            lifetime: 1.0,
        },
    };
    runtime.portals.tickets.insert(operation, ticket.clone());
    runtime
        .portals
        .ticket_bindings
        .insert(operation, vec![binding]);
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Summoned {
            operation,
            entity,
            template: PORTAL_GATEWAY_TEMPLATE,
        }));
    runtime.portals.summon_ready = Some((
        operation + 1,
        entity,
        PORTAL_GATEWAY_TEMPLATE,
        blueprint.clone(),
    ));
    assert!(runtime.poll_portal_summons().is_err());
    assert!(runtime.portals.output_pending());
    assert_eq!(
        runtime.visibility.service.registered_object_name(entity),
        None
    );
    let mut invalid_ticket = ticket.clone();
    let bace_simulation::PortalServiceEffect::Summon {
        original_template, ..
    } = &mut invalid_ticket.effect
    else {
        panic!("summon effect")
    };
    *original_template = 0;
    runtime.portals.tickets.insert(operation, invalid_ticket);
    runtime.portals.summon_ready = Some((operation, entity, PORTAL_GATEWAY_TEMPLATE, blueprint));
    assert!(runtime.poll_portal_summons().is_err());
    assert!(runtime.portals.output_pending());
    assert_eq!(
        runtime.visibility.service.registered_object_name(entity),
        None
    );
    runtime.portals.tickets.insert(operation, ticket.clone());
    runtime.poll_portal_summons().unwrap();
    assert!(!runtime.portals.output_pending());
    assert_eq!(
        runtime.visibility.service.registered_object_name(entity),
        Some("Summoned Gateway")
    );
    runtime
        .visibility
        .service
        .test_stage_accepted_nonplayer(
            key,
            1,
            bace_gameplay_api::visibility::AcceptedObjectView {
                entity,
                cell: 0x1234_0001,
                position: [1., 2., 3.],
                velocity: [0.; 3],
                heading_radians: 0.,
                grounded: true,
                epoch: 0,
                held: false,
                motion: Ok(None),
            },
        )
        .unwrap();
    assert!(runtime.visibility.service.pending());
    let message_capacity = runtime.limits.messages;
    runtime.limits.messages = 0;
    runtime
        .portals
        .push(PortalDeliveryWork::Completed(Box::new(PortalCompletion {
            work: PortalWork {
                epoch: 1,
                bindings: vec![binding],
                ticket: ticket.clone(),
            },
            committed: true,
            aborted: false,
        })));
    runtime.project_portal_deliveries().unwrap();
    runtime.limits.messages = message_capacity;
    assert!(!runtime.portals.tickets.contains_key(&operation));
    assert!(!runtime.portals.output_pending());
    assert!(!runtime.visibility.service.knows(key, entity));
    let (create_receipt, create) = runtime
        .visibility
        .service
        .test_submit_reliable(key)
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(create[0][..4].try_into().unwrap()),
        0xf745
    );
    assert!(
        !runtime
            .visibility
            .service
            .reliable_admission(key, create_receipt + 1, true)
            .unwrap()
    );
    assert!(!runtime.visibility.service.knows(key, entity));
    assert!(
        runtime
            .visibility
            .service
            .reliable_admission(key, create_receipt, true)
            .unwrap()
    );
    assert!(runtime.visibility.service.knows(key, entity));

    // Inject the simulation's Removed event after the ticket's lifetime. This
    // exercises its adapter/observer handoff; expiry selection has its own owner.
    runtime.last_elapsed = Duration::from_secs(2);
    runtime
        .portals
        .push(PortalDeliveryWork::Event(PortalServiceEvent::Removed {
            entity,
        }));
    runtime.project_portal_deliveries().unwrap();
    assert_eq!(runtime.visibility.retirements.get(&entity), Some(&60));
    runtime.poll_visibility().unwrap();
    assert!(!runtime.visibility.retirements.contains_key(&entity));
    assert_eq!(
        runtime.visibility.service.registered_object_name(entity),
        None
    );
    assert!(runtime.visibility.service.knows(key, entity));
    let (delete_receipt, delete) = runtime
        .visibility
        .service
        .test_submit_reliable(key)
        .unwrap();
    assert_eq!(
        delete,
        vec![
            bace_wire::ObjectControl::Delete {
                object_id: entity.0,
                instance_sequence: 0,
            }
            .encode()
        ]
    );
    assert!(
        !runtime
            .visibility
            .service
            .reliable_admission(key, delete_receipt + 1, true)
            .unwrap()
    );
    assert!(runtime.visibility.service.knows(key, entity));
    assert!(
        runtime
            .visibility
            .service
            .reliable_admission(key, delete_receipt, true)
            .unwrap()
    );
    assert!(!runtime.visibility.service.knows(key, entity));

    runtime.visibility.service.unbind(key);
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    runtime.sessions.remove(&key);
    runtime.quiesce(Duration::ZERO).unwrap();
}
