use super::*;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_content::{Property, WeenieV1};
use bace_entity::{Combatant, CombatantProfile, EntityProperties, PropertyFamily, PropertyValue};
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_geometry::Vec3;
use bace_inventory::{InventoryContainer, ItemPlace};
use bace_motion::Capabilities;
use bace_physics::{
    Body, CollisionFace, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon,
    GeometryCell, GeometryRegion, GeometrySpawn,
};
use bace_types::{AccountId, CellId};
use std::{collections::BTreeMap, sync::Arc};

const ACTOR: EntityId = EntityId(1);
const VENDOR: EntityId = EntityId(2);
const MARKER: EntityId = EntityId(0x8000_1000);
const STOCK: EntityId = EntityId(0x8000_1001);
const COIN: EntityId = EntityId(0x8000_1002);
const FRESH: EntityId = EntityId(0x8000_1003);

fn item(id: EntityId, template: u32, stack: u32, revision: u64) -> InventoryItem {
    InventoryItem {
        structure: None,
        id,
        revision,
        template,
        stack_key: u64::from(template),
        place: ItemPlace::Contained {
            container: ACTOR,
            slot: 0,
            equipped: 0,
        },
        stack,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: false,
    }
}

fn source_item() -> Arc<WeenieV1> {
    let mut item = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "vendor_buy_leaf".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    item.properties.ints = vec![
        Property { id: 11, value: 10 },
        Property { id: 12, value: 1 },
        Property { id: 15, value: 2 },
        Property { id: 19, value: 2 },
    ];
    Arc::new(item)
}

fn kernel() -> Kernel {
    let cell = CellId(0x100);
    let geometry = Arc::new(
        GeometryRegion::prepare(vec![GeometryCell {
            id: cell.0,
            restriction: None,
            terrain: false,
            solids: vec![],
            static_primitives: vec![],
            boundary: vec![
                CollisionPlane {
                    normal: Vec3::new(1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(-1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., 1., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., -1., 0.),
                    distance: 20.,
                },
            ],
            faces: vec![CollisionFace {
                polygon: GdlePolygon::prepare(vec![
                    Vec3::new(-20., -20., 0.),
                    Vec3::new(20., -20., 0.),
                    Vec3::new(20., 20., 0.),
                    Vec3::new(-20., 20., 0.),
                ])
                .unwrap(),
                two_sided: true,
                object: None,
            }],
            portals: vec![],
        }])
        .unwrap(),
    );
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., 0.5),
                radius: 0.5,
            }],
            0.,
            0.2,
        )
        .unwrap(),
    );
    let mut world = bace_world::World::default();
    world.install_geometry(geometry.clone()).unwrap();
    for (id, x) in [(ACTOR, 0.), (VENDOR, 1.5)] {
        let body = Body::spawn_geometry(
            &geometry,
            GeometrySpawn {
                cell: cell.0,
                position: Vec3::new(x, 0., 0.),
                shape: shape.clone(),
                capabilities: Capabilities {
                    speed: 5.,
                    jump_impulse: 5.,
                },
                heading: 0.,
                maximum_turn_rate: 3.,
            },
        )
        .unwrap();
        world.insert(bace_entity::Actor { id, cell, body }).unwrap();
    }
    let mut kernel = Kernel::new(world, 8192).unwrap();
    kernel
        .world
        .register_combatant(
            ACTOR,
            Combatant::new(CombatantProfile {
                maximum_health: 10,
                melee_damage: 1,
                melee_range: 2.0,
                attack_duration: 1.0,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap(),
        )
        .unwrap();
    let mut props = EntityProperties::new(8).unwrap();
    props
        .adopt(
            props
                .propose(PropertyFamily::Float, 54, Some(PropertyValue::Float(4.)))
                .unwrap(),
        )
        .unwrap();
    kernel.world.register_properties(VENDOR, props).unwrap();
    let ranks = RankTable::new(&[0, 10]).unwrap();
    kernel
        .register_character(
            CharacterBinding {
                session: SessionId(1),
                account: AccountId(1),
                actor: ACTOR,
            },
            CharacterProgression::new(
                &[],
                Arc::new(ProgressionTables {
                    attributes: ranks.clone(),
                    vitals: ranks.clone(),
                    trained_skills: ranks.clone(),
                    specialized_skills: ranks,
                }),
                0,
                0,
            )
            .unwrap(),
        )
        .unwrap();
    kernel
        .register_inventory_container(InventoryContainer {
            id: ACTOR,
            revision: 1,
            root_owner: Some(ACTOR),
            slots: 10,
            pack_slots: 2,
            burden_limit: 1000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
    kernel
        .register_inventory_item(item(COIN, 273, 10, 1))
        .unwrap();
    kernel.register_vendor_stock(VENDOR, 10).unwrap();
    let batch = crate::PreparedVendorLazyStock {
        vendor: VENDOR,
        vendor_expected_version: 2,
        marker: MARKER,
        operation_id: "vendor-seed".into(),
        source_revision: 7,
        source_hash: [3; 32],
        expected_stock_revision: 0,
        entries: vec![crate::PreparedVendorLazyItem {
            tree: crate::PreparedVendorTree {
                root: STOCK,
                template: source_item(),
                items: vec![],
                containers: vec![],
                templates: BTreeMap::new(),
            },
            display_quantity: -1,
            source_destinations: BTreeMap::from([(STOCK, Some(4))]),
        }],
    };
    kernel.reserve_vendor_lazy_stock(batch).unwrap();
    kernel
        .confirm_vendor_lazy_stock(crate::VendorLazyStockReceipt {
            vendor: VENDOR,
            marker: MARKER,
            operation_id: "vendor-seed".into(),
            marker_version: 1,
            items: vec![(STOCK, 1)],
        })
        .unwrap();
    kernel
}

fn context() -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(1),
        actor: ACTOR,
        sequence: 1,
    }
}

fn source() -> VendorBuySource {
    VendorBuySource {
        vendor_expected_version: 2,
        revision: 7,
        hash: [3; 32],
        sell_rate: Some(1.),
    }
}

fn reserve(kernel: &mut Kernel) -> VendorBuyReservation {
    let fresh = item(FRESH, 100, 2, 1);
    kernel
        .reserve_vendor_default_buy(
            context(),
            VENDOR,
            source(),
            &[VendorBuyRequest {
                stock_id: STOCK.0,
                amount: 2,
            }],
            &[fresh],
            "vendor-buy-1".into(),
        )
        .unwrap()
}

#[test]
fn joined_buy_reservation_blocks_generic_inventory_and_requires_exact_receipt() {
    let mut kernel = kernel();
    let ticket = reserve(&mut kernel);
    assert_eq!(ticket.quote.total_cost, 4);
    assert_eq!(
        kernel.characters.vendor_buy_operation(ACTOR),
        Some(ticket.inventory.operation)
    );
    assert_eq!(
        kernel.characters.get(ACTOR).unwrap().revision(),
        ticket.actor_revision
    );
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 10);
    let operation = ticket.inventory.operation;
    assert!(kernel.vendor_buy_owns_inventory(operation));
    assert_eq!(kernel.take_inventory_proposal(), None);
    assert_eq!(
        kernel.retry_inventory(operation),
        Err(InventoryRejection::DurabilityPending)
    );
    assert_eq!(
        kernel.reject_inventory(operation),
        Err(InventoryRejection::DurabilityPending)
    );
    let inventory = crate::InventoryReceipt {
        operation,
        revisions: ticket
            .inventory
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    };
    assert_eq!(
        kernel.confirm_inventory_committed(&inventory),
        Err(InventoryRejection::DurabilityPending)
    );
    let mut receipt = VendorBuyReceipt {
        vendor: VENDOR,
        marker: MARKER,
        operation_id: ticket.operation_id.clone(),
        marker_version: 2,
        marker_stock_revision: ticket.marker_stock_revision + 1,
        inventory,
    };
    receipt.marker_stock_revision += 1;
    assert_eq!(
        kernel.confirm_vendor_buy_committed(&ticket, &receipt),
        Err(VendorBuyError::Stale)
    );
    assert!(kernel.vendor_buy_owns_inventory(operation));
    receipt.marker_stock_revision -= 1;
    kernel
        .confirm_vendor_buy_committed(&ticket, &receipt)
        .unwrap();
    assert!(!kernel.vendor_buy_owns_inventory(operation));
    assert_eq!(kernel.characters.vendor_buy_operation(ACTOR), None);
    assert_eq!(
        kernel.characters.get(ACTOR).unwrap().revision(),
        ticket.actor_revision + 1
    );
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 6);
    assert_eq!(kernel.inventory_item(FRESH).unwrap().stack, 2);
    assert!(kernel.vendor_stock_durable(VENDOR));
}

#[test]
fn definite_buy_rejection_releases_exact_inventory_reservation() {
    let mut kernel = kernel();
    let ticket = reserve(&mut kernel);
    kernel.reject_vendor_buy(&ticket).unwrap();
    assert!(!kernel.vendor_buy_owns_inventory(ticket.inventory.operation));
    assert_eq!(kernel.characters.vendor_buy_operation(ACTOR), None);
    assert_eq!(
        kernel.characters.get(ACTOR).unwrap().revision(),
        ticket.actor_revision
    );
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 10);
    assert!(kernel.inventory_item(FRESH).is_none());
    assert!(kernel.vendor_stock_durable(VENDOR));
}

#[test]
fn stale_source_or_insufficient_currency_never_reserves_the_vendor() {
    let mut kernel = kernel();
    let mut stale = source();
    stale.hash = [4; 32];
    assert_eq!(
        kernel.reserve_vendor_default_buy(
            context(),
            VENDOR,
            stale,
            &[VendorBuyRequest {
                stock_id: STOCK.0,
                amount: 2,
            }],
            &[item(FRESH, 100, 2, 1)],
            "vendor-buy-stale".into(),
        ),
        Err(VendorBuyError::Stale)
    );
    assert_eq!(
        kernel.reserve_vendor_default_buy(
            context(),
            VENDOR,
            source(),
            &[VendorBuyRequest {
                stock_id: STOCK.0,
                amount: 6,
            }],
            &[item(FRESH, 100, 6, 1)],
            "vendor-buy-short".into(),
        ),
        Err(VendorBuyError::Inventory(InventoryRejection::Requirements))
    );
    assert!(kernel.generated_vendors[&VENDOR].pending_buy.is_none());
    assert!(kernel.vendor_stock_durable(VENDOR));
    assert_eq!(kernel.take_inventory_proposal(), None);
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 10);
}

#[test]
fn vendor_command_retains_buy_reservation_until_exact_receipt() {
    let mut kernel = kernel();
    let outcome = kernel.vendor_command(crate::VendorCommand {
        correlation: 73,
        action: crate::VendorAction::ReserveDefaultBuy {
            context: context(),
            vendor: VENDOR,
            source: source(),
            requests: vec![VendorBuyRequest {
                stock_id: STOCK.0,
                amount: 2,
            }],
            prepared: vec![item(FRESH, 100, 2, 1)],
            operation_id: "vendor-buy-command".into(),
        },
    });
    assert_eq!(outcome.correlation, 73);
    let crate::VendorDecision::BuyReserved(ticket) = outcome.result.unwrap() else {
        panic!("expected retained buy ticket");
    };
    assert!(kernel.vendor_buy_owns_inventory(ticket.inventory.operation));
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 10);
    let receipt = VendorBuyReceipt {
        vendor: VENDOR,
        marker: MARKER,
        operation_id: ticket.operation_id.clone(),
        marker_version: 2,
        marker_stock_revision: ticket.marker_stock_revision + 1,
        inventory: crate::InventoryReceipt {
            operation: ticket.inventory.operation,
            revisions: ticket
                .inventory
                .proposal
                .changes
                .iter()
                .map(|change| (change.after.id, change.after.revision))
                .collect(),
        },
    };
    let mut altered = receipt.clone();
    altered.marker_version += 1;
    assert_eq!(
        kernel
            .vendor_command(crate::VendorCommand {
                correlation: 74,
                action: crate::VendorAction::ConfirmBuy {
                    ticket: ticket.clone(),
                    receipt: altered,
                },
            })
            .result,
        Err(crate::VendorCommandError::Buy(VendorBuyError::Stale))
    );
    assert!(kernel.vendor_buy_owns_inventory(ticket.inventory.operation));
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 10);
    assert!(matches!(
        kernel
            .vendor_command(crate::VendorCommand {
                correlation: 75,
                action: crate::VendorAction::ConfirmBuy { ticket, receipt },
            })
            .result,
        Ok(crate::VendorDecision::BuyConfirmed(_))
    ));
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 6);
}

#[test]
fn bounded_buy_command_rejects_before_owner_mutation() {
    let mut kernel = kernel();
    let command = crate::VendorCommand {
        correlation: 76,
        action: crate::VendorAction::ReserveDefaultBuy {
            context: context(),
            vendor: VENDOR,
            source: source(),
            requests: vec![
                VendorBuyRequest {
                    stock_id: STOCK.0,
                    amount: 1,
                };
                1025
            ],
            prepared: vec![item(FRESH, 100, 1, 1)],
            operation_id: "vendor-buy-over-capacity".into(),
        },
    };
    assert!(!command.valid_bounds());
    assert!(kernel.vendor_command(command).result.is_err());
    assert!(kernel.generated_vendors[&VENDOR].pending_buy.is_none());
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 10);
}

#[test]
fn vendor_command_definite_rejection_releases_held_buy() {
    let mut kernel = kernel();
    let ticket = reserve(&mut kernel);
    let outcome = kernel.vendor_command(crate::VendorCommand {
        correlation: 77,
        action: crate::VendorAction::RejectBuy(Box::new(ticket.clone())),
    });
    assert_eq!(outcome.result, Ok(crate::VendorDecision::BuyRejected));
    assert!(!kernel.vendor_buy_owns_inventory(ticket.inventory.operation));
    assert_eq!(kernel.inventory_item(COIN).unwrap().stack, 10);
    assert!(kernel.inventory_item(FRESH).is_none());
}

#[test]
fn vendor_buy_snapshot_fence_requires_exact_pending_ticket_and_revision() {
    let mut kernel = kernel();
    let binding = CharacterBinding {
        session: SessionId(1),
        account: AccountId(1),
        actor: ACTOR,
    };
    let ticket = reserve(&mut kernel);
    assert!(kernel.validate_vendor_buy_snapshot(
        binding,
        ticket.inventory.operation,
        ticket.actor_revision
    ));
    assert!(!kernel.validate_vendor_buy_snapshot(
        binding,
        ticket.inventory.operation + 1,
        ticket.actor_revision
    ));
    assert!(!kernel.validate_vendor_buy_snapshot(
        binding,
        ticket.inventory.operation,
        ticket.actor_revision + 1
    ));
    kernel.reject_vendor_buy(&ticket).unwrap();
    assert!(!kernel.validate_vendor_buy_snapshot(
        binding,
        ticket.inventory.operation,
        ticket.actor_revision
    ));
}
