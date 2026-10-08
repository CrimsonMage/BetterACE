//! A delayed owner pass at a registry heartbeat must fence the post-heartbeat aggregate.
use crate::kernel::magic_components_fixture as fixture;
use bace_entity::EntityVital;
use bace_gameplay_api::CastRequest;
use bace_magic::{MagicSchool, SpellEffect};
use bace_types::EntityId;
use fixture::{component_kernel, context, register_component_caster, spell};
#[test]
fn component_fence_captures_registry_heartbeat_before_prepared_revision() {
    let mut k = component_kernel(64);
    k.register_inventory_container(bace_inventory::InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 10,
        pack_slots: 0,
        burden_limit: 100,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(bace_inventory::InventoryItem {
        id: EntityId(20),
        revision: 1,
        template: 500,
        stack_key: 1,
        place: bace_inventory::ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 10,
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
        wield_requirements_met: true,
        structure: None,
    })
    .unwrap();
    assert!(k.magic.registry(EntityId(1)).is_none());
    k.register_magic_registry(
        EntityId(1),
        bace_magic::EnchantmentRegistry::restore(
            16,
            1,
            vec![bace_magic::EnchantmentEntry {
                spell: 777,
                caster: 1,
                school: MagicSchool::Life,
                spec: bace_magic::EnchantmentSpec {
                    category: 10,
                    power: 1,
                    duration: 60.0,
                    layer: 1,
                    stat_type: 0x1000,
                    stat_key: 1,
                    value: 1.0,
                    beneficial: true,
                    set_id: None,
                },
                start_time: 0.0,
                is_set_spell: false,
                is_level8_aura: false,
                metadata: Default::default(),
            }],
        )
        .unwrap(),
        true,
    )
    .unwrap();
    register_component_caster(&mut k);
    let mut s = spell(
        100,
        SpellEffect::Boost {
            vital: bace_magic::Vital::Health,
            minimum: 10,
            maximum: 10,
        },
    );
    // Component burn probability is proportional to spell power; zero power
    // would exercise the local no-burn path instead of the durable fence.
    s.spell.power = 1;
    s.components = vec![(500, 1)];
    s.component_modifiers = vec![(500, 1.0)];
    s.component_loss = 10000.0;
    k.register_magic_spell(s).unwrap();
    k.magic
        .apply(
            context(1),
            CastRequest::Targeted {
                target: EntityId(1),
                spell: 100,
            },
            &mut k.world,
            0.0,
            &k.combat,
            &k.fellowships,
            None,
        )
        .unwrap();
    // Direct owner passes intentionally leave the inventory handoff pending,
    // modeling a delayed stage without advancing the registry to the new time.
    for i in 1..=10 {
        k.magic.step(
            &mut k.world,
            f64::from(i) / 30.0,
            &k.combat,
            &k.fellowships,
            None,
        );
    }
    assert_eq!(
        k.magic.pending_components().unwrap().consumed,
        vec![(500, 1)]
    );
    while k.magic.take_event().is_some() {}
    let cast = k.magic.pending_components().unwrap().cast;
    let before = k.characters.get(EntityId(1)).unwrap().revision();
    k.tick = 150;
    k.service_magic_components();
    assert!(
        k.take_inventory_proposal().is_none(),
        "named magic lane owns burned components"
    );
    let operation = k.magic.component_operation(EntityId(1), cast).unwrap();
    let ticket = k.inventory.pending_ticket(operation).unwrap().clone();
    let resources = k.pending_magic_resources(ticket.operation).unwrap();
    assert_eq!(
        k.magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        -5.0
    );
    assert!(resources.before_revision > before);
    assert_eq!(
        resources.before_revision,
        k.characters.get(EntityId(1)).unwrap().revision()
    );
    let health = bace_entity::VitalMutation {
        actor: EntityId(1),
        vital: EntityVital::Health,
        before: 50,
        after: 49,
    };
    let mana = bace_entity::VitalMutation {
        actor: EntityId(1),
        vital: EntityVital::Mana,
        before: 100,
        after: 99,
    };
    assert!(k.world.apply_vital_batch(&[health, mana], None).is_err());
    assert_eq!(
        k.world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    k.world.apply_vital_batch(&[health], None).unwrap();
    k.characters.touch_auxiliary(EntityId(1)).unwrap();
    let newer_revision = k.characters.get(EntityId(1)).unwrap().revision();
    let receipt = crate::InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert!(
        k.confirm_inventory_committed(&receipt).is_err(),
        "generic lane cannot adopt magic receipt"
    );
    k.confirm_inventory_committed_inner(&receipt).unwrap();
    assert!(k.characters.get(EntityId(1)).unwrap().revision() > newer_revision);
    assert_eq!(
        k.world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        49
    );
    assert!(!k.world.has_vital_reservations());
    assert_eq!(
        k.world
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
}
