use bace_content::{Property, Skill};
use bace_dat::{
    CharGen, CreationSkill, HeritageGroup, SkillBase, SkillFormula, SkillTable, XpTable,
};
use bace_gameplay_api::{ProgressionTarget, SkillAdvancement, TrainSkill};
use bace_runtime::{character_assets::prepare_character_assets, progression_saves::*};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};
use std::collections::BTreeMap;
fn fixture() -> (XpTable, SkillTable, CharGen) {
    // Synthetic cumulative tables already exercised by bace-character's
    // independent official C# progression oracle, not invented runtime defaults.
    let xp = XpTable {
        attribute_xp: vec![0, 10, 30, 30, 100],
        vital_xp: vec![0, 2, 8, 20, 100],
        trained_skill_xp: vec![0, 5, 15, 50, 100],
        specialized_skill_xp: vec![0, 1, 7, 40, 100],
        character_level_xp: vec![0, 1000, 3000],
        character_level_skill_credits: vec![0, 1, 2],
    };
    let skill = |trained, total| SkillBase {
        description: "retained description".into(),
        name: "synthetic".into(),
        icon_id: 123,
        trained_cost: trained,
        specialized_cost: total,
        category: 1,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 1,
            x: 1,
            y: 0,
            z: 2,
            attribute1: 2,
            attribute2: 4,
        },
        upper_bound: 1.0,
        lower_bound: 0.0,
        learn_modifier: 0.03,
    };
    let skills = SkillTable {
        bucket_size: 17,
        skills: BTreeMap::from([(6, skill(6, 18)), (7, skill(4, 10))]),
    };
    let heritage = HeritageGroup {
        name: "synthetic heritage".into(),
        icon: 42,
        setup: 43,
        environment_setup: 44,
        attribute_credits: 330,
        skill_credits: 52,
        primary_start_areas: vec![],
        secondary_start_areas: vec![],
        skills: vec![CreationSkill {
            skill: 6,
            normal_cost: 2,
            primary_cost: 3,
        }],
        templates: vec![],
        gender_marker: 1,
        genders: BTreeMap::new(),
    };
    let chargen = CharGen {
        reserved: 77,
        starter_areas: vec![],
        heritage_marker: 1,
        heritage_groups: BTreeMap::from([(1, heritage)]),
    };
    (xp, skills, chargen)
}

fn saved() -> PlayerSaveV6 {
    let mut state = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "human".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.ints = vec![
        Property { id: 24, value: 20 },
        Property { id: 188, value: 1 },
        Property {
            id: 9999,
            value: 73,
        },
    ];
    state.properties.int64s = vec![
        Property { id: 2, value: 1000 },
        Property { id: 1, value: 5000 },
    ];
    state.properties.skills = vec![
        Property {
            id: 6,
            value: Skill {
                level_from_pp: 0,
                sac: 1,
                pp: 0,
                init_level: 0,
                resistance_at_last_check: 23,
                last_used_time: 456.5,
            },
        },
        Property {
            id: 999,
            value: Skill {
                level_from_pp: 0,
                sac: 1,
                pp: 0,
                init_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        },
    ];
    let mut result = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: 0x50000001,
                template_revision: 1,
                mutation_revision: 4,
                state,
            },
            account_id: 1,
            name: "Alice smith".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    result.ui.gameplay_options = vec![1, 2, 3];
    result.player.metadata.options1 = 123;
    result.player.metadata.titles = vec![1, 2];
    result
}
#[test]
fn durable_training_snapshot_reload_preserves_unknown_and_auxiliary_fields() {
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let save = saved();
    let state = restore_progression(&save, &assets).unwrap();
    let proposal = state
        .propose_train_skill(TrainSkill {
            skill: 6,
            quoted_credits: 6,
        })
        .unwrap();
    let frozen = freeze_skill_proposal(&save, &assets, &proposal).unwrap();
    assert_eq!(save.player.entity.mutation_revision, 4);
    assert_eq!(state.available_skill_credits(), Some(20));
    assert_eq!(frozen.player.entity.mutation_revision, 5);
    assert_eq!(frozen.ui, save.ui);
    assert_eq!(frozen.player.metadata, save.player.metadata);
    assert_eq!(frozen.enchantments, save.enchantments);
    assert_eq!(frozen.rares, save.rares);
    assert_eq!(
        frozen
            .player
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 9999)
            .unwrap()
            .value,
        73
    );
    assert!(
        frozen
            .player
            .entity
            .state
            .properties
            .skills
            .iter()
            .any(|p| p.id == 999)
    );
    let decoded = PlayerSaveV6::decode(&frozen.encode().unwrap()).unwrap();
    let mut loaded = restore_progression(&decoded, &assets).unwrap();
    assert_eq!(loaded.available_skill_credits(), Some(14));
    assert_eq!(
        loaded
            .projection(ProgressionTarget::Skill(6))
            .unwrap()
            .advancement,
        SkillAdvancement::Trained
    );
    let spec = loaded.propose_specialize_skill(6).unwrap();
    let frozen = freeze_skill_proposal(&decoded, &assets, &spec).unwrap();
    loaded.adopt_skill_proposal(spec).unwrap();
    let same = freeze_progression(&decoded, &loaded).unwrap();
    assert_eq!(frozen, same);
}

#[test]
fn endurance_rank_and_accepted_health_current_share_one_player_v6_save() {
    use bace_content::{Attribute, SecondaryAttribute};
    use bace_entity::VitalPool;
    use bace_gameplay_api::{AttributeId, RaiseProgression};
    use bace_geometry::Vec3;
    use bace_runtime::world_saves::freeze_world;
    use bace_simulation::PlayerWorldSnapshot;
    use bace_types::CellId;

    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let mut saved = saved();
    let properties = &mut saved.player.entity.state.properties;
    properties.attributes.push(Property {
        id: AttributeId::Endurance as u32,
        value: Attribute {
            init_level: 100,
            level_from_cp: 0,
            cp_spent: 0,
        },
    });
    properties.secondary_attributes = [1, 3, 5]
        .map(|id| Property {
            id,
            value: SecondaryAttribute {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 90,
            },
        })
        .to_vec();
    let mut character = restore_progression(&saved, &assets).unwrap();
    let change = character
        .raise(RaiseProgression {
            target: ProgressionTarget::Attribute(AttributeId::Endurance),
            amount: 10,
        })
        .unwrap();
    assert_eq!(change.after.ranks, 1);
    let mut frozen = freeze_progression(&saved, &character).unwrap();
    freeze_world(
        &mut frozen,
        PlayerWorldSnapshot {
            cell: CellId(0xa260000a),
            position: Vec3::new(1.0, 2.0, 3.0),
            heading: 0.0,
            vitals: [47, 50, 50].map(|current| {
                Some(VitalPool {
                    current,
                    maximum: 101,
                })
            }),
        },
    )
    .unwrap();
    let reloaded = PlayerSaveV6::decode(&frozen.encode().unwrap()).unwrap();
    assert_eq!(reloaded.player.entity.mutation_revision, 5);
    let properties = &reloaded.player.entity.state.properties;
    assert_eq!(
        properties
            .attributes
            .iter()
            .find(|p| p.id == 2)
            .unwrap()
            .value
            .level_from_cp,
        1
    );
    assert_eq!(
        properties
            .secondary_attributes
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .value
            .current_level,
        47
    );
    assert_eq!(
        properties.ints.iter().find(|p| p.id == 9999).unwrap().value,
        73
    );
    assert_eq!(reloaded.ui, saved.ui);
}
#[test]
fn stale_snapshot_or_inconsistent_rank_cannot_be_frozen() {
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let mut save = saved();
    let state = restore_progression(&save, &assets).unwrap();
    let p = state
        .propose_train_skill(TrainSkill {
            skill: 6,
            quoted_credits: 6,
        })
        .unwrap();
    save.player.entity.mutation_revision += 1;
    assert_eq!(
        freeze_skill_proposal(&save, &assets, &p),
        Err(ProgressionSaveError::StaleState)
    );
    save.player.entity.state.properties.skills[0]
        .value
        .level_from_pp = 190;
    assert!(matches!(
        restore_progression(&save, &assets),
        Err(ProgressionSaveError::RankMismatch)
    ));
}
#[test]
fn name_adapter_uses_first_authored_taboo_category() {
    let table = bace_dat::TabooTable {
        marker: 1,
        entries: vec![
            bace_dat::TabooEntry {
                flags: 4,
                unknown1: 0,
                unknown2: 0,
                patterns: vec!["foo".into()],
            },
            bace_dat::TabooEntry {
                flags: 1,
                unknown1: 0,
                unknown2: 0,
                patterns: vec!["alice".into()],
            },
        ],
    };
    let policy =
        bace_runtime::name_preparation::prepare_name_policy(&table, &["Drudge".into()]).unwrap();
    assert!(policy.approve("Foo").is_err());
    assert!(policy.approve("Alice").is_ok());
    assert!(policy.approve("Drudge").is_err());
}
#[test]
fn channel_ticket_freeze_matches_opaque_proposal_and_checks_usage_fields() {
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let saved = saved();
    let state = restore_progression(&saved, &assets).unwrap();
    let p = state
        .propose_train_skill(TrainSkill {
            skill: 6,
            quoted_credits: 6,
        })
        .unwrap();
    assert_eq!(
        freeze_skill_change(&saved, p.change(), 4).unwrap(),
        freeze_skill_proposal(&saved, &assets, &p).unwrap()
    );
    let mut stale = saved.clone();
    stale.player.entity.state.properties.skills[0]
        .value
        .last_used_time += 1.0;
    assert_eq!(
        freeze_skill_change(&stale, p.change(), 4),
        Err(ProgressionSaveError::StaleState)
    );
    assert_eq!(
        freeze_skill_change(&saved, p.change(), 3),
        Err(ProgressionSaveError::StaleState)
    );
}

#[test]
fn one_player_revision_preserves_simultaneous_ui_skills_and_enchantments() {
    use bace_runtime::{
        enchantment_saves::restore_saved_enchantments, player_saves::freeze_player,
        ui_saves::restore_ui,
    };
    use bace_simulation::{OwnedCharacterState, OwnedUiState};
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let mut save = saved();
    // A cooldown needs no spell-table entry; all stored fields still survive.
    save.enchantments
        .push(bace_storage_codec::FrozenEnchantmentV1 {
            schema_version: 1,
            enchantment_category: 1,
            spell_id: 100,
            layer_id: 1,
            has_spell_set_id: false,
            spell_category: 0x8000,
            power_level: 0,
            start_time: -5.0,
            duration: 30.0,
            caster_object_id: 0x50000001,
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            stat_mod_type: 0,
            stat_mod_key: 0,
            stat_mod_value: 0.0,
            spell_set_id: 0,
        });
    let mut registry = restore_saved_enchantments(&save, 16, |_| None).unwrap();
    let mut progression = restore_progression(&save, &assets).unwrap();
    let proposal = progression
        .propose_train_skill(TrainSkill {
            skill: 6,
            quoted_credits: 6,
        })
        .unwrap();
    progression.adopt_skill_proposal(proposal).unwrap();
    let mut ui = restore_ui(&save).unwrap();
    ui.bars[7] = vec![100, 101];
    ui.gameplay = vec![255, 0, 123];
    let owner = OwnedCharacterState {
        native_services: None,
        contracts: None,
        progression,
        rares: None,
        ui: Some(OwnedUiState {
            state: ui.clone(),
            known_spells: vec![100, 101],
            component_templates: vec![],
            entered: true,
        }),
    };
    let frozen = freeze_player(&save, &owner, Some(&registry)).unwrap();
    let decoded = PlayerSaveV6::decode(&frozen.encode().unwrap()).unwrap();
    assert_eq!(restore_ui(&decoded).unwrap(), ui);
    assert_eq!(
        restore_progression(&decoded, &assets)
            .unwrap()
            .available_skill_credits(),
        Some(14)
    );
    assert_eq!(decoded.enchantments, save.enchantments);
    assert_eq!(decoded.player.metadata.titles, save.player.metadata.titles);
    // A fresh auxiliary mutation cannot hide under an already saved revision.
    registry
        .heartbeat(30.0, &mut Vec::with_capacity(16))
        .unwrap();
    assert!(freeze_player(&decoded, &owner, Some(&registry)).is_err());
    assert!(freeze_player(&decoded, &owner, None).is_err());
}

#[test]
fn login_preparation_normalizes_recovered_vitae_and_marks_complete_owner_dirty() {
    use bace_runtime::{
        enchantment_saves::EnchantmentDefinition,
        game_login::LoadedPlayer,
        player_saves::{freeze_player, restore_player},
    };
    use bace_types::{AccountId, EntityId};
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let mut save = saved();
    save.enchantments
        .push(bace_storage_codec::FrozenEnchantmentV1 {
            schema_version: 1,
            enchantment_category: 12,
            spell_id: 666,
            layer_id: 1,
            has_spell_set_id: false,
            spell_category: 1,
            power_level: 0,
            start_time: -10.0,
            duration: -1.0,
            caster_object_id: 0x50000001,
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            stat_mod_type: 0x4000,
            stat_mod_key: 0,
            stat_mod_value: 1.0,
            spell_set_id: 0,
        });
    let loaded = LoadedPlayer {
        is_plussed: false,
        key: bace_session::SessionKey {
            id: 1,
            generation: 1,
        },
        binding: bace_gameplay_api::CharacterBinding {
            session: bace_gameplay_api::SessionId(1),
            account: AccountId(1),
            actor: EntityId(0x50000001),
        },
        lease: bace_persistence::CharacterLease {
            character_id: 0x50000001,
            epoch: 1,
            state: bace_persistence::OwnershipState::Loading,
        },
        persisted_version: 1,
        player: save.clone(),
        cached_experience: 0,
        inventory: vec![],
    };
    // Unknown non-cooldown definitions cannot be silently dropped on login.
    assert!(restore_player(&loaded, &assets, &[], &[], |_| None).is_err());
    let owned = restore_player(&loaded, &assets, &[], &[], |id| {
        (id == 666).then_some(EnchantmentDefinition {
            school: bace_magic::MagicSchool::Creature,
            is_set_spell: false,
            is_level8_aura: false,
            category: 1,
            power: 0,
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            stat_type: 0x4000,
            stat_key: 0,
            beneficial: false,
        })
    })
    .unwrap();
    assert_eq!(owned.character.progression.revision(), 5);
    assert!(!owned.character.ui.as_ref().unwrap().entered);
    assert!(owned.enchantments.as_ref().unwrap().entries().is_empty());
    let next = freeze_player(&save, &owned.character, owned.enchantments.as_ref()).unwrap();
    assert!(next.enchantments.is_empty());
    assert_eq!(next.player.entity.mutation_revision, 5);
    assert_eq!(loaded.player, save);
}

fn loaded_with_set_item(equipped: u32) -> bace_runtime::game_login::LoadedPlayer {
    use bace_runtime::game_login::{LoadedPlayer, LoadedPlayerItem};
    let player = saved();
    let actor = player.player.entity.object_id;
    let mut entity = player.player.entity.clone();
    entity.object_id = 42;
    entity.state.properties.ints = vec![Property { id: 265, value: 7 }];
    LoadedPlayer {
        is_plussed: false,
        key: bace_session::SessionKey {
            id: 1,
            generation: 1,
        },
        binding: bace_gameplay_api::CharacterBinding {
            session: bace_gameplay_api::SessionId(1),
            account: bace_types::AccountId(1),
            actor: bace_types::EntityId(actor),
        },
        lease: bace_persistence::CharacterLease {
            character_id: actor,
            epoch: 1,
            state: bace_persistence::OwnershipState::Loading,
        },
        persisted_version: 1,
        player,
        cached_experience: 0,
        inventory: vec![LoadedPlayerItem {
            construction: None,
            source_destination: None,
            enchantments: vec![],
            entity,
            location: bace_persistence::ItemLocation {
                container: actor,
                slot: 0,
            },
            persisted_version: 1,
            depth: 1,
            placement: bace_storage_codec::ItemPlacementV2::Contained {
                container: actor,
                slot: 0,
                pack_slot: false,
                equipped,
            },
        }],
    }
}

fn item_spell(has_spell_set_id: bool) -> bace_storage_codec::FrozenEnchantmentV1 {
    bace_storage_codec::FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 1,
        spell_id: 100,
        layer_id: 1,
        has_spell_set_id,
        spell_category: 1,
        power_level: 0,
        start_time: 0.0,
        duration: -1.0,
        caster_object_id: 42,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        stat_mod_type: 0,
        stat_mod_key: 0,
        stat_mod_value: 0.0,
        spell_set_id: if has_spell_set_id { 7 } else { 0 },
    }
}

fn item_spell_definition(
    id: u32,
) -> Option<bace_runtime::enchantment_saves::EnchantmentDefinition> {
    (id == 100).then_some(bace_runtime::enchantment_saves::EnchantmentDefinition {
        school: bace_magic::MagicSchool::Item,
        is_set_spell: false,
        is_level8_aura: false,
        category: 1,
        power: 0,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        stat_type: 0,
        stat_key: 0,
        beneficial: false,
    })
}

#[test]
fn login_carried_set_item_without_applicable_permanent_buff_needs_no_set_assets() {
    use bace_runtime::player_saves::restore_player;
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let mut loaded = loaded_with_set_item(0);
    let owner = restore_player(&loaded, &assets, &[], &[], |_| None).unwrap();
    assert_eq!(owner.character.progression.revision(), 4);
    assert!(owner.enchantments.unwrap().entries().is_empty());

    // ACE Player_Spells.AuditItemSpells only resolves set metadata after the
    // indefinite-duration and caster ownership/equipment gates. A timed set
    // spell remains intact even though its item's set assets were not prepared.
    let mut timed = item_spell(true);
    timed.duration = 30.0;
    loaded.player.enchantments.push(timed);
    let owner = restore_player(&loaded, &assets, &[], &[], item_spell_definition).unwrap();
    assert_eq!(owner.character.progression.revision(), 4);
    assert_eq!(owner.enchantments.unwrap().entries().len(), 1);

    // An ordinary permanent item spell from an unequipped item is removed by
    // the equipment gate without consulting that item's set metadata.
    loaded.player.enchantments = vec![item_spell(false)];
    let owner = restore_player(&loaded, &assets, &[], &[], item_spell_definition).unwrap();
    assert_eq!(owner.character.progression.revision(), 5);
    assert!(owner.enchantments.unwrap().entries().is_empty());
}

#[test]
fn login_applicable_permanent_item_spell_requires_prepared_set_assets() {
    use bace_runtime::player_saves::{PlayerSaveError, restore_player};
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    // Set spells use all possessions; ordinary item spells require equipment.
    for (equipped, has_spell_set_id) in [(0, true), (1, false)] {
        let mut loaded = loaded_with_set_item(equipped);
        loaded
            .player
            .enchantments
            .push(item_spell(has_spell_set_id));
        assert!(matches!(
            restore_player(&loaded, &assets, &[], &[], item_spell_definition),
            Err(PlayerSaveError::Codec(
                bace_storage_codec::SaveCodecError::Invalid("missing prepared equipment set")
            ))
        ));
        let prepared = [bace_magic::EquippedSpellSet {
            id: 7,
            possible: &[100],
            active: &[100],
        }];
        let item_xp = bace_simulation::PreparedItemExperience {
            actor: loaded.binding.actor,
            item: bace_types::EntityId(42),
            name: "Set item".into(),
            experience: None,
            set: Some(bace_simulation::PreparedItemSet {
                id: 7,
                tiers: BTreeMap::new(),
            }),
            set_uses_item_levels: false,
            equipment_order: 1,
        };
        let owner = bace_runtime::player_saves::restore_player_at_with_item_experience(
            &loaded,
            &assets,
            0,
            &[],
            &prepared,
            &[item_xp],
            item_spell_definition,
        )
        .unwrap();
        assert_eq!(owner.character.progression.revision(), 4);
        assert_eq!(owner.enchantments.unwrap().entries().len(), 1);
        assert_eq!(
            loaded.player.enchantments,
            vec![item_spell(has_spell_set_id)]
        );
    }
}

#[test]
fn full_player_snapshot_rejects_missing_ui_owner_even_at_new_revision() {
    use bace_runtime::player_saves::{PlayerSaveError, freeze_player, restore_player};
    let (xp, skills, cg) = fixture();
    let assets = prepare_character_assets(xp, skills, cg).unwrap();
    let loaded = loaded_with_set_item(0);
    let mut owner = restore_player(&loaded, &assets, &[], &[], |_| None).unwrap();
    owner.character.progression.touch_revision().unwrap();
    let ui = owner.character.ui.take().unwrap();
    assert!(matches!(
        freeze_player(
            &loaded.player,
            &owner.character,
            owner.enchantments.as_ref()
        ),
        Err(PlayerSaveError::Codec(
            bace_storage_codec::SaveCodecError::Invalid("missing UI owner")
        ))
    ));
    owner.character.ui = Some(ui);
    let frozen = freeze_player(
        &loaded.player,
        &owner.character,
        owner.enchantments.as_ref(),
    )
    .unwrap();
    assert_eq!(frozen.player.entity.mutation_revision, 5);
    assert_eq!(frozen.ui, loaded.player.ui);
    assert_eq!(loaded.player.player.entity.mutation_revision, 4);
}
