use bace_character::{
    CharacterProgression, ProgressionTables, RankTable, SkillTrainingRules, TraitProgress,
    TraitState,
};
use bace_content::{Property, SecondaryAttribute, WeenieV1};
use bace_entity::VitalPool;
use bace_gameplay_api::{ProgressionTarget, SkillAdvancement, TraitDetails, VitalId};
use bace_geometry::Vec3;
use bace_runtime::player_saves::freeze_player_state;
use bace_simulation::{OwnedCharacterState, OwnedPlayerState, OwnedUiState, PlayerWorldSnapshot};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
use bace_types::CellId;
use std::sync::Arc;
fn fixture() -> (PlayerSaveV6, OwnedPlayerState) {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "player_save_fixture".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.secondary_attributes = vec![Property {
        id: 1,
        value: SecondaryAttribute {
            init_level: 100,
            level_from_cp: 0,
            cp_spent: 0,
            current_level: 100,
        },
    }];
    state.properties.ints = vec![Property { id: 24, value: 0 }];
    state.properties.int64s = vec![Property { id: 2, value: 30 }];
    let saved = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state,
        },
        account_id: 1,
        name: "Snapshot Fixture".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let progression = CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Vital(VitalId::MaxHealth),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Vital {
                starting_value: 100,
                current: 100,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        30,
        2,
    )
    .unwrap()
    .with_training(Arc::new(SkillTrainingRules::new(&[]).unwrap()), 0, &[])
    .unwrap();
    let ui = bace_runtime::ui_saves::restore_ui(&saved).unwrap();
    let owned = OwnedPlayerState {
        gag: None,
        equipment_mana: None,
        chat_age: None,
        physical_recovery: 0.0,
        death: None,
        social: None,
        portal_links: None,
        world: Some(PlayerWorldSnapshot {
            cell: CellId(1),
            position: Vec3::new(0.0, 0.0, 0.5),
            heading: 0.0,
            vitals: [
                Some(VitalPool {
                    current: 70,
                    maximum: 100,
                }),
                None,
                None,
            ],
        }),
        recovery: None,
        character: OwnedCharacterState {
            progression,
            rares: None,
            ui: Some(OwnedUiState {
                state: ui,
                known_spells: vec![],
                component_templates: vec![],
                entered: true,
            }),
            native_services: None,
            contracts: None,
        },
        enchantments: None,
        item_experience: vec![],
        item_enchantments: vec![],
    };
    (saved, owned)
}
#[test]
fn unchanged_full_resave_composes_world_vitals_before_the_revision_check() {
    let (saved, owned) = fixture();
    let after = freeze_player_state(&saved, &owned, 10_000).unwrap();
    assert_eq!(
        after.player.entity.state.properties.secondary_attributes[0]
            .value
            .current_level,
        70
    );
    assert_eq!(
        owned.character.progression.trait_states().next().unwrap().1,
        Some(TraitDetails::Vital {
            starting_value: 100,
            current: 100
        })
    );
    assert_eq!(freeze_player_state(&after, &owned, 11_000).unwrap(), after);
}
#[test]
fn changed_final_world_or_portal_supplement_requires_a_new_aggregate_revision() {
    let (saved, mut owned) = fixture();
    let after = freeze_player_state(&saved, &owned, 10_000).unwrap();
    owned.world.as_mut().unwrap().position.x = 2.0;
    assert!(freeze_player_state(&after, &owned, 11_000).is_err());
    owned.world.as_mut().unwrap().position.x = 0.0;
    owned.portal_links = Some(
        bace_interactions::PortalLinks::new(
            2,
            &[(
                4,
                bace_interactions::PortalPosition {
                    cell: 1,
                    origin: [2.0, 0.0, 0.5],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                },
            )],
            &[],
        )
        .unwrap(),
    );
    assert!(freeze_player_state(&after, &owned, 11_000).is_err());
}
#[test]
fn recovery_recapture_can_age_same_history_but_cannot_hide_a_new_cast_revision() {
    let (saved, mut owned) = fixture();
    let after = freeze_player_state(&saved, &owned, 10_000).unwrap();
    owned.recovery = Some(bace_magic::CastRecovery {
        revision: 1,
        minimum_remaining: 1.0,
        streak_remaining: 2.0,
        last_success_school: Some(bace_magic::MagicSchool::War),
        last_success_age: 0.0,
    });
    assert!(freeze_player_state(&after, &owned, 10_000).is_err());
    owned.character.progression.touch_revision().unwrap();
    let captured = freeze_player_state(&after, &owned, 10_000).unwrap();
    let recovery = owned.recovery.as_mut().unwrap();
    recovery.minimum_remaining = 0.0;
    recovery.streak_remaining = 1.0;
    recovery.last_success_age = 1.0;
    let recaptured = freeze_player_state(&captured, &owned, 11_000).unwrap();
    assert_eq!(
        recaptured.player.entity.mutation_revision,
        captured.player.entity.mutation_revision
    );
    assert_eq!(recaptured.combat_recovery.unwrap().state.revision, 1);
    owned.recovery.as_mut().unwrap().revision = 2;
    assert!(freeze_player_state(&recaptured, &owned, 11_000).is_err());
}
#[test]
fn physical_recovery_survives_frozen_save_and_requires_dirty_revision_to_extend() {
    let (saved, mut owned) = fixture();
    // The fixture starts one dirty revision ahead; establish a same-revision baseline.
    let saved = freeze_player_state(&saved, &owned, 10_000).unwrap();
    owned.physical_recovery = 2.0;
    assert!(freeze_player_state(&saved, &owned, 10_000).is_err());
    owned.character.progression.touch_revision().unwrap();
    let captured = freeze_player_state(&saved, &owned, 10_000).unwrap();
    let decoded = PlayerSaveV6::decode(&captured.encode().unwrap()).unwrap();
    assert_eq!(
        decoded
            .physical_recovery
            .unwrap()
            .remaining_at(10_500)
            .unwrap(),
        1.5
    );
    assert_eq!(
        decoded
            .physical_recovery
            .unwrap()
            .remaining_at(9_000)
            .unwrap(),
        2.0
    );
    owned.physical_recovery = 0.0;
    assert!(freeze_player_state(&captured, &owned, 10_500).is_err());
    owned.physical_recovery = 1.5;
    let aged = freeze_player_state(&captured, &owned, 10_500).unwrap();
    assert_eq!(
        aged.player.entity.mutation_revision,
        captured.player.entity.mutation_revision
    );
    owned.physical_recovery = 3.0;
    assert!(freeze_player_state(&aged, &owned, 11_000).is_err());
}

#[test]
fn online_snapshot_matches_drained_save_without_removing_the_live_character() {
    use bace_entity::{Actor, Combatant, CombatantProfile};
    use bace_gameplay_api::{CharacterBinding, SessionId};
    use bace_geometry::Aabb;
    use bace_types::{AccountId, EntityId};
    let (saved, mut owned) = fixture();
    owned.character.ui.as_mut().unwrap().state.options1 = 0x1234;
    let registry = bace_magic::EnchantmentRegistry::restore(
        8,
        1,
        vec![bace_magic::EnchantmentEntry {
            spell: 666,
            caster: saved.player.entity.object_id,
            school: bace_magic::MagicSchool::Life,
            spec: bace_magic::EnchantmentSpec {
                category: 204,
                power: 0,
                duration: -1.0,
                layer: 1,
                stat_type: 0,
                stat_key: 0,
                value: 0.95,
                beneficial: false,
                set_id: None,
            },
            start_time: 0.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: bace_magic::EnchantmentMetadata::default(),
        }],
    )
    .unwrap();
    let binding = CharacterBinding {
        actor: EntityId(saved.player.entity.object_id),
        account: AccountId(1),
        session: SessionId(7),
    };
    let mut world = bace_world::World::default();
    world
        .register_scene(
            CellId(1),
            bace_physics::SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let body = bace_physics::Body::spawn(
        world.scene(CellId(1)).unwrap(),
        Vec3::new(0., 0., 0.5),
        0.5,
        bace_motion::Capabilities {
            speed: 5.,
            jump_impulse: 5.,
        },
    )
    .unwrap();
    world
        .insert(Actor {
            id: binding.actor,
            cell: CellId(1),
            body,
        })
        .unwrap();
    let mut combatant = Combatant::new(CombatantProfile {
        maximum_health: 100,
        melee_damage: 1,
        melee_range: 2.,
        attack_duration: 1.,
        strike_offsets: vec![0.5],
        player: true,
    })
    .unwrap();
    combatant.damage(30).unwrap();
    world.register_combatant(binding.actor, combatant).unwrap();
    let mut kernel = bace_simulation::Kernel::with_gameplay_limits(world, 8, 1, 4).unwrap();
    kernel
        .register_character_with_rare(binding, owned.character)
        .unwrap();
    kernel
        .register_magic_registry(binding.actor, registry, true)
        .unwrap();
    let capture = kernel.read_player_snapshot(binding).unwrap();
    assert!(
        bace_runtime::player_saves::freeze_player_operation_baseline(
            &saved,
            &capture,
            bace_simulation::PlayerSnapshotOperation::PlayerDeath(9),
            capture.character().progression().revision(),
            10_000
        )
        .is_err()
    );
    let online =
        bace_runtime::player_saves::freeze_player_snapshot(&saved, &capture, 10_000).unwrap();
    assert_eq!(online.player.metadata.options1, 0x1234);
    assert_eq!(online.enchantments.len(), 1);
    assert_eq!(online.enchantments[0].spell_id, 666);
    assert!(kernel.has_characters());
    let drained = kernel.take_player_state(binding).unwrap();
    assert_eq!(
        online,
        freeze_player_state(&saved, &drained, 10_000).unwrap()
    );
    let mut wrong = saved.clone();
    wrong.player.account_id = 2;
    assert!(bace_runtime::player_saves::freeze_player_snapshot(&wrong, &capture, 10_000).is_err());
}

#[test]
fn spellbook_capture_preserves_known_metadata_and_applies_exact_authoritative_membership() {
    let (mut saved, mut owned) = fixture();
    saved.player.entity.state.properties.spell_book = vec![
        Property { id: 1, value: 0.25 },
        Property { id: 2, value: 0.5 },
    ];
    owned.character.ui.as_mut().unwrap().known_spells = vec![2, 3];
    let next = freeze_player_state(&saved, &owned, 10_000).unwrap();
    assert_eq!(
        next.player.entity.state.properties.spell_book,
        vec![
            Property { id: 2, value: 0.5 },
            Property { id: 3, value: 1.0 }
        ]
    );
    owned.character.ui.as_mut().unwrap().known_spells = vec![2, 2];
    assert!(freeze_player_state(&saved, &owned, 10_000).is_err());
}

#[test]
fn accepted_player_age_is_frozen_without_regression_or_unrevisioned_change() {
    let (saved, mut owned) = fixture();
    owned.chat_age = Some(51);
    let frozen = freeze_player_state(&saved, &owned, 1000).unwrap();
    assert_eq!(
        frozen
            .player
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 125)
            .unwrap()
            .value,
        51
    );
    assert_eq!(freeze_player_state(&frozen, &owned, 1000).unwrap(), frozen);
    owned.chat_age = Some(52);
    assert!(freeze_player_state(&frozen, &owned, 1000).is_err());
    owned.character.progression.touch_revision().unwrap();
    assert_eq!(
        freeze_player_state(&frozen, &owned, 1000)
            .unwrap()
            .player
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 125)
            .unwrap()
            .value,
        52
    );
    owned.chat_age = Some(50);
    assert!(freeze_player_state(&frozen, &owned, 1000).is_err());
}
