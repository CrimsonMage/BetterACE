use bace_content::{Property, WeenieV1};
use bace_gameplay_api::*;
use bace_inventory::*;
use bace_random::RandomRoot;
use bace_simulation::{
    GeneratorControl, GeneratorHostRequest, GeneratorServiceError, InventoryReceipt, Kernel,
};
use bace_types::{AccountId, EntityId};
use std::sync::Arc;
#[path = "generated_inventory/trees.rs"]
mod trees;
#[path = "generated_inventory/vendor_trees.rs"]
mod vendor_trees;
fn container(id: u32, owner: Option<u32>) -> InventoryContainer {
    InventoryContainer {
        id: EntityId(id),
        revision: 1,
        root_owner: owner.map(EntityId),
        slots: 10,
        pack_slots: 10,
        burden_limit: 10000,
        accessible: true,
        open: true,
        generation: 1,
    }
}
fn kernel() -> Kernel {
    let mut k = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let ranks = bace_character::RankTable::new(&[0, 10]).unwrap();
    let character = bace_character::CharacterProgression::new(
        &[],
        Arc::new(bace_character::ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        0,
        0,
    )
    .unwrap();
    k.register_character(
        CharacterBinding {
            session: SessionId(1),
            account: AccountId(1),
            actor: EntityId(1),
        },
        character,
    )
    .unwrap();
    k.register_inventory_container(container(1, Some(1)))
        .unwrap();
    k.configure_generators(Arc::new(RandomRoot::new([7; 32], 1).unwrap()), 1000, true)
        .unwrap();
    k
}
fn definition(id: u32, flags: u32) -> GeneratorDefinition {
    GeneratorDefinition {
        identity: GeneratorIdentity {
            entity: EntityId(id),
            incarnation: 1,
            content_revision: 1,
            random_identity: [1; 16],
        },
        profiles: vec![GeneratorProfile {
            id: 0,
            probability: -1.,
            weenie_class_id: 100,
            delay: Some(0.),
            init_create: 1,
            max_create: 1,
            when_create: 1,
            where_create: flags,
            stack_size: None,
            palette_id: None,
            shade: None,
            position: Default::default(),
        }],
        location: GeneratorLocation {
            cell: 0x01010001,
            origin: [0.; 3],
            rotation: [0., 0., 0., 1.],
        },
        kind: if flags == 32 {
            GeneratorKind::Vendor
        } else {
            GeneratorKind::Container
        },
        initial_count: 1,
        maximum_count: 1,
        regeneration_interval: 1.,
        initial_delay: 0.,
        regeneration_timestamp: 0.,
        time_type: GeneratorTimeType::Undefined,
        event: None,
        start_time: 0,
        end_time: 0,
        disabled: false,
        automatic_destruction: false,
        parent: None,
        destruction: GeneratorDestruction::Destroy,
        end_destruction: GeneratorDestruction::Destroy,
        rotation_type: GeneratorRotationType::Undefined,
        use_rotation_offset: true,
        radius: 0.,
        vendor_shop_uses_generator: true,
    }
}
fn item(id: u32, parent: u32, bag: bool) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 100,
        stack_key: 0,
        place: ItemPlace::Contained {
            container: EntityId(parent),
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: bag,
        is_container: bag,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
fn request(k: &mut Kernel, id: u32) -> GeneratorHostRequest {
    k.supply_generator_id(EntityId(id)).unwrap();
    k.step().unwrap();
    k.take_generator_request().expect("spawn host request")
}
fn authority() -> InventoryAuthority {
    InventoryAuthority {
        actor: EntityId(1),
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: true,
        source_view: Some(1),
        destination_view: None,
        new_item: None,
    }
}
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
fn move_to_player(k: &mut Kernel, id: u32, sequence: u32) -> bace_simulation::InventoryTicket {
    let operation = k
        .propose_inventory(
            context(sequence),
            InventoryRequest::Move {
                item: EntityId(id),
                container: EntityId(1),
                placement: 0,
            },
            authority(),
        )
        .unwrap();
    let ticket = k.take_inventory_proposal().unwrap();
    assert_eq!(ticket.operation, operation);
    ticket
}
fn receipt(ticket: &bace_simulation::InventoryTicket) -> InventoryReceipt {
    InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    }
}
#[test]
fn contain_admission_is_exact_and_first_acquisition_releases_slot_only_after_commit() {
    let mut k = kernel();
    let d = definition(10, 8);
    k.register_inventory_container(container(10, None)).unwrap();
    k.register_generator(Arc::new(d.clone())).unwrap();
    let request = request(&mut k, 0x80000101);
    let correct = item(request.entities[0].0, 10, false);
    let wrong = item(correct.id.0, 1, false);
    assert_eq!(
        k.admit_generated_inventory(request.intent.key, &[wrong], &[]),
        Err(GeneratorServiceError::Invalid)
    );
    assert!(k.inventory_item(correct.id).is_none());
    k.admit_generated_inventory(request.intent.key, std::slice::from_ref(&correct), &[])
        .unwrap();
    assert_eq!(k.inventory_item(correct.id), Some(&correct));
    assert!(
        k.admit_generated_inventory(request.intent.key, std::slice::from_ref(&correct), &[])
            .is_err()
    );
    let ticket = move_to_player(&mut k, correct.id.0, 1);
    let expected = k.generated_inventory_items(ticket.operation).unwrap();
    assert_eq!(expected, vec![correct.id]);
    let exact = receipt(&ticket);
    assert!(k.confirm_inventory_committed(&exact).is_err());
    assert_eq!(k.inventory_count(EntityId(1), 100), 0);
    assert!(k.generator_inventory_busy(d.identity));
    assert_eq!(
        k.generator_control(d.identity, GeneratorControl::Reset),
        Err(GeneratorServiceError::Busy)
    );
    let mut wrong = exact.clone();
    wrong.revisions[0].1 += 1;
    assert!(
        k.confirm_generated_inventory_committed(&wrong, &expected)
            .is_err()
    );
    assert!(
        k.confirm_generated_inventory_committed(&exact, &[])
            .is_err()
    );
    assert_eq!(k.inventory_item(correct.id), Some(&correct));
    k.supply_generator_id(EntityId(0x80000102)).unwrap();
    for _ in 0..40 {
        k.step().unwrap();
    }
    assert!(
        k.take_generator_request().is_none(),
        "reserved item must keep its generator slot"
    );
    k.confirm_generated_inventory_committed(&exact, &expected)
        .unwrap();
    assert_eq!(k.inventory_count(EntityId(1), 100), 1);
    assert!(!k.generator_inventory_busy(d.identity));
    assert!(
        k.confirm_generated_inventory_committed(&exact, &expected)
            .is_err()
    );
    for _ in 0..40 {
        k.step().unwrap();
    }
    let respawn = k
        .take_generator_request()
        .expect("slot released by committed pickup");
    assert_eq!(respawn.entities, vec![EntityId(0x80000102)]);
}

#[test]
fn request_ids_are_atomic_idempotent_and_private_to_the_exact_spawn() {
    let mut k = kernel();
    k.register_inventory_container(container(10, None)).unwrap();
    k.register_generator(Arc::new(definition(10, 8))).unwrap();
    let first = request(&mut k, 0x80000801);
    assert_eq!(first.landblock, 0x0101);
    assert_eq!(first.next_slots, Some((0, 0)));
    let ids = [EntityId(0x80000802), EntityId(0x80000803)];
    assert_eq!(
        k.supply_generator_request_ids(first.intent.key, 1, &[ids[0], ids[0]]),
        Err(GeneratorServiceError::Invalid)
    );
    assert_eq!(
        k.supply_generator_request_ids(first.intent.key, 1, &[ids[0], EntityId(7)]),
        Err(GeneratorServiceError::Invalid)
    );
    k.supply_generator_request_ids(first.intent.key, 1, &ids)
        .unwrap();
    k.supply_generator_request_ids(first.intent.key, 1, &ids)
        .unwrap();
    assert!(k.supply_generator_id(ids[0]).is_err());
    assert_eq!(
        k.supply_generator_request_ids(first.intent.key, 2, &[EntityId(0x80000804)]),
        Err(GeneratorServiceError::Stale)
    );
    let mut stale = first.intent.key;
    stale.occurrence += 1;
    assert_eq!(
        k.supply_generator_request_ids(stale, 1, &[EntityId(0x80000804)]),
        Err(GeneratorServiceError::Stale)
    );
    k.supply_generator_id(EntityId(0x80000804)).unwrap();
    k.retry_generator_request(first.intent.key).unwrap();
    let repeated = k.take_generator_request().unwrap();
    assert_eq!(repeated.entities, [first.entities[0], ids[0], ids[1]]);
    assert_eq!(repeated.intent, first.intent);
}

#[test]
fn request_redelivery_refreshes_current_container_slots_without_replacing_ids_or_rng() {
    let mut k = kernel();
    k.register_inventory_container(container(10, None)).unwrap();
    k.register_generator(Arc::new(definition(10, 8))).unwrap();
    let first = request(&mut k, 0x80000901);
    k.register_inventory_item(item(0x80000902, 10, false))
        .unwrap();
    k.register_inventory_container(container(0x80000903, None))
        .unwrap();
    k.register_inventory_item(item(0x80000903, 10, true))
        .unwrap();
    k.retry_generator_request(first.intent.key).unwrap();
    let next = k.take_generator_request().unwrap();
    assert_eq!(next.next_slots, Some((1, 1)));
    assert_eq!(next.entities, first.entities);
    assert_eq!(next.intent, first.intent);
}
#[test]
fn acquiring_generated_bag_promotes_every_descendant_in_one_exact_receipt() {
    let mut k = kernel();
    k.register_inventory_container(container(10, None)).unwrap();
    k.register_generator(Arc::new(definition(10, 8))).unwrap();
    let bag_request = request(&mut k, 0x80000201);
    let bag = item(bag_request.entities[0].0, 10, true);
    k.admit_generated_inventory(
        bag_request.intent.key,
        std::slice::from_ref(&bag),
        &[container(bag.id.0, None)],
    )
    .unwrap();
    let mut child_def = definition(bag.id.0, 8);
    child_def.parent = Some(EntityId(10));
    k.register_generator(Arc::new(child_def)).unwrap();
    let child_request = request(&mut k, 0x80000202);
    let child = item(child_request.entities[0].0, bag.id.0, false);
    k.admit_generated_inventory(child_request.intent.key, std::slice::from_ref(&child), &[])
        .unwrap();
    let ticket = move_to_player(&mut k, bag.id.0, 1);
    let mut ids = k.generated_inventory_items(ticket.operation).unwrap();
    ids.sort_unstable();
    assert_eq!(ids, vec![bag.id, child.id]);
    assert!(
        ticket
            .proposal
            .participants
            .iter()
            .any(|(id, revision)| *id == child.id && *revision == 1)
    );
    let exact = receipt(&ticket);
    assert!(
        k.confirm_generated_inventory_committed(&exact, &[bag.id])
            .is_err()
    );
    assert_eq!(k.inventory_item(child.id), Some(&child));
    k.confirm_generated_inventory_committed(&exact, &ids)
        .unwrap();
    assert_eq!(k.inventory_count(EntityId(1), 100), 2);
    assert_eq!(
        k.inventory_item(child.id).unwrap().revision,
        child.revision,
        "acquisition fences unchanged descendants without fabricating a mutation"
    );
    let operation = k
        .propose_inventory(
            context(2),
            InventoryRequest::Drop { item: bag.id },
            authority(),
        )
        .unwrap();
    assert!(
        k.generated_inventory_items(operation).unwrap().is_empty(),
        "durable children cannot be inserted a second time"
    );
    let drop = k.take_inventory_proposal().unwrap();
    k.confirm_inventory_committed(&receipt(&drop)).unwrap();
}
fn stock(stack: i32) -> Arc<WeenieV1> {
    Arc::new(WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "stock".into(),
        weenie_type: 1,
        last_modified: None,
        properties: bace_content::SparseProperties {
            ints: vec![
                Property { id: 11, value: 100 },
                Property {
                    id: 12,
                    value: stack,
                },
            ],
            ..Default::default()
        },
    })
}
#[test]
fn vendor_merges_track_retained_identity_and_sum_only_source_contributions() {
    let mut k = kernel();
    let mut d = definition(20, 32);
    d.initial_count = 2;
    d.maximum_count = 2;
    d.profiles[0].max_create = -1;
    k.register_generator_vendor(EntityId(20), 10).unwrap();
    k.register_generator(Arc::new(d.clone())).unwrap();
    let first = request(&mut k, 0x80000301);
    let retained = first.entities[0];
    k.admit_generated_vendor_stock(first.intent.key, &[(retained, stock(5))])
        .unwrap();
    assert!(
        k.supply_generator_id(retained).is_err(),
        "live vendor stock ID must remain reserved"
    );
    let second = request(&mut k, 0x80000302);
    let incoming = second.entities[0];
    k.admit_generated_vendor_stock(second.intent.key, &[(incoming, stock(99))])
        .unwrap();
    let items = k.generated_vendor_stock(EntityId(20)).unwrap().items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, retained.0);
    assert_eq!(items[0].stack, Some(6));
    assert!(k.generated_vendor_item(EntityId(20), incoming.0).is_none());
    k.generator_control(d.identity, GeneratorControl::Destroy)
        .unwrap();
    assert!(
        matches!(k.take_generator_lifecycle(),Some(GeneratorLifecycleEffect::DestroyMember{member:GeneratorSpawnMember{entity,contribution:6},..}) if entity==retained)
    );
    k.step().unwrap();
    assert!(
        k.generated_vendor_stock(EntityId(20))
            .unwrap()
            .items()
            .is_empty()
    );
}
#[test]
fn wrong_or_duplicate_vendor_admission_cannot_change_stock_or_discard_request() {
    let mut k = kernel();
    k.register_generator_vendor(EntityId(20), 10).unwrap();
    k.register_generator(Arc::new(definition(20, 32))).unwrap();
    let pending = request(&mut k, 0x80000401);
    assert_eq!(
        k.admit_generated_vendor_stock(pending.intent.key, &[(EntityId(0x80009999), stock(1))]),
        Err(GeneratorServiceError::Invalid)
    );
    assert!(
        k.generated_vendor_stock(EntityId(20))
            .unwrap()
            .items()
            .is_empty()
    );
    k.retry_generator_request(pending.intent.key).unwrap();
    assert_eq!(k.take_generator_request(), Some(pending.clone()));
    k.admit_generated_vendor_stock(pending.intent.key, &[(pending.entities[0], stock(1))])
        .unwrap();
    assert!(
        k.admit_generated_vendor_stock(pending.intent.key, &[(pending.entities[0], stock(1))])
            .is_err()
    );
    assert_eq!(
        k.generated_vendor_stock(EntityId(20))
            .unwrap()
            .items()
            .len(),
        1
    );
}

fn durable_generated_remainder() -> (Kernel, GeneratorIdentity, EntityId) {
    let mut k = kernel();
    k.register_inventory_container(container(30, None)).unwrap();
    let definition = definition(30, 8);
    let identity = definition.identity;
    k.register_generator(Arc::new(definition)).unwrap();
    let source = EntityId(0x80000401);
    let request = request(&mut k, source.0);
    let mut stack = item(source.0, 30, false);
    stack.stack = 10;
    stack.maximum_stack = 100;
    k.admit_generated_inventory(request.intent.key, &[stack], &[])
        .unwrap();
    let mut authorization = authority();
    authorization.new_item = Some(EntityId(0x80000402));
    k.propose_stack_split(
        context(1),
        InventoryRequest::SplitToContainer {
            item: source,
            container: EntityId(1),
            placement: 0,
            amount: 3,
        },
        authorization,
        bace_inventory::StackSplitPreparation {
            fresh: bace_inventory::InventoryItem {
                id: EntityId(0x80000402),
                revision: 0,
                ..k.inventory_item(source).unwrap().clone()
            },
            source_stackable: true,
            source_stuck: false,
            source_vendor: false,
            destination_corpse: false,
        },
    )
    .unwrap();
    let ticket = k.take_inventory_proposal().unwrap();
    assert_eq!(
        k.generated_inventory_items(ticket.operation).unwrap(),
        vec![source]
    );
    k.confirm_generated_inventory_committed(&receipt(&ticket), &[source])
        .unwrap();
    assert_eq!(k.inventory_item(source).unwrap().stack, 7);
    assert_eq!(k.inventory_count(EntityId(1), 100), 3);
    (k, identity, source)
}

#[test]
fn durable_generated_remainder_is_retained_until_exact_epoch_fenced_tombstone_receipt() {
    let (mut k, identity, source) = durable_generated_remainder();
    k.generator_control(identity, GeneratorControl::Destroy)
        .unwrap();
    k.step().unwrap();
    let ticket = k
        .take_generated_retirement()
        .expect("durable remainder tombstone");
    assert!(ticket.transient.is_empty());
    assert_eq!(ticket.inventory.proposal.changes.len(), 1);
    assert_eq!(
        ticket.inventory.proposal.changes[0].after.place,
        ItemPlace::Removed
    );
    assert!(k.take_inventory_proposal().is_none());
    assert!(k.take_generated_retirement().is_none());
    let exact = receipt(&ticket.inventory);
    assert_eq!(
        k.confirm_inventory_committed(&exact),
        Err(InventoryRejection::DurabilityPending)
    );
    assert_eq!(
        k.retry_inventory(exact.operation),
        Err(InventoryRejection::DurabilityPending)
    );
    assert_eq!(
        k.reject_inventory(exact.operation),
        Err(InventoryRejection::DurabilityPending)
    );
    let mut wrong = exact.clone();
    wrong.revisions[0].1 += 1;
    assert!(k.confirm_generated_retirement(&wrong).is_err());
    for _ in 0..40 {
        k.step().unwrap();
    }
    assert_eq!(k.inventory_item(source).unwrap().stack, 7);
    assert!(k.generator_inventory_busy(identity));
    k.retry_generated_retirement(exact.operation).unwrap();
    assert_eq!(k.take_generated_retirement(), Some(ticket.clone()));
    assert_eq!(k.confirm_generated_retirement(&exact).unwrap(), ticket);
    assert!(k.inventory_item(source).is_none());
    assert_eq!(k.inventory_count(EntityId(1), 100), 3);
    assert!(k.confirm_generated_retirement(&exact).is_err());
    k.step().unwrap();
    assert!(!k.generator_inventory_busy(identity));
}

#[test]
fn definite_retirement_rollback_keeps_member_and_requeues_new_operation() {
    let (mut k, identity, source) = durable_generated_remainder();
    k.generator_control(identity, GeneratorControl::Destroy)
        .unwrap();
    k.step().unwrap();
    let first = k.take_generated_retirement().unwrap();
    k.reject_generated_retirement(first.inventory.operation)
        .unwrap();
    assert_eq!(k.inventory_item(source).unwrap().stack, 7);
    k.step().unwrap();
    let second = k.take_generated_retirement().unwrap();
    assert_ne!(first.inventory.operation, second.inventory.operation);
    assert_eq!(first.inventory.proposal, second.inventory.proposal);
    assert!(
        k.confirm_generated_retirement(&receipt(&first.inventory))
            .is_err()
    );
    k.confirm_generated_retirement(&receipt(&second.inventory))
        .unwrap();
    assert!(k.inventory_item(source).is_none());
}

#[test]
fn retirement_tombstone_and_unrelated_container_slot_shift_commit_together() {
    let (mut k, identity, source) = durable_generated_remainder();
    let sibling = EntityId(0x80000403);
    let mut item = item(sibling.0, 30, false);
    item.place = ItemPlace::Contained {
        container: EntityId(30),
        slot: 1,
        equipped: 0,
    };
    k.register_inventory_item(item).unwrap();
    let entry = bace_magic::EnchantmentEntry {
        spell: 123,
        caster: 100,
        school: bace_magic::MagicSchool::Creature,
        spec: bace_magic::EnchantmentSpec {
            category: 23,
            power: 100,
            duration: -1.,
            layer: 7,
            stat_type: 0x2008000,
            stat_key: 7,
            value: 12.5,
            beneficial: true,
            set_id: None,
        },
        start_time: -15.,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: Default::default(),
    };
    k.register_magic_registry(
        sibling,
        bace_magic::EnchantmentRegistry::restore(16, 20, vec![entry.clone()]).unwrap(),
        false,
    )
    .unwrap();
    k.generator_control(identity, GeneratorControl::Destroy)
        .unwrap();
    k.step().unwrap();
    let ticket = k.take_generated_retirement().unwrap();
    assert_eq!(ticket.inventory.proposal.changes.len(), 2);
    assert_eq!(
        ticket.enchantments[&sibling].as_slice(),
        std::slice::from_ref(&entry)
    );
    assert_eq!(ticket.registry_revisions[&sibling], 20);
    assert!(
        k.set_magic_registry_active(sibling, true).is_err(),
        "frozen registry remains reserved"
    );
    let mut wrong = receipt(&ticket.inventory);
    wrong.revisions.retain(|(id, _)| *id != sibling);
    assert!(k.confirm_generated_retirement(&wrong).is_err());
    assert!(matches!(
        k.inventory_item(sibling).unwrap().place,
        ItemPlace::Contained { slot: 1, .. }
    ));
    assert!(k.inventory_item(source).is_some());
    k.confirm_generated_retirement(&receipt(&ticket.inventory))
        .unwrap();
    assert!(k.inventory_item(source).is_none());
    assert!(matches!(
        k.inventory_item(sibling).unwrap().place,
        ItemPlace::Contained { slot: 0, .. }
    ));
    assert_eq!(k.inventory_item(sibling).unwrap().revision, 2);
    assert_eq!(k.magic_registry(sibling).unwrap().entries(), &[entry]);
}

#[test]
fn later_pickup_of_durable_generated_remainder_releases_its_generator_slot() {
    let (mut k, identity, source) = durable_generated_remainder();
    let ticket = move_to_player(&mut k, source.0, 2);
    assert!(
        k.generated_inventory_items(ticket.operation)
            .unwrap()
            .is_empty()
    );
    k.supply_generator_id(EntityId(0x80000409)).unwrap();
    for _ in 0..40 {
        k.step().unwrap();
    }
    assert!(k.take_generator_request().is_none());
    assert!(k.generator_inventory_busy(identity));
    k.confirm_inventory_committed(&receipt(&ticket)).unwrap();
    assert_eq!(k.inventory_count(EntityId(1), 100), 10);
    for _ in 0..40 {
        k.step().unwrap();
    }
    let respawn = k
        .take_generator_request()
        .expect("durable generated remainder pickup releases original profile slot");
    assert_eq!(respawn.entities, vec![EntityId(0x80000409)]);
}

#[test]
fn exact_inventory_tombstone_retires_empty_bag_container_metadata() {
    let mut k = kernel();
    let bag = EntityId(0x80000410);
    k.register_inventory_container(container(bag.0, None))
        .unwrap();
    k.register_inventory_item(item(bag.0, 1, true)).unwrap();
    k.propose_item_take(EntityId(1), bag, 1).unwrap();
    let ticket = k.take_inventory_proposal().unwrap();
    assert!(k.inventory_container(bag).is_some());
    let mut wrong = receipt(&ticket);
    wrong.revisions[0].1 += 1;
    assert!(k.confirm_inventory_committed(&wrong).is_err());
    assert!(k.inventory_container(bag).is_some());
    k.confirm_inventory_committed(&receipt(&ticket)).unwrap();
    assert!(k.inventory_item(bag).is_none());
    assert!(k.inventory_container(bag).is_none());
}

#[test]
fn correlated_request_refresh_never_requeues_behind_other_generators() {
    use bace_simulation::{Command, GeneratorAction, GeneratorCommand};
    let mut k = kernel();
    k.register_inventory_container(container(10, None)).unwrap();
    k.register_generator(Arc::new(definition(10, 8))).unwrap();
    let original = request(&mut k, 0x80000e01);
    let child = EntityId(0x80000e02);
    for (correlation, action) in [
        (
            71,
            GeneratorAction::SupplyRequestIds {
                key: original.intent.key,
                expected: 1,
                ids: vec![child],
            },
        ),
        (72, GeneratorAction::RefreshRequest(original.intent.key)),
    ] {
        k.enqueue(Command::Generator(GeneratorCommand {
            correlation,
            action,
        }))
        .unwrap();
        k.step().unwrap();
        let outcome = k.take_generator_outcome().unwrap();
        assert_eq!(outcome.correlation, correlation);
        assert_eq!(outcome.result, Ok(()));
        let refreshed = outcome.request.unwrap();
        assert_eq!(refreshed.entities, [original.entities[0], child]);
        assert_eq!(refreshed.intent, original.intent);
        assert_eq!(refreshed.next_slots, Some((0, 0)));
        assert!(k.take_generator_request().is_none());
    }
}
