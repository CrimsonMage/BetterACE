use super::*;
use bace_gameplay_api::{physical_procs::*, weapon_combat::PhysicalKind};
fn key() -> PhysicalHitKey {
    PhysicalHitKey {
        attacker: EntityId(2),
        operation: 1,
        ordinal: 0,
        target: EntityId(1),
        kind: PhysicalKind::Melee,
    }
}
#[test]
fn weapon_terminal_precedes_live_sigils_and_receipt_is_exact() {
    let mut magic = Magic::new(4);
    let world = super::periodic_tests::world(true, 100);
    magic
        .damage_profiles
        .insert(EntityId(2), bace_magic::MagicDamageProfile::neutral(true));
    let request = PhysicalProcRequest::Attack {
        key: key(),
        weapon: Some((EntityId(10), 123)),
        sigil_target: Some(EntityId(1)),
        sigil_rolls: [0.; 3],
    };
    assert!(
        magic
            .advance_physical_proc(&request, &world)
            .unwrap()
            .is_none()
    );
    let weapon = magic.pending_item_proc_excluding(&[]).unwrap();
    assert_eq!(weapon.spell, 123);
    // A source weapon effect can alter equipped/active sigils before selection.
    magic
        .damage_profiles
        .get_mut(&EntityId(2))
        .unwrap()
        .sigils
        .push(bace_magic::MagicSigil {
            item: 11,
            slot: 0,
            level: 5,
            spell: 5208,
        });
    magic.damage_spell_flags.insert(5208, 8);
    assert!(
        magic
            .advance_physical_proc(&request, &world)
            .unwrap()
            .is_none()
    );
    assert_eq!(magic.item_procs.len(), 1);
    magic.confirm_item_proc(weapon).unwrap();
    assert!(
        magic
            .advance_physical_proc(&request, &world)
            .unwrap()
            .is_none()
    );
    let sigil = magic.pending_item_proc_excluding(&[]).unwrap();
    assert_eq!(sigil.target, EntityId(2));
    magic.confirm_item_proc(sigil).unwrap();
    let receipt = magic
        .advance_physical_proc(&request, &world)
        .unwrap()
        .unwrap();
    assert_eq!(
        magic.advance_physical_proc(&request, &world).unwrap(),
        Some(receipt)
    );
    let mut wrong = receipt;
    wrong.damage = Some(7);
    assert!(magic.acknowledge_physical_proc(&wrong).is_err());
    magic.acknowledge_physical_proc(&receipt).unwrap();
    assert!(magic.completed_item_procs.is_empty());
}
#[test]
fn unarmed_dirty_retains_actual_actor_origin_and_malformed_phase_is_atomic() {
    let mut magic = Magic::new(1);
    let world = super::periodic_tests::world(true, 100);
    let malformed = PhysicalProcRequest::Dirty {
        key: key(),
        weapon: None,
        spells: [1, 2],
        count: 3,
    };
    assert!(magic.advance_physical_proc(&malformed, &world).is_err());
    assert!(magic.item_procs.is_empty());
    let request = PhysicalProcRequest::Dirty {
        key: key(),
        weapon: None,
        spells: [1, 2],
        count: 2,
    };
    assert!(
        magic
            .advance_physical_proc(&request, &world)
            .unwrap()
            .is_none()
    );
    for _ in 0..2 {
        let proc = magic.pending_item_proc_excluding(&[]).unwrap();
        assert!(matches!(
            proc.origin,
            CastOrigin::PhysicalProc {
                actor: EntityId(2),
                ..
            }
        ));
        magic.confirm_item_proc(proc).unwrap();
    }
    let receipt = magic
        .advance_physical_proc(&request, &world)
        .unwrap()
        .unwrap();
    magic.acknowledge_physical_proc(&receipt).unwrap();
}
#[test]
fn cloak_threshold_reads_current_health_and_small_capacity_plain_hit_completes() {
    let mut magic = Magic::new(1);
    let world = super::periodic_tests::world(true, 40);
    let mut target = bace_magic::MagicDamageProfile::neutral(true);
    target.cloak = Some(bace_magic::MagicCloak {
        item: 10,
        level: 1,
        effect: bace_magic::MagicCloakEffect::Absorb,
    });
    magic.damage_profiles.insert(EntityId(1), target);
    let request = PhysicalProcRequest::Cloak {
        key: key(),
        damage: 40,
        roll: 0.10,
    };
    let receipt = magic
        .advance_physical_proc(&request, &world)
        .unwrap()
        .unwrap();
    assert_eq!(
        receipt.damage,
        Some(0),
        "40/current40 earns native half tier; 40/max100 would not"
    );
    magic.acknowledge_physical_proc(&receipt).unwrap();
    magic.damage_profiles.clear();
    let receipt = magic
        .advance_physical_proc(&request, &world)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.damage, Some(40));
}
#[test]
fn missile_dirty_retains_try_begin_cast_zero_skill_override() {
    let mut magic = Magic::new(4);
    let world = super::periodic_tests::world(true, 100);
    let mut key = key();
    key.kind = PhysicalKind::Missile;
    let request = PhysicalProcRequest::Dirty {
        key,
        weapon: Some(EntityId(10)),
        spells: [5938, 5941],
        count: 2,
    };
    assert!(
        magic
            .advance_physical_proc(&request, &world)
            .unwrap()
            .is_none()
    );
    for proc in &magic.item_procs {
        assert_eq!(proc.skill_override, Some(0));
        let spell = PreparedSpell {
            id: proc.spell,
            school: bace_magic::MagicSchool::Creature,
            power: 1,
            base_mana: 0,
            range_constant: 5.,
            range_per_skill: 0.25,
            harmful: true,
            resistable: true,
            effect: SpellEffect::Boost {
                vital: Vital::Health,
                minimum: -1,
                maximum: -1,
            },
        };
        magic.spell_categories.insert(proc.spell, 684);
        assert_eq!(
            magic
                .effective_cast_skill(proc.origin, &spell, &world)
                .unwrap(),
            0,
            "TryBeginCast overrides earlier Resolve category1"
        );
    }
}
