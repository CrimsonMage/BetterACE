use super::*;
use bace_character::{
    CharacterServiceState, ProgressionTables, RankTable, TraitProgress, TraitState, VitalFormula,
};
use bace_entity::{Actor, Combatant, CombatantProfile, VitalPool};
use bace_gameplay_api::{AttributeId, ProgressionTarget, SkillAdvancement, TraitDetails, VitalId};
use bace_geometry::Aabb;
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use std::sync::Arc;
const ACTOR: EntityId = EntityId(0x50000001);
fn body(world: &World, id: EntityId) -> Actor {
    Actor {
        id,
        cell: CellId(1),
        body: Body::spawn(
            world.scene(CellId(1)).unwrap(),
            Vec3::new(0., 0., 0.5),
            0.5,
            Capabilities {
                speed: 5.,
                jump_impulse: 5.,
            },
        )
        .unwrap(),
    }
}
fn item(id: u32, place: ItemPlace, container: bool) -> InventoryItem {
    InventoryItem {
        id: EntityId(id),
        revision: 0,
        template: 100 + id % 100,
        stack_key: u64::from(id),
        place,
        stack: 1,
        maximum_stack: 10,
        unit_burden: 1,
        unit_value: 100,
        pack_slot: container,
        is_container: container,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
        structure: None,
    }
}
fn container(id: EntityId, owner: Option<EntityId>) -> InventoryContainer {
    InventoryContainer {
        id,
        revision: 0,
        root_owner: owner,
        slots: 40,
        pack_slots: 10,
        burden_limit: 100000,
        accessible: true,
        open: true,
        generation: 1,
    }
}
fn entry(spell: u32, duration: f64) -> EnchantmentEntry {
    EnchantmentEntry {
        spell,
        caster: ACTOR.0,
        school: MagicSchool::Life,
        spec: EnchantmentSpec {
            category: if spell == 666 { 204 } else { 1 },
            power: 1,
            duration,
            layer: 1,
            stat_type: 0,
            stat_key: 0,
            value: 0.95,
            beneficial: true,
            set_id: None,
        },
        start_time: 0.,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: EnchantmentMetadata::default(),
    }
}
pub(super) fn fixture() -> Kernel {
    fixture_with_entry(None)
}

#[test]
fn accepted_blow_freezes_source_death_text_and_keeps_unresolved_blow_explicit() {
    let mut k = fixture();
    let victim = bace_entity::EntityProperties::restore_snapshot(
        1,
        vec![(
            bace_entity::PropertyFamily::String,
            1,
            bace_entity::PropertyValue::String("Victim".into()),
        )],
    )
    .unwrap();
    k.world.register_properties(ACTOR, victim).unwrap();
    assert_eq!(k.death_announcement(ACTOR, None, None, 1).unwrap(), None);
    let unattended = k
        .death_announcement(
            ACTOR,
            None,
            Some(crate::DeathBlow {
                damage_type: 0,
                critical: false,
            }),
            1,
        )
        .unwrap()
        .unwrap();
    assert_eq!(unattended.victim_text, "You died!");
    assert_eq!(unattended.broadcast_text, "Victim died!");
    assert_eq!(unattended.killer_text, None);
    let attacker = EntityId(0x50000002);
    k.world.insert(body(&k.world, attacker)).unwrap();
    let name = bace_entity::EntityProperties::restore_snapshot(
        1,
        vec![(
            bace_entity::PropertyFamily::String,
            1,
            bace_entity::PropertyValue::String("Damager".into()),
        )],
    )
    .unwrap();
    k.world.register_properties(attacker, name).unwrap();
    let first = k
        .death_announcement(
            ACTOR,
            Some(attacker),
            Some(crate::DeathBlow {
                damage_type: 1,
                critical: false,
            }),
            9,
        )
        .unwrap()
        .unwrap();
    let repeat = k
        .death_announcement(
            ACTOR,
            Some(attacker),
            Some(crate::DeathBlow {
                damage_type: 1,
                critical: false,
            }),
            9,
        )
        .unwrap()
        .unwrap();
    assert_eq!(first, repeat);
    assert_eq!(first.last_damager, Some(attacker));
    assert!(first.victim_text.contains("Damager"));
    assert!(first.broadcast_text.contains("Victim"));
}

#[test]
fn pending_recall_blocks_crafting_and_skill_device_before_sequence_admission() {
    let mut k = fixture();
    let context = bace_gameplay_api::ActionContext {
        session: bace_gameplay_api::SessionId(1),
        account: bace_types::AccountId(1),
        actor: ACTOR,
        sequence: 77,
    };
    let revision = k.characters.get(ACTOR).unwrap().revision();
    k.recalls.pending_bindings.insert(
        ACTOR,
        crate::recalls::PendingBinding {
            context,
            object: EntityId(99),
            due: 100,
            motion_owner: 0,
            start_epoch: 0,
            completed: None,
            action_sequence: 1,
            action_started: true,
            action_announced: true,
        },
    );
    assert_eq!(
        k.authorize_crafting(context, ACTOR.0, revision),
        Err(bace_crafting::CraftError::Busy)
    );
    assert_eq!(
        k.check_device_busy(ACTOR),
        Err(crate::SkillDeviceError::Busy)
    );
    k.recalls.pending_bindings.remove(&ACTOR);
    k.authorize_crafting(context, ACTOR.0, revision).unwrap();
    assert!(k.check_device_busy(ACTOR).is_ok());
}
fn fixture_with_entry(extra: Option<EnchantmentEntry>) -> Kernel {
    let mut world = World::default();
    world
        .register_scene(
            CellId(1),
            SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-100., -100., -1.), Vec3::new(100., 100., 100.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    world.insert(body(&world, ACTOR)).unwrap();
    world
        .register_combatant(
            ACTOR,
            Combatant::new(CombatantProfile {
                maximum_health: 100,
                melee_damage: 1,
                melee_range: 2.,
                attack_duration: 1.,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap()
            .with_resources(
                Some(VitalPool {
                    current: 100,
                    maximum: 100,
                }),
                Some(VitalPool {
                    current: 100,
                    maximum: 100,
                }),
            )
            .unwrap(),
        )
        .unwrap();
    let mut k = Kernel::with_gameplay_limits(world, 32, 4, 64).unwrap();
    let table = RankTable::new(&[0, 100]).unwrap();
    let tables = Arc::new(ProgressionTables {
        attributes: table.clone(),
        vitals: table.clone(),
        trained_skills: table.clone(),
        specialized_skills: table,
    });
    let mut states = Vec::new();
    for id in 1..=6 {
        states.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Attribute(AttributeId::try_from(id).unwrap()),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Attribute {
                starting_value: 100,
            },
        });
    }
    for id in [1, 3, 5] {
        states.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Vital(VitalId::try_from(id).unwrap()),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Vital {
                starting_value: 100,
                current: 100,
            },
        });
    }
    k.register_character(
        CharacterBinding {
            session: bace_gameplay_api::SessionId(1),
            account: bace_types::AccountId(1),
            actor: ACTOR,
        },
        CharacterProgression::with_state(&states, tables, 0, 0).unwrap(),
    )
    .unwrap();
    k.register_npc_character_services(
        ACTOR,
        CharacterServiceState {
            level: 200,
            total_experience: 0,
            titles: vec![],
            enlightenment: 0,
            sanctuary: None,
            total_skill_credits: None,
        },
        bace_quests::ContractRegistry::restore(vec![]).unwrap(),
    )
    .unwrap();
    k.register_player_death_state(ACTOR, PlayerDeathState::default())
        .unwrap();
    k.configure_player_death_random(
        Arc::new(bace_random::RandomRoot::new([9; 32], 1).unwrap()),
        11,
    )
    .unwrap();
    k.register_inventory_container(container(ACTOR, Some(ACTOR)))
        .unwrap();
    k.register_inventory_item(item(
        20,
        ItemPlace::Contained {
            container: ACTOR,
            slot: 0,
            equipped: 0,
        },
        false,
    ))
    .unwrap();
    let mut registry = EnchantmentRegistry::new(32).unwrap();
    registry.add(entry(100, 120.), 0., false).unwrap();
    if let Some(extra) = extra {
        let mut entries = registry.entries().to_vec();
        entries.push(extra);
        registry = EnchantmentRegistry::restore(32, registry.revision() + 1, entries).unwrap();
    }
    k.register_magic_registry(ACTOR, registry, true).unwrap();
    k.register_vitae_template(entry(666, -1.)).unwrap();
    k
}
pub(super) fn prepare(k: &mut Kernel) -> crate::PreparedPlayerDeath {
    prepare_with_killer(k, None)
}
fn prepare_with_killer(k: &mut Kernel, killer: Option<EntityId>) -> crate::PreparedPlayerDeath {
    k.world.combatant_mut(ACTOR).unwrap().damage(100).unwrap();
    k.begin_player_death(ACTOR, killer).unwrap();
    let crate::PlayerDeathEvent::Prepare { operation, .. } = k.take_player_death_event().unwrap()
    else {
        panic!()
    };
    crate::PreparedPlayerDeath {
        operation,
        actor: ACTOR,
        corpse: body(&k.world, EntityId(30)),
        corpse_item: item(30, ItemPlace::World, true),
        corpse_container: container(EntityId(30), None),
        possessions: vec![bace_interactions::DeathPossession {
            id: EntityId(20),
            template: 120,
            item_type: 1,
            value: 100,
            stack: 1,
            wielded: false,
            bonded: 0,
        }],
        fresh_stacks: vec![],
        coin_stacks: vec![],
        animation_seconds: 0.,
        equipped_health: vec![],
        vital_formulas: [VitalFormula {
            enabled: false,
            divisor: 0,
            attribute1: 0,
            attribute2: 0,
        }; 3],
        instantiation: Some(bace_interactions::PortalPosition {
            cell: 1,
            origin: [10., 10., 0.5],
            rotation: [1., 0., 0., 0.],
        }),
        olthoi: None,
    }
}
#[test]
fn whole_pyreal_stack_keeps_its_identity_and_first_corpse_slot() {
    let mut k = fixture();
    k.death_policy.destroy_pyreals = false;
    for id in [21, 22] {
        let mut coin = item(
            id,
            ItemPlace::Contained {
                container: ACTOR,
                slot: id - 20,
                equipped: 0,
            },
            false,
        );
        coin.template = 273;
        coin.stack = 2;
        k.register_inventory_item(coin).unwrap();
    }
    let mut prepared = prepare(&mut k);
    for id in [21, 22] {
        prepared
            .possessions
            .push(bace_interactions::DeathPossession {
                id: EntityId(id),
                template: 273,
                item_type: 1,
                value: 2,
                stack: 2,
                wielded: false,
                bonded: 0,
            });
    }
    k.prepare_player_death(prepared).map_err(|e| e.0).unwrap();
    let ticket = k.take_player_death_proposal().unwrap();
    let transcript = ticket.inventory_transcript.as_ref().unwrap();
    assert_eq!(transcript.coin_amount, 2);
    assert_eq!(transcript.coin_drops, vec![EntityId(21)]);
    assert_eq!(transcript.coin_sources.len(), 1);
    assert!(transcript.coin_sources[0].whole);
    assert_eq!(transcript.coin_sources[0].burden_after, 3);
    assert_eq!(transcript.coin_sources[0].coin_value_after, 2);
    let moved = ticket
        .inventory
        .proposal
        .changes
        .iter()
        .find(|change| change.after.id == EntityId(21))
        .unwrap();
    assert_eq!(moved.before.as_ref().unwrap().stack, 2);
    assert_eq!(moved.after.stack, 2);
    assert_eq!(
        moved.after.place,
        ItemPlace::Contained {
            container: ticket.corpse,
            slot: 0,
            equipped: 0,
        }
    );
    assert!(
        !ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|change| { change.before.is_none() && change.after.template == 273 })
    );
}
#[test]
fn destroyed_pyreals_keep_spend_receipts_but_no_corpse_coin_identity() {
    let mut k = fixture();
    k.death_policy.destroy_pyreals = true;
    for id in [21, 22] {
        let mut coin = item(
            id,
            ItemPlace::Contained {
                container: ACTOR,
                slot: id - 20,
                equipped: 0,
            },
            false,
        );
        coin.template = 273;
        coin.stack = 2;
        k.register_inventory_item(coin).unwrap();
    }
    let mut prepared = prepare(&mut k);
    for id in [21, 22] {
        prepared
            .possessions
            .push(bace_interactions::DeathPossession {
                id: EntityId(id),
                template: 273,
                item_type: 1,
                value: 2,
                stack: 2,
                wielded: false,
                bonded: 0,
            });
    }
    k.prepare_player_death(prepared).map_err(|e| e.0).unwrap();
    let ticket = k.take_player_death_proposal().unwrap();
    let transcript = ticket.inventory_transcript.as_ref().unwrap();
    assert!(transcript.pyreals_destroyed);
    assert_eq!(transcript.coin_amount, 2);
    assert!(transcript.coin_drops.is_empty());
    assert_eq!(transcript.coin_sources.len(), 1);
    assert!(transcript.coin_sources[0].whole);
    assert_eq!(transcript.coin_sources[0].source, EntityId(21));
    assert!(ticket.corpse_items.iter().all(|id| *id != EntityId(21)));
    let spent = ticket
        .inventory
        .proposal
        .changes
        .iter()
        .find(|change| change.after.id == EntityId(21))
        .unwrap();
    assert_eq!(spent.after.place, ItemPlace::Removed);
    assert_eq!(spent.after.stack, 0);
}
#[test]
fn partial_pyreal_stack_keeps_source_and_uses_one_fresh_corpse_identity() {
    let mut k = fixture();
    k.death_policy.destroy_pyreals = false;
    let mut coin = item(
        21,
        ItemPlace::Contained {
            container: ACTOR,
            slot: 1,
            equipped: 0,
        },
        false,
    );
    coin.template = 273;
    coin.stack = 4;
    k.register_inventory_item(coin).unwrap();
    let mut prepared = prepare(&mut k);
    prepared
        .possessions
        .push(bace_interactions::DeathPossession {
            id: EntityId(21),
            template: 273,
            item_type: 1,
            value: 4,
            stack: 4,
            wielded: false,
            bonded: 0,
        });
    let mut fresh = item(31, ItemPlace::World, false);
    fresh.template = 273;
    fresh.stack = 2;
    prepared.coin_stacks.push(fresh);
    k.prepare_player_death(prepared).map_err(|e| e.0).unwrap();
    let ticket = k.take_player_death_proposal().unwrap();
    let transcript = ticket.inventory_transcript.as_ref().unwrap();
    assert_eq!(transcript.coin_amount, 2);
    assert_eq!(transcript.coin_drops, vec![EntityId(31)]);
    assert!(!transcript.coin_sources[0].whole);
    assert_eq!(transcript.coin_sources[0].after, 2);
    assert_eq!(transcript.coin_sources[0].burden_after, 3);
    assert_eq!(transcript.coin_sources[0].coin_value_after, 2);
    let source = ticket
        .inventory
        .proposal
        .changes
        .iter()
        .find(|change| change.after.id == EntityId(21))
        .unwrap();
    assert_eq!(
        source.after.place,
        ItemPlace::Contained {
            container: ACTOR,
            slot: 0,
            equipped: 0,
        }
    );
    assert_eq!(source.after.stack, 2);
    assert!(ticket.inventory.proposal.changes.iter().any(|change| {
        change.before.is_none()
            && change.after.id == EntityId(31)
            && change.after.stack == 2
            && change.after.place
                == ItemPlace::Contained {
                    container: ticket.corpse,
                    slot: 0,
                    equipped: 0,
                }
    }));
}
fn drain(k: &mut Kernel) {
    while k.take_magic_event().is_some() {}
    while k.take_player_death_event().is_some() {}
    while k.take_portal_event().is_some() {}
}
#[test]
fn exact_receipt_loss_and_respawn_are_reserved_and_do_not_reroll() {
    let mut k = fixture();
    let prepared = prepare(&mut k);
    assert!(k.inventory.reserved(ACTOR));
    assert!(k.inventory.reserved(EntityId(20)));
    assert!(
        k.register_inventory_item(item(
            21,
            ItemPlace::Contained {
                container: ACTOR,
                slot: 1,
                equipped: 0
            },
            false
        ))
        .is_err()
    );
    k.prepare_player_death(prepared).map_err(|e| e.0).unwrap();
    let t = k.take_player_death_proposal().unwrap();
    assert_eq!(t.kind, bace_interactions::PlayerDeathKind::Ordinary);
    assert!(t.before.last_outside_death.is_none());
    assert_eq!(t.after.last_outside_death.unwrap().cell, 1);
    assert!(
        k.player_death_state(ACTOR)
            .unwrap()
            .last_outside_death
            .is_none()
    );
    assert_eq!(t.post_death_maxima, [95; 3]);
    assert_eq!(
        t.vitals.iter().map(|v| v.after).collect::<Vec<_>>(),
        vec![71; 3]
    );
    assert!(t.enchantments.iter().all(|e| e.spell == 666));
    assert!(!t.purge_bad);
    let transcript = t.inventory_transcript.as_ref().unwrap();
    assert_eq!(transcript.coin_amount, 0);
    assert_eq!(transcript.drops.len(), 1);
    assert_eq!(transcript.drops[0].source, EntityId(20));
    assert_eq!(transcript.drops[0].dropped, EntityId(20));
    assert_eq!(transcript.drops[0].origin, crate::DeathDropOrigin::Pack);
    assert!(
        k.confirm_inventory_committed(&crate::InventoryReceipt {
            operation: t.inventory.operation,
            revisions: vec![]
        })
        .is_err()
    );
    k.retry_player_death(t.operation).unwrap();
    assert_eq!(k.take_player_death_proposal().unwrap(), t);
    let mut receipt = crate::PlayerDeathReceipt {
        operation: t.operation,
        actor: ACTOR,
        after_revision: t.after_revision,
        inventory: crate::InventoryReceipt {
            operation: t.inventory.operation,
            revisions: t
                .inventory
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        },
    };
    receipt.after_revision += 1;
    assert_eq!(
        k.confirm_player_death_committed_at(&receipt, 100000),
        Err(E::Receipt)
    );
    receipt.after_revision -= 1;
    for _ in 0..35 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(k.world.corpse(EntityId(30)).is_none());
    assert_eq!(k.world.combatant(ACTOR).unwrap().health(), 0);
    k.confirm_player_death_committed_at(&receipt, 100000)
        .unwrap();
    assert!(matches!(
        k.take_player_death_event(),
        Some(crate::PlayerDeathEvent::Started {
            num_deaths: 1,
            death_level: Some(200),
            vitae_pool: Some(0),
            vitae: Some(ref entry),
            purge_bad: false,
            suicide: false,
            ..
        }) if entry.spell == 666
    ));
    assert_eq!(
        k.magic.registry(ACTOR).unwrap().revision(),
        t.registry_after_revision
    );
    assert!(k.take_magic_event().is_none());
    let mut forged = receipt.clone();
    forged.inventory.revisions.clear();
    assert_eq!(
        k.confirm_player_death_committed_at(&forged, 100000),
        Err(E::Receipt)
    );
    for _ in 0..31 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(k.world.corpse(EntityId(30)).is_some());
    assert_eq!(k.world.combatant(ACTOR).unwrap().health(), 0);
    assert!(matches!(
        k.inventory_item(EntityId(20)).unwrap().place,
        ItemPlace::Contained {
            container: EntityId(30),
            ..
        }
    ));
    assert!(k.player_death_pending(ACTOR));
    for _ in 0..90 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert_eq!(k.world.combatant(ACTOR).unwrap().health(), 71);
    assert!(!k.player_death_pending(ACTOR));
    assert_eq!(
        k.world.combatant(ACTOR).unwrap().profile().maximum_health,
        95
    );
    assert!(k.world.combatant(ACTOR).unwrap().contributors().is_empty());
    assert!(k.world.is_in_portal_transit(ACTOR));
    assert!(k.world.combatant(ACTOR).unwrap().lifestone_protected());
    assert_eq!(k.player_death_state(ACTOR).unwrap().num_deaths, 1);
}
#[test]
fn rejected_preparation_retains_graph_and_player_until_assets_are_correct() {
    let mut k = fixture();
    let mut p = prepare(&mut k);
    p.possessions[0].stack = 2;
    let (e, p) = k.prepare_player_death(p).err().unwrap();
    assert_eq!(e, E::Stale);
    assert!(k.take_player_death_proposal().is_none());
    assert!(k.inventory.reserved(EntityId(20)));
    let mut p = *p;
    p.possessions[0].stack = 1;
    k.prepare_player_death(p).map_err(|e| e.0).unwrap();
    assert!(k.take_player_death_proposal().is_some());
}

#[test]
fn dropped_equipment_loses_permanent_aura_and_gear_health_before_respawn() {
    let mut aura = entry(101, -1.);
    aura.caster = 22;
    aura.spec.category = 2;
    aura.spec.stat_type = 2 | 0x8000;
    aura.spec.stat_key = 1;
    aura.spec.value = 30.;
    let mut k = fixture_with_entry(Some(aura));
    let mut gear = item(
        22,
        ItemPlace::Contained {
            container: ACTOR,
            slot: 1,
            equipped: 1,
        },
        false,
    );
    gear.valid_wield = 1;
    k.register_inventory_item(gear).unwrap();
    let mut p = prepare(&mut k);
    p.possessions.push(bace_interactions::DeathPossession {
        id: EntityId(22),
        template: 122,
        item_type: 2,
        value: 100,
        stack: 1,
        wielded: true,
        bonded: -1,
    });
    p.equipped_health.push((EntityId(22), 20));
    k.prepare_player_death(p).map_err(|e| e.0).unwrap();
    let t = k.take_player_death_proposal().unwrap();
    assert_eq!(t.post_death_maxima, [95; 3]);
    assert!(!t.enchantments.iter().any(|e| e.spell == 101));
    assert!(t.corpse_items.contains(&EntityId(22)));
}
#[test]
fn protection_and_pk_respite_tick_only_after_capacity_and_reservation_checks() {
    let mut k = fixture();
    k.death_policy.pk_server = true;
    k.death_policy.pk_respite_seconds = 10;
    let state = k.player_deaths.states.get_mut(&ACTOR).unwrap();
    state.protection_elapsed = Some(55.);
    state.pk_respite_elapsed = Some(5.);
    k.synchronize_death_projection(ACTOR);
    assert_eq!(k.world.combatant_mut(ACTOR).unwrap().damage(10).unwrap(), 0);
    k.characters.reserve_death(ACTOR, 99).unwrap();
    k.step_player_death_timers(150).unwrap();
    assert_eq!(
        k.player_death_state(ACTOR).unwrap().protection_elapsed,
        Some(55.)
    );
    k.characters.release_death(ACTOR, 99).unwrap();
    k.step_player_death_timers(150).unwrap();
    assert_eq!(
        k.player_death_state(ACTOR).unwrap().protection_elapsed,
        None
    );
    assert_eq!(k.player_death_state(ACTOR).unwrap().pk_status, 4);
    assert_eq!(k.world.combatant(ACTOR).unwrap().death_pk_status(), Some(4));
    assert!(!k.world.combatant(ACTOR).unwrap().lifestone_protected());
}
#[test]
fn empty_corpse_waits_for_exact_tombstone_and_preserves_bonded_possessions() {
    let mut k = fixture();
    let mut p = prepare(&mut k);
    p.possessions[0].bonded = 1;
    k.prepare_player_death(p).map_err(|e| e.0).unwrap();
    let t = k.take_player_death_proposal().unwrap();
    let receipt = crate::PlayerDeathReceipt {
        operation: t.operation,
        actor: ACTOR,
        after_revision: t.after_revision,
        inventory: crate::InventoryReceipt {
            operation: t.inventory.operation,
            revisions: t
                .inventory
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        },
    };
    k.confirm_player_death_committed_at(&receipt, 40).unwrap();
    for _ in 0..50 {
        k.step().unwrap();
        drain(&mut k);
    }
    let expiry = k
        .take_corpse_expiry_proposal()
        .expect("deadline remains due after corpse adoption");
    assert_eq!(expiry.corpse, t.corpse);
    assert_eq!(expiry.death_operation, t.operation);
    assert!(
        expiry
            .inventory
            .proposal
            .changes
            .iter()
            .all(|c| c.after.place == ItemPlace::Removed)
    );
    assert!(k.world.corpse(t.corpse).is_some());
    assert!(k.inventory_item(EntityId(20)).is_some());
    let receipt = crate::InventoryReceipt {
        operation: expiry.inventory.operation,
        revisions: expiry
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert!(k.confirm_inventory_committed(&receipt).is_err());
    let mut wrong = receipt.clone();
    wrong.revisions.pop();
    assert_eq!(k.confirm_corpse_expiry_committed(&wrong), Err(E::Receipt));
    k.retry_corpse_expiry(receipt.operation).unwrap();
    assert_eq!(k.take_corpse_expiry_proposal().unwrap(), expiry);
    k.confirm_corpse_expiry_committed(&receipt).unwrap();
    assert_eq!(
        k.take_corpse_expiry_event().unwrap().phase,
        crate::CorpseExpiryPhase::Destroying
    );
    assert!(k.world.corpse(t.corpse).is_some());
    for _ in 0..31 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(k.world.corpse(t.corpse).is_none());
    assert!(k.inventory_item(EntityId(20)).is_some());
    assert_eq!(k.take_corpse_expiry_event().unwrap().corpse, t.corpse);
}

#[test]
fn due_corpse_cannot_reopen_during_pending_or_destroying_expiry() {
    let mut k = fixture();
    let mut prepared = prepare(&mut k);
    prepared.possessions[0].bonded = 1;
    k.prepare_player_death(prepared).map_err(|e| e.0).unwrap();
    let death = k.take_player_death_proposal().unwrap();
    let committed = crate::PlayerDeathReceipt {
        operation: death.operation,
        actor: ACTOR,
        after_revision: death.after_revision,
        inventory: crate::InventoryReceipt {
            operation: death.inventory.operation,
            revisions: death
                .inventory
                .proposal
                .changes
                .iter()
                .map(|change| (change.after.id, change.after.revision))
                .collect(),
        },
    };
    k.confirm_player_death_committed_at(&committed, 40).unwrap();
    for _ in 0..50 {
        k.step().unwrap();
        drain(&mut k);
    }
    let expiry = k.take_corpse_expiry_proposal().unwrap();
    let profile = k
        .player_deaths
        .corpse_access
        .get(&death.corpse)
        .unwrap()
        .profile
        .clone();
    assert_eq!(
        k.inspect_corpse_access(death.corpse, ACTOR, false, false),
        Err(crate::CorpseAccessError::Stale)
    );
    let receipt = crate::InventoryReceipt {
        operation: expiry.inventory.operation,
        revisions: expiry
            .inventory
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    };
    k.confirm_corpse_expiry_committed(&receipt).unwrap();
    assert_eq!(
        k.take_corpse_expiry_event().unwrap().phase,
        crate::CorpseExpiryPhase::Destroying
    );
    assert!(k.world.corpse(death.corpse).is_some());
    assert_eq!(
        k.inspect_corpse_access(death.corpse, ACTOR, false, false),
        Err(crate::CorpseAccessError::Missing)
    );
    assert_eq!(
        k.register_corpse_access(death.corpse, death.operation, profile),
        Err(crate::CorpseAccessError::Stale)
    );
}

#[test]
fn trusted_death_rejection_returns_exact_corpse_and_retries_after_output_pressure() {
    let mut k = fixture();
    let mut prepared = prepare(&mut k);
    let corpse = prepared.corpse.id;
    let position = prepared.corpse.body.accepted().position();
    prepared.animation_seconds = f64::NAN;
    k.enqueue(crate::Command::PlayerDeathService(
        crate::PlayerDeathServiceCommand {
            correlation: 41,
            command: crate::PlayerDeathCommand::Prepare(Box::new(prepared)),
        },
    ))
    .unwrap();
    k.enqueue(crate::Command::PlayerDeathService(
        crate::PlayerDeathServiceCommand {
            correlation: 42,
            command: crate::PlayerDeathCommand::Retry {
                operation: u64::MAX,
            },
        },
    ))
    .unwrap();
    k.step().unwrap();
    assert!(k.player_death_pending(ACTOR));
    assert!(!k.world.contains_identity(corpse));
    let first = k.take_player_death_service_outcome().unwrap();
    assert_eq!(first.correlation, 41);
    let Err((E::MissingAssets, command)) = first.result else {
        panic!("expected retained invalid preparation")
    };
    let crate::PlayerDeathCommand::Prepare(mut prepared) = *command else {
        panic!("lost prepared body")
    };
    assert_eq!(prepared.corpse.id, corpse);
    assert_eq!(prepared.corpse.body.accepted().position(), position);
    // The second command stayed queued while the single outcome slot was full.
    assert!(k.take_player_death_service_outcome().is_none());
    k.step().unwrap();
    let second = k.take_player_death_service_outcome().unwrap();
    assert_eq!(second.correlation, 42);
    assert!(second.result.is_err());
    prepared.animation_seconds = 0.;
    let result = k.apply_player_death_service(crate::PlayerDeathServiceCommand {
        correlation: 43,
        command: crate::PlayerDeathCommand::Prepare(prepared),
    });
    assert!(result.result.is_ok());
    assert_eq!(k.peek_player_death_proposal().unwrap().corpse, corpse);
}

#[test]
fn player_corpse_spill_keeps_items_until_geometry_and_exact_receipt_then_delays_removal() {
    let mut k = fixture();
    let p = prepare(&mut k);
    k.prepare_player_death(p).map_err(|e| e.0).unwrap();
    let death = k.take_player_death_proposal().unwrap();
    k.confirm_player_death_committed_at(
        &crate::PlayerDeathReceipt {
            operation: death.operation,
            actor: ACTOR,
            after_revision: death.after_revision,
            inventory: crate::InventoryReceipt {
                operation: death.inventory.operation,
                revisions: death
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .map(|c| (c.after.id, c.after.revision))
                    .collect(),
            },
        },
        40,
    )
    .unwrap();
    for _ in 0..50 {
        k.step().unwrap();
        drain(&mut k);
    }
    let expiry = k.take_corpse_expiry_proposal().unwrap();
    assert_eq!(expiry.spill.as_ref().unwrap().roots, vec![EntityId(20)]);
    let receipt = crate::InventoryReceipt {
        operation: expiry.inventory.operation,
        revisions: expiry
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert_eq!(
        k.confirm_corpse_expiry_committed(&receipt),
        Err(E::MissingAssets)
    );
    assert!(
        matches!(k.inventory.item(EntityId(20)).unwrap().place,ItemPlace::Contained{container,..} if container==death.corpse)
    );
    let bad = crate::PreparedCorpseSpill {
        operation: expiry.inventory.operation,
        actors: vec![body(&k.world, EntityId(21))],
    };
    let (_, returned) = k.prepare_corpse_spill(bad).err().unwrap();
    assert_eq!(returned.actors[0].id, EntityId(21));
    let prepared = crate::PreparedCorpseSpill {
        operation: expiry.inventory.operation,
        actors: vec![body(&k.world, EntityId(20))],
    };
    k.prepare_corpse_spill(prepared).map_err(|e| e.0).unwrap();
    assert!(k.world.body(EntityId(20)).is_err());
    let mut stale = receipt.clone();
    stale.revisions.pop();
    assert_eq!(k.confirm_corpse_expiry_committed(&stale), Err(E::Receipt));
    k.confirm_corpse_expiry_committed(&receipt).unwrap();
    assert_eq!(
        k.inventory.item(EntityId(20)).unwrap().place,
        ItemPlace::World
    );
    assert!(k.world.is_anchor(EntityId(20)));
    assert!(k.world.corpse(death.corpse).is_some());
    assert_eq!(
        k.take_corpse_expiry_event().unwrap().phase,
        crate::CorpseExpiryPhase::Destroying
    );
    for _ in 0..31 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert_eq!(
        k.take_corpse_expiry_event().unwrap().phase,
        crate::CorpseExpiryPhase::Removed
    );
    assert!(k.world.corpse(death.corpse).is_none());
    assert!(k.world.is_anchor(EntityId(20)));
    assert!(k.inventory.item(EntityId(20)).is_some());
}

#[test]
fn olthoi_empty_death_preserves_possessions_and_rejects_wrong_source_branch() {
    let mut k = fixture();
    k.register_social_presence(
        bace_social::SocialPresence {
            identity: bace_gameplay_api::social::SocialIdentity {
                character: ACTOR,
                account: bace_types::AccountId(1),
                name: "Olthoi".into(),
            },
            access: 0,
            online: true,
            appear_offline: false,
            afk: false,
            gagged: false,
            olthoi: true,
            no_olthoi_talk: false,
            ignore_fellowship_requests: false,
            auto_accept_fellowship: false,
            share_fellowship_loot: false,
            society: 0,
            listen_allegiance: true,
            listen_general: true,
            listen_trade: true,
            listen_lfg: true,
            listen_roleplay: true,
            listen_society: true,
        },
        Default::default(),
    )
    .unwrap();
    let mut p = prepare(&mut k);
    p.olthoi = Some(crate::PreparedOlthoiDeath {
        kind: crate::OlthoiDeathKind::Slag,
        had_vitae: false,
        before_timestamp: None,
        after_timestamp: Some(100),
        items: vec![],
        containers: vec![],
    });
    let (_, rejected) = k
        .prepare_player_death(p)
        .expect_err("wrong source branch rejected");
    let mut p = *rejected;
    assert_eq!(
        k.inventory.item(EntityId(20)).unwrap().place,
        ItemPlace::Contained {
            container: ACTOR,
            slot: 0,
            equipped: 0
        }
    );
    p.olthoi.as_mut().unwrap().kind = crate::OlthoiDeathKind::Empty;
    p.olthoi.as_mut().unwrap().after_timestamp = None;
    k.prepare_player_death(p).map_err(|e| e.0).unwrap();
    let ticket = k.take_player_death_proposal().unwrap();
    assert_eq!(ticket.olthoi, Some(crate::OlthoiDeathKind::Empty));
    assert_eq!(ticket.inventory.proposal.changes.len(), 1);
    assert!(ticket.corpse_items.is_empty());
    assert!(ticket.after.last_outside_death.is_none());
}

#[test]
fn olthoi_treasure_container_and_descendant_share_corpse_operation() {
    let mut k = fixture();
    let killer = EntityId(0x50000002);
    k.world.insert(body(&k.world, killer)).unwrap();
    k.world
        .register_combatant(
            killer,
            Combatant::new(CombatantProfile {
                maximum_health: 100,
                melee_damage: 1,
                melee_range: 2.,
                attack_duration: 1.,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap(),
        )
        .unwrap();
    k.register_social_presence(
        bace_social::SocialPresence {
            identity: bace_gameplay_api::social::SocialIdentity {
                character: ACTOR,
                account: bace_types::AccountId(1),
                name: "Olthoi".into(),
            },
            access: 0,
            online: true,
            appear_offline: false,
            afk: false,
            gagged: false,
            olthoi: true,
            no_olthoi_talk: false,
            ignore_fellowship_requests: false,
            auto_accept_fellowship: false,
            share_fellowship_loot: false,
            society: 0,
            listen_allegiance: true,
            listen_general: true,
            listen_trade: true,
            listen_lfg: true,
            listen_roleplay: true,
            listen_society: true,
        },
        Default::default(),
    )
    .unwrap();
    let mut p = prepare_with_killer(&mut k, Some(killer));
    let root = EntityId(31);
    let child = EntityId(32);
    let younger_child = EntityId(33);
    p.olthoi = Some(crate::PreparedOlthoiDeath {
        kind: crate::OlthoiDeathKind::Treasure,
        had_vitae: false,
        before_timestamp: None,
        after_timestamp: None,
        items: vec![
            item(
                root.0,
                ItemPlace::Contained {
                    container: p.corpse.id,
                    slot: 0,
                    equipped: 0,
                },
                true,
            ),
            item(
                child.0,
                ItemPlace::Contained {
                    container: root,
                    slot: 1,
                    equipped: 0,
                },
                false,
            ),
            item(
                younger_child.0,
                ItemPlace::Contained {
                    container: root,
                    slot: 0,
                    equipped: 0,
                },
                false,
            ),
        ],
        containers: vec![container(root, None)],
    });
    k.prepare_player_death(p).map_err(|e| e.0).unwrap();
    let ticket = k.take_player_death_proposal().unwrap();
    assert_eq!(ticket.olthoi, Some(crate::OlthoiDeathKind::Treasure));
    assert_eq!(ticket.corpse_items, vec![root]);
    assert_eq!(ticket.inventory.proposal.changes.len(), 4);
    assert!(ticket.inventory.proposal.changes.iter().any(|change| {
        change.after.id == child
            && change.after.place
                == ItemPlace::Contained {
                    container: root,
                    slot: 1,
                    equipped: 0,
                }
    }));
    assert!(ticket.inventory.proposal.changes.iter().any(|change| {
        change.after.id == younger_child
            && change.after.place
                == ItemPlace::Contained {
                    container: root,
                    slot: 0,
                    equipped: 0,
                }
    }));
    assert_eq!(
        k.inventory.item(EntityId(20)).unwrap().place,
        ItemPlace::Contained {
            container: ACTOR,
            slot: 0,
            equipped: 0,
        }
    );
}

#[test]
fn death_cancels_unreleased_cast_without_fizzle_or_effect_replay() {
    use crate::kernel::magic_components_fixture as f;
    let mut k = f::component_kernel(16);
    k.register_magic_caster(
        EntityId(1),
        crate::MagicCaster {
            player: true,
            known_spells: std::collections::BTreeSet::from([100]),
            school_skills: [1000; 5],
            magic_defense: 0,
            mana_conversion: 0,
            components_required: false,
            safe_components: false,
        },
    )
    .unwrap();
    k.register_magic_spell(f::spell(
        100,
        bace_magic::SpellEffect::Boost {
            vital: bace_magic::Vital::Health,
            minimum: 20,
            maximum: 20,
        },
    ))
    .unwrap();
    let request = bace_gameplay_api::CastRequest::Targeted {
        target: EntityId(2),
        spell: 100,
    };
    assert!(matches!(
        k.magic.apply(
            f::context(1),
            request,
            &mut k.world,
            0.,
            &k.combat,
            &k.fellowships,
            None
        ),
        Ok(bace_gameplay_api::CastChange::Started { .. })
    ));
    assert!(k.magic.busy(EntityId(1)));
    assert!(
        k.magic
            .cancel_for_player_death(EntityId(1), &mut k.world)
            .is_err()
    );
    k.world
        .combatant_mut(EntityId(1))
        .unwrap()
        .damage(100)
        .unwrap();
    k.magic
        .cancel_for_player_death(EntityId(1), &mut k.world)
        .unwrap();
    assert!(!k.magic.busy(EntityId(1)));
    assert!(matches!(
        k.magic.take_outcome().unwrap().result,
        Ok(bace_gameplay_api::CastChange::Cancelled { .. })
    ));
    assert_eq!(
        k.world
            .vital(EntityId(2), bace_entity::EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    assert_eq!(
        k.world
            .vital(EntityId(1), bace_entity::EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
    k.magic
        .cancel_for_player_death(EntityId(1), &mut k.world)
        .unwrap();
    assert!(k.magic.take_outcome().is_none());
}
