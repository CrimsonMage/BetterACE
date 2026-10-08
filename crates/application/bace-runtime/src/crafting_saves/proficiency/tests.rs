use super::*;
use bace_character::{
    CharacterLevelTable, CharacterProgression, ProgressionTables, RankTable, TraitProgress,
    TraitState,
};
use bace_simulation::CraftingProficiency;
use std::sync::Arc;

fn fixture(actor_counter: bool) -> (PlacementOperation, CraftingSavedPlayer, CraftingTicket) {
    let (_, online, work) = crate::crafting_service::tests::fixture::fixture();
    let mut ticket = work.ticket;
    let (saved, version, lease) = online.baseline(ticket.actor.0).unwrap();
    let mut saved = saved.clone();
    let CraftingDecision::Tinker(recipe) = &mut ticket.decision else {
        panic!("tinker");
    };
    saved.player.entity.mutation_revision = recipe.expected_actor_revision;
    saved.player.entity.state.properties.skills.push(Property {
        id: 29,
        value: bace_content::Skill {
            level_from_pp: 0,
            sac: 2,
            pp: 0,
            init_level: 5,
            resistance_at_last_check: 0,
            last_used_time: 0.0,
        },
    });
    saved.ui.spellbook_filters = 3;
    recipe.actor_before_properties = crafting_properties(&saved.player.entity.state.properties);
    recipe.actor_properties = recipe.actor_before_properties.clone();
    if actor_counter {
        recipe.actor_properties.insert(
            PropertyKey {
                kind: PropertyKind::Int,
                id: 205,
            },
            PropertyValue::Int(1),
        );
    }
    recipe.actor_revision = recipe.expected_actor_revision + u64::from(actor_counter);
    let table = RankTable::new(&[0, 100, 200]).unwrap();
    let state = CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(29),
                experience_spent: 0,
                advancement: SkillAdvancement::Trained,
            },
            details: TraitDetails::Skill {
                initial_level: 5,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        100,
        recipe.expected_actor_revision,
    )
    .unwrap()
    .with_training(
        Arc::new(bace_character::SkillTrainingRules::new(&[]).unwrap()),
        0,
        &[],
    )
    .unwrap();
    let services = crate::native_player::restore_services(&saved).unwrap();
    let change = state
        .propose_proficiency(
            &services,
            &CharacterLevelTable::prepare(vec![0, 0, 1000], vec![0, 0, 0]).unwrap(),
            bace_character::ProficiencyUse {
                skill: 29,
                difficulty: 100,
                unix_time: 1800.0,
                olthoi: false,
            },
            actor_counter,
        )
        .unwrap()
        .unwrap();
    let mut snapshots = Vec::new();
    if actor_counter {
        let mut next = saved.clone();
        apply_properties(
            &mut next.player.entity.state.properties,
            &recipe.actor_before_properties,
            &recipe.actor_properties,
            None,
        )
        .unwrap();
        next.player.entity.mutation_revision = recipe.actor_revision;
        snapshots.push(SaveSnapshot {
            object_id: ticket.actor.0,
            mutation_revision: recipe.actor_revision,
            expected_version: version,
            bytes: next.encode().unwrap(),
        });
    }
    let item = online.inventory_baselines(ticket.actor.0).remove(0);
    snapshots.push(SaveSnapshot {
        object_id: item.entity.object_id,
        mutation_revision: item.entity.mutation_revision,
        expected_version: item.persisted_version,
        bytes: bace_storage_codec::ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: ItemSaveV2 {
                    entity: item.entity.clone(),
                    placement: item.placement.clone().unwrap(),
                },
                enchantments: item.enchantments.clone(),
            },
            construction: item.construction.clone(),
        }
        .encode()
        .unwrap(),
    });
    ticket.proficiency = Some(CraftingProficiency {
        change,
        vitals: vec![],
        vitae: None,
        skill_base: None,
        skill_maximum: false,
        experience: None,
    });
    let op = PlacementOperation {
        operation_id: "craft-test".into(),
        snapshots,
        participants: vec![ticket.actor.0, item.entity.object_id],
        leases: vec![lease],
        changes: vec![],
        storage_views: vec![],
    };
    (
        op,
        CraftingSavedPlayer {
            saved,
            persisted_version: version,
        },
        ticket,
    )
}
#[test]
fn proficiency_joins_existing_or_missing_recipe_player_once_and_preserves_ui_and_items() {
    for counter in [false, true] {
        let (operation, player, ticket) = fixture(counter);
        let item = operation
            .snapshots
            .iter()
            .find(|s| s.object_id != ticket.actor.0)
            .unwrap()
            .clone();
        let result = join_proficiency(operation, &player, &ticket).unwrap();
        assert_eq!(
            result
                .snapshots
                .iter()
                .filter(|s| s.object_id == ticket.actor.0)
                .count(),
            1
        );
        assert_eq!(
            result
                .snapshots
                .iter()
                .find(|s| s.object_id == item.object_id)
                .unwrap(),
            &item
        );
        let row = result
            .snapshots
            .iter()
            .find(|s| s.object_id == ticket.actor.0)
            .unwrap();
        let saved = PlayerSaveV6::decode(&row.bytes).unwrap();
        assert_eq!(
            row.mutation_revision,
            player.saved.player.entity.mutation_revision + 1
        );
        assert_eq!(saved.ui, player.saved.ui);
        assert_eq!(
            saved
                .player
                .entity
                .state
                .properties
                .int64s
                .iter()
                .find(|p| p.id == 2)
                .unwrap()
                .value,
            110
        );
        assert_eq!(
            saved
                .player
                .entity
                .state
                .properties
                .int64s
                .iter()
                .find(|p| p.id == 1)
                .unwrap()
                .value,
            110
        );
        let skill = saved
            .player
            .entity
            .state
            .properties
            .skills
            .iter()
            .find(|s| s.id == 29)
            .unwrap();
        assert_eq!(skill.value.pp, 100);
        assert_eq!(skill.value.level_from_pp, 1);
        assert_eq!(skill.value.last_used_time, 1800.0);
        assert_eq!(
            saved
                .player
                .entity
                .state
                .properties
                .ints
                .iter()
                .any(|p| p.id == 205 && p.value == 1),
            counter
        );
    }
}
#[test]
fn conflicting_before_skill_or_existing_player_row_rejects_the_complete_join() {
    let (mut operation, mut player, ticket) = fixture(true);
    let valid = player.saved.clone();
    player.saved.player.entity.state.properties.skills[0]
        .value
        .last_used_time = 42.0;
    assert!(join_proficiency(operation.clone(), &player, &ticket).is_err());
    player.saved = valid;
    operation
        .snapshots
        .iter_mut()
        .find(|s| s.object_id == ticket.actor.0)
        .unwrap()
        .expected_version += 1;
    assert!(join_proficiency(operation, &player, &ticket).is_err());
}
