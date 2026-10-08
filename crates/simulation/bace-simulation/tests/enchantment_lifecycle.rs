use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_entity::Actor;
use bace_gameplay_api::{
    AttributeId, CharacterBinding, CharacterRareState, ProgressionTarget, SessionId,
    SkillAdvancement,
};
use bace_geometry::{Aabb, Vec3};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::{CharacterRegistrationError, Kernel, OwnedCharacterState, OwnedUiState};
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;
use std::sync::Arc;

fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(4),
        session: SessionId(9),
    }
}
fn kernel() -> Kernel {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let body = Body::spawn(
        world.scene(cell).unwrap(),
        Vec3::new(0.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 5.0,
            jump_impulse: 5.0,
        },
    )
    .unwrap();
    world
        .insert(Actor {
            id: EntityId(1),
            cell,
            body,
        })
        .unwrap();
    let mut kernel = Kernel::with_gameplay_limits(world, 16, 16, 16).unwrap();
    let table = RankTable::new(&[0, 1, 10, 100]).unwrap();
    let progression = CharacterProgression::new(
        &[TraitProgress {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            experience_spent: 0,
            advancement: SkillAdvancement::Inactive,
        }],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        100,
        0,
    )
    .unwrap();
    kernel
        .register_character_with_rare(
            binding(),
            OwnedCharacterState {
                native_services: None,
                contracts: None,
                progression,
                rares: Some(CharacterRareState {
                    character: 1,
                    random_identity: [3; 16],
                    key_version: 1,
                    attempt_ordinal: 27,
                    timer_ordinal: 8,
                    next_realtime_at: Some(500),
                    last_effective_time: 200,
                }),
                ui: Some(OwnedUiState {
                    state: Default::default(),
                    known_spells: vec![123],
                    component_templates: vec![],
                    entered: true,
                }),
            },
        )
        .unwrap();
    kernel
}
fn registry(duration: f64) -> EnchantmentRegistry {
    EnchantmentRegistry::restore(
        16,
        20,
        vec![EnchantmentEntry {
            spell: 123,
            caster: 100,
            school: MagicSchool::Creature,
            spec: EnchantmentSpec {
                category: 23,
                power: 100,
                duration,
                layer: 7,
                stat_type: 0x2008000,
                stat_key: 7,
                value: 12.5,
                beneficial: true,
                set_id: None,
            },
            start_time: -15.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: EnchantmentMetadata {
                enchantment_category: 12,
                degrade_modifier: 0.1,
                degrade_limit: 0.2,
                last_time_degraded: -3.0,
                ..Default::default()
            },
        }],
    )
    .unwrap()
}
fn ticks(kernel: &mut Kernel, count: usize) {
    for _ in 0..count {
        assert!(kernel.step().unwrap().is_empty());
    }
}
fn register_item(kernel: &mut Kernel) {
    kernel
        .register_inventory_container(InventoryContainer {
            id: EntityId(1),
            revision: 0,
            root_owner: Some(EntityId(1)),
            slots: 10,
            pack_slots: 10,
            burden_limit: 10000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
    kernel
        .register_inventory_item(InventoryItem {
            structure: None,
            id: EntityId(50),
            revision: 5,
            template: 100,
            stack_key: 1,
            place: ItemPlace::Contained {
                container: EntityId(1),
                slot: 0,
                equipped: 0,
            },
            stack: 1,
            maximum_stack: 1,
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
        })
        .unwrap();
}

#[test]
fn loaded_player_registry_ticks_dirty_then_transfers_with_ui_and_rng() {
    let mut kernel = kernel();
    kernel
        .register_magic_registry(EntityId(1), registry(60.0), true)
        .unwrap();
    assert!(matches!(
        kernel.take_character(binding()),
        Err(CharacterRegistrationError::CompleteStateRequired)
    ));
    ticks(&mut kernel, 150);
    assert_eq!(
        kernel.magic_registry(EntityId(1)).unwrap().entries()[0].start_time,
        -20.0
    );
    assert_eq!(kernel.character(EntityId(1)).unwrap().revision(), 1);
    let state = kernel.take_player_state(binding()).unwrap();
    assert_eq!(state.character.progression.revision(), 1);
    assert_eq!(state.character.rares.unwrap().attempt_ordinal, 27);
    assert!(state.character.ui.unwrap().entered);
    let registry = state.enchantments.unwrap();
    assert_eq!(registry.entries()[0].metadata.last_time_degraded, -3.0);
    assert!(kernel.magic_registry(EntityId(1)).is_none());
    assert!(!kernel.has_characters());
    ticks(&mut kernel, 300);
    assert_eq!(registry.entries()[0].start_time, -20.0);
}

#[test]
fn inactive_item_registry_stops_aging_and_active_time_changes_item_revision() {
    let mut kernel = kernel();
    register_item(&mut kernel);
    kernel
        .register_magic_registry(EntityId(50), registry(60.0), false)
        .unwrap();
    ticks(&mut kernel, 300);
    assert_eq!(
        kernel.magic_registry(EntityId(50)).unwrap().entries()[0].start_time,
        -15.0
    );
    assert_eq!(kernel.inventory_item(EntityId(50)).unwrap().revision, 5);
    kernel
        .set_magic_registry_active(EntityId(50), true)
        .unwrap();
    ticks(&mut kernel, 150);
    assert_eq!(
        kernel.magic_registry(EntityId(50)).unwrap().entries()[0].start_time,
        -20.0
    );
    assert_eq!(kernel.inventory_item(EntityId(50)).unwrap().revision, 6);
    let owned = kernel.take_magic_registry(EntityId(50)).unwrap();
    ticks(&mut kernel, 300);
    assert_eq!(owned.entries()[0].start_time, -20.0);
    kernel
        .register_magic_registry(EntityId(50), owned, true)
        .unwrap();
    ticks(&mut kernel, 150);
    assert_eq!(
        kernel.magic_registry(EntityId(50)).unwrap().entries()[0].start_time,
        -25.0
    );
    assert_eq!(kernel.inventory_item(EntityId(50)).unwrap().revision, 7);
}

#[test]
fn expiry_output_and_reservation_block_logout_without_losing_either_owner() {
    let mut kernel = kernel();
    kernel
        .register_magic_registry(EntityId(1), registry(20.0), true)
        .unwrap();
    kernel.reserve_magic_registry(EntityId(1), true).unwrap();
    ticks(&mut kernel, 150);
    assert!(matches!(
        kernel.take_player_state(binding()),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    assert!(kernel.has_characters());
    assert!(kernel.magic_registry(EntityId(1)).is_some());
    kernel.reserve_magic_registry(EntityId(1), false).unwrap();
    ticks(&mut kernel, 1);
    assert!(matches!(
        kernel.take_player_state(binding()),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    assert!(kernel.take_magic_event().is_some());
    let state = kernel.take_player_state(binding()).unwrap();
    assert!(state.enchantments.unwrap().entries().is_empty());
    assert!(!kernel.has_characters());
}

#[test]
fn failed_target_admission_returns_saved_registry_unchanged() {
    let mut kernel = kernel();
    let registry = registry(60.0);
    let rows = registry.entries().to_vec();
    let (_, registry) = kernel
        .register_magic_registry(EntityId(999), registry, true)
        .unwrap_err();
    assert_eq!(registry.entries(), rows);
    assert_eq!(registry.revision(), 20);
    assert!(kernel.magic_registry(EntityId(999)).is_none());
}

#[test]
fn player_logout_atomically_transfers_contained_registries_and_stops_offline_aging() {
    let mut kernel = kernel();
    register_item(&mut kernel);
    kernel
        .register_magic_registry(EntityId(1), registry(60.0), true)
        .unwrap();
    kernel
        .register_magic_registry(EntityId(50), registry(60.0), true)
        .unwrap();
    kernel.reserve_magic_registry(EntityId(50), true).unwrap();
    assert!(matches!(
        kernel.take_player_state(binding()),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    assert!(kernel.has_characters());
    assert!(kernel.magic_registry(EntityId(1)).is_some());
    assert!(kernel.magic_registry(EntityId(50)).is_some());
    kernel.reserve_magic_registry(EntityId(50), false).unwrap();
    ticks(&mut kernel, 150);
    let state = kernel.take_player_state(binding()).unwrap();
    assert_eq!(state.item_enchantments.len(), 1);
    assert_eq!(state.item_enchantments[0].0, EntityId(50));
    assert_eq!(state.item_enchantments[0].1.entries()[0].start_time, -20.0);
    assert!(kernel.magic_registry(EntityId(50)).is_none());
    assert!(!kernel.has_magic_state());
    ticks(&mut kernel, 300);
    assert_eq!(state.item_enchantments[0].1.entries()[0].start_time, -20.0);
}

#[test]
fn house_registry_ticks_dirty_and_retains_timer_debt_through_housing_receipt() {
    use bace_gameplay_api::{ActionContext, HousingRequest};
    use bace_housing::{HousingActor, HousingState, PurchaseRules};
    use bace_simulation::{HousingReceipt, HousingRegistration};
    let mut kernel = kernel();
    let house = EntityId(100);
    kernel
        .register_housing(HousingRegistration {
            slumlord: EntityId(101),
            state: HousingState {
                house,
                revision: 1,
                owner: Some(EntityId(1)),
                allegiance_monarch: None,
                generation: 1,
                purchased_at: 1,
                period_start: 1,
                rent_due: 1000,
                interval_seconds: 1000,
                maintenance_free: true,
                open: false,
                storage_open: false,
                hooks_visible: true,
                guests: vec![],
                rent: vec![],
            },
            rules: PurchaseRules {
                minimum_level: 0,
                requires_monarch: false,
                minimum_rank: 0,
                account_age_seconds: 0,
                cooldown_seconds: 0,
                apartment: false,
                buy: vec![],
            },
            owner_account: Some(4),
        })
        .unwrap();
    kernel
        .register_magic_registry(house, registry(120.0), true)
        .unwrap();
    ticks(&mut kernel, 150);
    assert_eq!(kernel.housing_state(house).unwrap().revision, 2);
    assert_eq!(
        kernel.magic_registry(house).unwrap().entries()[0].start_time,
        -20.0
    );
    let actor = HousingActor {
        actor: EntityId(1),
        account: 4,
        level: 100,
        monarch: false,
        allegiance_rank: 0,
        account_age_seconds: 0,
        previous_purchase: 0,
        owns_house: true,
        in_range: true,
    };
    let context = |sequence| ActionContext {
        actor: EntityId(1),
        account: AccountId(4),
        session: SessionId(9),
        sequence,
    };
    // A caller-owned manual reservation cannot be stolen/released by housing.
    kernel.reserve_magic_registry(house, true).unwrap();
    assert!(
        kernel
            .propose_housing(context(1), HousingRequest::SetOpen(true), actor, &[], 5)
            .is_err()
    );
    assert!(kernel.take_magic_registry(house).is_err());
    kernel.reserve_magic_registry(house, false).unwrap();
    let operation = kernel
        .propose_housing(context(1), HousingRequest::SetOpen(true), actor, &[], 5)
        .unwrap();
    let ticket = kernel.take_housing_proposal().unwrap();
    assert_eq!(ticket.operation, operation);
    assert_eq!(ticket.proposal.before.revision, 2);
    ticks(&mut kernel, 300);
    assert_eq!(kernel.housing_state(house).unwrap().revision, 2);
    assert_eq!(
        kernel.magic_registry(house).unwrap().entries()[0].start_time,
        -20.0
    );
    assert!(kernel.take_magic_registry(house).is_err());
    kernel
        .confirm_housing_committed(HousingReceipt {
            operation,
            house,
            revision: ticket.proposal.after.revision,
            generation: ticket.proposal.after.generation,
            owner: ticket.proposal.after.owner,
        })
        .unwrap();
    assert_eq!(kernel.housing_state(house).unwrap().revision, 3);
    ticks(&mut kernel, 1);
    assert_eq!(kernel.housing_state(house).unwrap().revision, 4);
    assert_eq!(
        kernel.magic_registry(house).unwrap().entries()[0].start_time,
        -30.0
    );
    let operation = kernel
        .propose_housing(context(2), HousingRequest::SetOpen(false), actor, &[], 16)
        .unwrap();
    ticks(&mut kernel, 150);
    kernel.reject_housing(operation).unwrap();
    ticks(&mut kernel, 1);
    assert!(kernel.housing_state(house).unwrap().open);
    assert_eq!(kernel.housing_state(house).unwrap().revision, 5);
    assert_eq!(
        kernel.magic_registry(house).unwrap().entries()[0].start_time,
        -35.0
    );
    assert!(kernel.take_magic_registry(house).is_ok());
}
