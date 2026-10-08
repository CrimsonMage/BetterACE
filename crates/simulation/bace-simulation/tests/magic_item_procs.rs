#[allow(
    dead_code,
    unused_imports,
    reason = "shared fixture also supplies client-command helpers"
)]
mod magic_common;
use bace_gameplay_api::CastOrigin;
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_magic::{MagicCloak, MagicCloakEffect, MagicDamageProfile, MagicWand};
use magic_common::*;
fn fixture(equipped: bool, nested: bool) -> Kernel {
    fixture_with_launch(equipped, nested, true)
}
fn fixture_with_launch(equipped: bool, nested: bool, launch: bool) -> Kernel {
    let mut k = kernel(64);
    k.register_inventory_container(InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 10,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(InventoryItem {
        structure: None,
        id: EntityId(900),
        revision: 1,
        template: 900,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: if equipped { 0x08000000 } else { 0 },
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 0,
        unit_value: 0,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0x08000000,
        incompatible_wield: 0,
        wield_requirements_met: true,
    })
    .unwrap();
    let mut target = MagicDamageProfile::neutral(true);
    // Synthetic maximum-level fixture forces the probability branch; original
    // supported level1/5 probability thresholds are independently covered.
    target.cloak = Some(MagicCloak {
        item: 900,
        level: 100,
        effect: MagicCloakEffect::Spell(101),
    });
    target.proc_items.push(bace_magic::MagicProcItem {
        item: 900,
        spellcraft: Some(1000),
        skills: [0; 5],
        cloak: true,
        wield_difficulty: Some(300),
    });
    k.register_magic_damage_profile(EntityId(1), target)
        .unwrap();
    let mut source = MagicDamageProfile::neutral(false);
    source.wand = Some(MagicWand {
        entity: 800,
        revision: 1,
        damage_type: 16,
        elemental_modifier: 1.,
        elemental_present: true,
        inherit_wielder: false,
        imbues: 0,
        biting: 1.,
        double_enchant_biting: true,
        crushing: 0.,
        double_enchant_crushing: true,
        slayer_type: 0,
        slayer_bonus: 0.,
        resistance_cleaving: None,
        ignore_magic_resistance: false,
    });
    k.register_magic_damage_profile(EntityId(2), source)
        .unwrap();
    let mut spec = shot(ProjectileShape::Bolt, 1);
    spec.minimum_damage = 4;
    spec.maximum_damage = 4;
    k.register_magic_spell(spell(100, SpellEffect::Projectile(spec)))
        .unwrap();
    k.register_magic_damage_spell(100, 4).unwrap();
    k.register_magic_spell(spell(
        101,
        SpellEffect::Boost {
            vital: bace_magic::Vital::Health,
            minimum: if nested { -1 } else { 10 },
            maximum: if nested { -1 } else { 10 },
        },
    ))
    .unwrap();
    if nested {
        k.register_magic_damage_spell(101, 4).unwrap();
    }
    k.supply_projectile_id(EntityId(1000)).unwrap();
    if launch {
        k.cast_from_server(
            CastOrigin::Emote {
                actor: EntityId(2),
                event: 99,
                instant: true,
            },
            CastRequest::Targeted {
                target: EntityId(1),
                spell: 100,
            },
        )
        .unwrap();
    }
    k
}
#[test]
fn cloak_cast_completion_precedes_damage_and_generic_outcome_drain_cannot_steal_it() {
    let mut k = fixture(true, false);
    let mut values = Vec::new();
    for _ in 0..35 {
        for event in step(&mut k) {
            if let MagicEvent::Vital {
                actor: EntityId(1),
                after,
                ..
            } = event
            {
                values.push(after);
            }
        }
        while k.take_server_cast_outcome().is_some() {}
    }
    assert_eq!(values, vec![60, 57]);
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        57
    );
}
#[test]
fn unequipped_proc_item_is_rejected_and_retained_damage_still_applies_once() {
    let mut k = fixture(false, false);
    let mut values = Vec::new();
    for _ in 0..35 {
        for event in step(&mut k) {
            if let MagicEvent::Vital {
                actor: EntityId(1),
                after,
                ..
            } = event
            {
                values.push(after);
            }
        }
    }
    assert_eq!(values, vec![47]);
}

#[test]
fn nested_harmful_cloak_chain_has_bounded_depth_and_unwinds_without_deadlock() {
    let mut k = fixture(true, true);
    let mut rejections = 0;
    let mut hits = 0;
    for _ in 0..100 {
        for event in step(&mut k) {
            match event {
                MagicEvent::TargetRejected {
                    reason: bace_gameplay_api::CastRejection::Capacity,
                    ..
                } => rejections += 1,
                MagicEvent::Vital {
                    actor: EntityId(1), ..
                } => hits += 1,
                _ => {}
            }
        }
        while k.take_server_cast_outcome().is_some() {}
    }
    assert_eq!(rejections, 1);
    assert_eq!(hits, 17);
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        31
    );
    for _ in 0..30 {
        assert!(!step(&mut k).iter().any(|e| matches!(
            e,
            MagicEvent::Vital {
                actor: EntityId(1),
                ..
            }
        )));
    }
}
#[test]
fn reflected_cloak_killing_caster_cannot_discard_the_retained_direct_hit() {
    let mut kernel = fixture_with_launch(true, false, false);
    let mut target = MagicDamageProfile::neutral(true);
    target.current_enemy = Some(2);
    target.cloak = Some(MagicCloak {
        item: 900,
        level: 100,
        effect: MagicCloakEffect::Spell(5754),
    });
    target.proc_items.push(bace_magic::MagicProcItem {
        item: 900,
        spellcraft: Some(1000),
        skills: [0; 5],
        cloak: true,
        wield_difficulty: Some(300),
    });
    kernel
        .register_magic_damage_profile(EntityId(1), target)
        .unwrap();
    for (id, damage) in [(102, -4), (5754, -1000)] {
        kernel
            .register_magic_spell(spell(
                id,
                SpellEffect::Boost {
                    vital: bace_magic::Vital::Health,
                    minimum: damage,
                    maximum: damage,
                },
            ))
            .unwrap();
        kernel.register_magic_damage_spell(id, 4).unwrap();
    }
    let origin = CastOrigin::Emote {
        actor: EntityId(2),
        event: 201,
        instant: false,
    };
    kernel
        .cast_from_server(
            origin,
            CastRequest::Targeted {
                target: EntityId(1),
                spell: 102,
            },
        )
        .unwrap();
    assert_eq!(
        kernel
            .world()
            .vital(EntityId(2), EntityVital::Mana)
            .unwrap()
            .current,
        90,
        "source cost precedes cloak"
    );
    let mut target_hits = 0;
    let mut completed = false;
    for _ in 0..45 {
        for event in step(&mut kernel) {
            if let MagicEvent::Vital {
                actor: EntityId(1),
                vital: EntityVital::Health,
                before,
                after,
                ..
            } = event
                && after < before
            {
                target_hits += 1;
            }
        }
        while let Some(outcome) = kernel.take_server_cast_outcome() {
            if outcome.origin == origin {
                assert!(
                    matches!(
                        outcome.result,
                        Ok(bace_gameplay_api::CastChange::Completed { .. })
                    ),
                    "{outcome:?}"
                );
                completed = true;
            }
        }
    }
    assert_eq!(
        kernel
            .world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        0
    );
    assert!(
        kernel
            .world()
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current
            < 50
    );
    assert_eq!(target_hits, 1);
    assert!(completed);
}
