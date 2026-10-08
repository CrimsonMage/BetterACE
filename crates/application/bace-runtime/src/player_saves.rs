//! Compose one authoritative player revision without losing sibling supplements.
use bace_magic::EnchantmentRegistry;
use bace_simulation::OwnedCharacterState;
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};

/// Prepare every player-owned feature before world admission. Missing spell
/// definitions fail the whole preparation; saved entries are never discarded.
pub fn restore_player(
    loaded: &crate::game_login::LoadedPlayer,
    assets: &crate::character_assets::PreparedCharacterAssets,
    component_templates: &[u32],
    equipment_sets: &[bace_magic::EquippedSpellSet<'_>],
    resolve: impl FnMut(u32) -> Option<crate::enchantment_saves::EnchantmentDefinition>,
) -> Result<bace_simulation::OwnedPlayerState, PlayerSaveError> {
    if loaded.player.combat_recovery.is_some() || loaded.player.physical_recovery.is_some() {
        return Err(
            SaveCodecError::Invalid("explicit restore clock required for cast recovery").into(),
        );
    }
    restore_player_at(
        loaded,
        assets,
        0,
        component_templates,
        equipment_sets,
        resolve,
    )
}

pub fn restore_player_at(
    loaded: &crate::game_login::LoadedPlayer,
    assets: &crate::character_assets::PreparedCharacterAssets,
    now_unix_millis: u64,
    component_templates: &[u32],
    equipment_sets: &[bace_magic::EquippedSpellSet<'_>],
    resolve: impl FnMut(u32) -> Option<crate::enchantment_saves::EnchantmentDefinition>,
) -> Result<bace_simulation::OwnedPlayerState, PlayerSaveError> {
    restore_player_at_with_item_experience(
        loaded,
        assets,
        now_unix_millis,
        component_templates,
        equipment_sets,
        &[],
        resolve,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn restore_player_at_with_item_experience(
    loaded: &crate::game_login::LoadedPlayer,
    assets: &crate::character_assets::PreparedCharacterAssets,
    now_unix_millis: u64,
    component_templates: &[u32],
    equipment_sets: &[bace_magic::EquippedSpellSet<'_>],
    item_experience: &[bace_simulation::PreparedItemExperience],
    mut resolve: impl FnMut(u32) -> Option<crate::enchantment_saves::EnchantmentDefinition>,
) -> Result<bace_simulation::OwnedPlayerState, PlayerSaveError> {
    use crate::enchantment_saves::restore_saved_enchantments;
    if component_templates.len() > 4096
        || loaded.inventory.len() > 1023
        || equipment_sets.len() > 1024
    {
        return Err(SaveCodecError::Invalid("player preparation count").into());
    }
    let saved = &loaded.player;
    if loaded.binding.actor.0 != saved.player.entity.object_id
        || loaded.binding.account.0 != saved.player.account_id
    {
        return Err(SaveCodecError::Invalid("player preparation identity").into());
    }
    let progression = crate::progression_saves::restore_progression(saved, assets)
        .map_err(PlayerSaveError::Progression)?;
    let known_spells = saved
        .player
        .entity
        .state
        .properties
        .spell_book
        .iter()
        .map(|spell| {
            u32::try_from(spell.id)
                .ok()
                .filter(|id| (1..=65535).contains(id))
                .ok_or(SaveCodecError::Invalid("saved learned spell"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if known_spells.len() > 4096 {
        return Err(SaveCodecError::Invalid("saved learned spell count").into());
    }
    let mut character = OwnedCharacterState {
        native_services: Some(crate::native_player::restore_services(saved)?),
        contracts: Some(crate::native_player::restore_contracts(saved)?),
        progression,
        rares: saved.rares.map(crate::rare_saves::rare_state_from_save),
        ui: Some(bace_simulation::OwnedUiState {
            state: crate::ui_saves::restore_ui(saved)?,
            known_spells,
            component_templates: component_templates.to_vec(),
            entered: false,
        }),
    };
    let mut registry = restore_saved_enchantments(saved, 4096, &mut resolve)
        .map_err(PlayerSaveError::Enchantment)?;
    let mut owners = Vec::with_capacity(loaded.inventory.len());
    for item in &loaded.inventory {
        let equipped = matches!(item.placement,
            bace_storage_codec::ItemPlacementV2::Contained { container, equipped, .. } if container == loaded.binding.actor.0 && equipped != 0);
        // ACE resolves set assets only after a permanent effect passes the
        // ownership/equipment gate. Unrelated carried set items need no asset.
        let audits_set = registry.entries().iter().any(|entry| {
            entry.spec.duration == -1.0
                && entry.spell != 666
                && entry.caster == item.entity.object_id
                && (entry.metadata.has_spell_set_id || equipped)
        });
        let set_id = item
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 265)
            .map_or(0, |p| p.value);
        let set = if audits_set && set_id != 0 {
            Some(
                *equipment_sets
                    .iter()
                    .find(|set| set.id == set_id as u32)
                    .ok_or(SaveCodecError::Invalid("missing prepared equipment set"))?,
            )
        } else {
            None
        };
        owners.push(bace_magic::EquippedSpellOwner {
            id: item.entity.object_id,
            equipped,
            set,
        });
    }
    let normalized = registry
        .normalize_vitae()
        .map_err(PlayerSaveError::Registry)?;
    let audited = bace_magic::audit_equipped_spells(&mut registry, &owners)
        .map_err(PlayerSaveError::Registry)?;
    if normalized || audited {
        character
            .progression
            .touch_revision()
            .map_err(|_| SaveCodecError::Invalid("login cleanup revision overflow"))?;
    }
    let enchantments = Some(registry);
    let mut item_enchantments = Vec::new();
    for item in &loaded.inventory {
        if item.enchantments.is_empty() {
            continue;
        }
        let frozen = bace_storage_codec::ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: bace_storage_codec::ItemSaveV2 {
                    entity: item.entity.clone(),
                    placement: item.placement.clone(),
                },
                enchantments: item.enchantments.clone(),
            },
            construction: item.construction.clone(),
        };
        let registry = restore_saved_enchantments(&frozen, 4096, &mut resolve)
            .map_err(PlayerSaveError::Enchantment)?;
        item_enchantments.push((bace_types::EntityId(item.entity.object_id), registry));
    }
    let recovery = loaded
        .player
        .combat_recovery
        .map(|saved| {
            let state = crate::magic_recovery::prepare_cast_recovery(saved.state)?;
            bace_magic::CastRecoveryClock::restore(
                state,
                0.0,
                saved.elapsed_seconds(now_unix_millis)?,
            )
            .and_then(|clock| clock.snapshot(0.0))
            .map_err(|_| SaveCodecError::Invalid("invalid restored recovery clock"))
        })
        .transpose()?;
    let portal_links = Some(
        crate::portal_preparation::prepare_portal_links(
            &saved.player.entity.state,
            saved.player.entity.mutation_revision,
        )
        .map_err(|_| SaveCodecError::Invalid("invalid saved portal links"))?,
    );
    let item_experience = crate::item_experience::restore_item_experience(loaded, item_experience)?;
    Ok(bace_simulation::OwnedPlayerState {
        gag: Some(crate::staff_gags::restore_gag(&saved.player.entity.state)?),
        equipment_mana: None,
        chat_age: Some(
            u64::try_from(
                saved
                    .player
                    .entity
                    .state
                    .properties
                    .ints
                    .iter()
                    .find(|p| p.id == 125)
                    .map_or(0, |p| p.value),
            )
            .map_err(|_| SaveCodecError::Invalid("negative player age"))?,
        ),
        physical_recovery: saved
            .physical_recovery
            .map(|r| r.remaining_at(now_unix_millis))
            .transpose()?
            .unwrap_or(0.0),
        item_experience,
        death: Some(crate::player_death_state::restore_player_death_state(
            saved,
        )?),
        social: Some(crate::social_saves::restore_social(&saved.social)?),
        portal_links,
        world: None,
        recovery,
        character,
        enchantments,
        item_enchantments,
    })
}

/// Input is a read projection or a drained owner. This function does not mark
/// anything clean; the save coordinator must reconcile the returned revision.
pub fn freeze_player(
    saved: &PlayerSaveV6,
    character: &OwnedCharacterState,
    enchantments: Option<&EnchantmentRegistry>,
) -> Result<PlayerSaveV6, PlayerSaveError> {
    let mut next = compose_character(saved, CharacterSaveView::from(character), enchantments)?;
    validate_final_revision(saved, &mut next)?;
    Ok(next)
}

struct CharacterSaveView<'a> {
    progression: &'a bace_character::CharacterProgression,
    rares: Option<bace_gameplay_api::CharacterRareState>,
    ui: Option<&'a bace_simulation::OwnedUiState>,
    native_services: Option<&'a bace_character::CharacterServiceState>,
    contracts: Option<&'a bace_quests::ContractRegistry>,
}
impl<'a> From<&'a OwnedCharacterState> for CharacterSaveView<'a> {
    fn from(value: &'a OwnedCharacterState) -> Self {
        Self {
            progression: &value.progression,
            rares: value.rares,
            ui: value.ui.as_ref(),
            native_services: value.native_services.as_ref(),
            contracts: value.contracts.as_ref(),
        }
    }
}
impl<'a> From<&'a bace_simulation::CharacterReadSnapshot> for CharacterSaveView<'a> {
    fn from(value: &'a bace_simulation::CharacterReadSnapshot) -> Self {
        Self {
            progression: value.progression(),
            rares: value.rares(),
            ui: value.ui(),
            native_services: value.native_services(),
            contracts: value.contracts(),
        }
    }
}
fn compose_character(
    saved: &PlayerSaveV6,
    character: CharacterSaveView<'_>,
    enchantments: Option<&EnchantmentRegistry>,
) -> Result<PlayerSaveV6, PlayerSaveError> {
    let revision = character.progression.revision();
    let mut next = crate::progression_saves::freeze_progression(saved, character.progression)
        .map_err(PlayerSaveError::Progression)?;
    let ui = character
        .ui
        .ok_or(SaveCodecError::Invalid("missing UI owner"))?;
    if ui.known_spells.len() > 4096 || ui.known_spells.iter().any(|id| !(1..=65535).contains(id)) {
        return Err(SaveCodecError::Invalid("invalid authoritative spellbook").into());
    }
    let known: std::collections::BTreeSet<_> = ui.known_spells.iter().copied().collect();
    if known.len() != ui.known_spells.len() {
        return Err(SaveCodecError::Invalid("duplicate authoritative spellbook spell").into());
    }
    next.player
        .entity
        .state
        .properties
        .spell_book
        .retain(|entry| known.contains(&(entry.id as u32)));
    for &id in &ui.known_spells {
        if !next
            .player
            .entity
            .state
            .properties
            .spell_book
            .iter()
            .any(|entry| entry.id == id as i32)
        {
            next.player
                .entity
                .state
                .properties
                .spell_book
                .push(bace_content::Property {
                    id: id as i32,
                    value: 1.0,
                });
        }
    }
    let previous_ui = crate::ui_saves::restore_ui(saved)?;
    if previous_ui != ui.state {
        // Both supplements are compared against the same prior aggregate,
        // not against a sibling that already installed the new revision.
        let frozen = crate::ui_saves::freeze_ui(saved, &ui.state, revision)?;
        next.player.metadata.options1 = frozen.player.metadata.options1;
        next.player.metadata.options2 = frozen.player.metadata.options2;
        next.player.metadata.spell_favorites = frozen.player.metadata.spell_favorites.clone();
        next.ui = frozen.previous.previous.previous.ui;
    }
    let rare = character.rares.map(crate::rare_saves::rare_state_to_save);
    if let Some(old) = saved.rares {
        let current = rare.ok_or(SaveCodecError::Invalid("missing rare owner"))?;
        if current.character != old.character
            || current.random_identity != old.random_identity
            || current.key_version != old.key_version
            || current.attempt_ordinal < old.attempt_ordinal
            || current.timer_ordinal < old.timer_ordinal
            || current.last_effective_time < old.last_effective_time
        {
            return Err(SaveCodecError::Invalid("rare state moved backwards").into());
        }
    }
    next.rares = rare;
    if let Some(registry) = enchantments {
        let mut frozen = saved.clone();
        crate::enchantment_saves::snapshot_enchantments(&mut frozen, registry, revision)
            .map_err(PlayerSaveError::Enchantment)?;
        next.enchantments = frozen.enchantments.clone();
    } else if !saved.enchantments.is_empty() {
        return Err(SaveCodecError::Invalid("missing enchantment owner").into());
    }
    match (character.native_services, character.contracts) {
        (Some(state), Some(contracts)) => {
            crate::native_player::freeze_services(&mut next, state, contracts)?
        }
        (None, None) if saved.contracts.is_empty() => (),
        _ => {
            return Err(
                SaveCodecError::Invalid("incomplete native character service owner").into(),
            );
        }
    }
    next.validate()?;
    Ok(next)
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerSaveError {
    #[error("invalid login registry: {0:?}")]
    Registry(bace_magic::RegistryError),
    #[error(transparent)]
    Codec(#[from] SaveCodecError),
    #[error("invalid progression snapshot: {0:?}")]
    Progression(crate::progression_saves::ProgressionSaveError),
    #[error(transparent)]
    Enchantment(crate::enchantment_saves::EnchantmentSaveError),
}

/// Compose the final drained player, including recovery, at one explicit clock.
pub fn freeze_player_state(
    saved: &PlayerSaveV6,
    owned: &bace_simulation::OwnedPlayerState,
    now_unix_millis: u64,
) -> Result<PlayerSaveV6, PlayerSaveError> {
    let mut next = compose_character(
        saved,
        CharacterSaveView::from(&owned.character),
        owned.enchantments.as_ref(),
    )?;
    freeze_supplements(
        saved,
        &mut next,
        PlayerSupplements {
            gag: owned.gag,
            chat_age: owned.chat_age,
            physical_recovery: owned.physical_recovery,
            death: owned.death.as_ref(),
            social: owned.social.as_ref(),
            portal_links: owned.portal_links.as_ref(),
            world: owned.world,
            recovery: owned.recovery,
        },
        now_unix_millis,
        true,
    )?;
    Ok(next)
}

/// Freeze an online read projection at its captured revision. This does not
/// acknowledge persistence, transfer gameplay ownership, or mark anything clean.
pub fn freeze_player_snapshot(
    saved: &PlayerSaveV6,
    snapshot: &bace_simulation::PlayerReadSnapshot,
    now_unix_millis: u64,
) -> Result<PlayerSaveV6, PlayerSaveError> {
    if snapshot.operation().is_some()
        || snapshot.binding().actor.0 != saved.player.entity.object_id
        || snapshot.binding().account.0 != saved.player.account_id
    {
        return Err(SaveCodecError::Invalid("player snapshot identity").into());
    }
    let mut next = compose_character(
        saved,
        CharacterSaveView::from(snapshot.character()),
        snapshot.enchantments(),
    )?;
    freeze_supplements(
        saved,
        &mut next,
        PlayerSupplements {
            gag: snapshot.gag(),
            chat_age: snapshot.chat_age(),
            physical_recovery: snapshot.physical_recovery(),
            death: snapshot.death(),
            social: snapshot.social(),
            portal_links: snapshot.portal_links(),
            world: Some(snapshot.world()),
            recovery: snapshot.recovery(),
        },
        now_unix_millis,
        true,
    )?;
    Ok(next)
}
struct PlayerSupplements<'a> {
    gag: Option<bace_simulation::GagRecovery>,
    chat_age: Option<u64>,
    physical_recovery: f64,
    death: Option<&'a bace_simulation::PlayerDeathState>,
    social: Option<&'a bace_social::SocialPreferences>,
    portal_links: Option<&'a bace_interactions::PortalLinks>,
    world: Option<bace_simulation::PlayerWorldSnapshot>,
    recovery: Option<bace_magic::CastRecovery>,
}
fn freeze_supplements(
    saved: &PlayerSaveV6,
    next: &mut PlayerSaveV6,
    state: PlayerSupplements<'_>,
    now_unix_millis: u64,
    check_revision: bool,
) -> Result<(), PlayerSaveError> {
    let physical = bace_storage_codec::PhysicalRecoverySaveV1 {
        captured_unix_millis: now_unix_millis,
        remaining_seconds: state.physical_recovery,
    };
    physical.validate()?;
    bace_storage_codec::validate_physical_recovery_transition(
        saved.physical_recovery,
        Some(physical),
        next.player.entity.mutation_revision != saved.player.entity.mutation_revision
            || !check_revision,
    )?;
    next.physical_recovery = if physical.remaining_seconds == 0.0
        && saved
            .physical_recovery
            .is_none_or(|old| old.remaining_seconds == 0.0)
    {
        saved.physical_recovery
    } else {
        Some(physical)
    };
    if let Some(death) = state.death {
        crate::player_death_state::freeze_player_death_state(next, death)?;
    }
    if let Some(gag) = state.gag {
        crate::staff_gags::overlay_gag(&mut next.player.entity.state, gag.state, false)?;
    } else if next
        .player
        .entity
        .state
        .properties
        .bools
        .iter()
        .any(|p| p.id == 111 && p.value)
    {
        return Err(SaveCodecError::Invalid("gag owner omitted").into());
    }
    if let Some(age) = state.chat_age {
        let age = i32::try_from(age).map_err(|_| SaveCodecError::Invalid("player age overflow"))?;
        let properties = &mut next.player.entity.state.properties.ints;
        if let Some(property) = properties.iter_mut().find(|p| p.id == 125) {
            if age < property.value {
                return Err(SaveCodecError::Invalid("player age regression").into());
            }
            property.value = age;
        } else if age != 0 {
            properties.push(bace_content::Property {
                id: 125,
                value: age,
            });
            properties.sort_by_key(|p| p.id);
        }
    }
    if state.chat_age.is_none()
        && saved
            .player
            .entity
            .state
            .properties
            .ints
            .iter()
            .any(|p| p.id == 125 && p.value > 0)
    {
        return Err(SaveCodecError::Invalid("missing player age owner").into());
    }
    if let Some(social) = state.social {
        next.social = crate::social_saves::freeze_social(social)?;
    } else if saved.social != Default::default() {
        return Err(SaveCodecError::Invalid("missing social preference owner").into());
    }
    if let Some(links) = state.portal_links {
        next.player.entity.state =
            crate::portal_preparation::freeze_portal_links(&next.player.entity.state, links)
                .map_err(|_| SaveCodecError::Invalid("invalid portal link owner"))?;
    }
    if let Some(world) = state.world {
        crate::world_saves::freeze_world(next, world)?;
    }
    if let Some(state) = state.recovery {
        let frozen = bace_storage_codec::CombatRecoverySaveV1 {
            captured_unix_millis: now_unix_millis,
            state: crate::magic_recovery::freeze_cast_recovery(state)?,
        };
        bace_storage_codec::validate_recovery_transition(saved.combat_recovery, Some(frozen))?;
        next.combat_recovery = Some(frozen);
    } else if saved.combat_recovery.is_some() {
        return Err(SaveCodecError::Invalid("missing cast recovery owner").into());
    }
    next.validate()?;
    if check_revision {
        validate_final_revision(saved, next)
    } else {
        Ok(())
    }
}

/// Check after World vitals/pose and portal links override their stale mirrors.
/// Re-capturing an unchanged recovery history is semantically read-only; its
/// countdown/age bounds were validated separately against the capture clocks.
fn validate_final_revision(
    saved: &PlayerSaveV6,
    next: &mut PlayerSaveV6,
) -> Result<(), PlayerSaveError> {
    if next.player.entity.mutation_revision != saved.player.entity.mutation_revision {
        return Ok(());
    }
    let recovery = next.combat_recovery;
    let same_history = match (saved.combat_recovery, recovery) {
        (None, None) => true,
        (Some(old), Some(new)) => old.state.revision == new.state.revision,
        (None, Some(new)) => {
            new.state.revision == 0
                && new.state.minimum_remaining == 0.0
                && new.state.streak_remaining == 0.0
                && new.state.last_success_school == 0
                && new.state.last_success_age == 0.0
        }
        _ => false,
    };
    next.combat_recovery = saved.combat_recovery;
    let physical = next.physical_recovery;
    next.physical_recovery = saved.physical_recovery;
    let unchanged = *next == *saved;
    next.physical_recovery = physical;
    next.combat_recovery = recovery;
    if !same_history || !unchanged {
        return Err(SaveCodecError::Invalid("changed player requires a dirty revision").into());
    }
    Ok(())
}

/// Compose the exact committed-before state of one held valuable operation.
/// This baseline is not a routine save: its consumer MUST overlay the operation's
/// new revision/effects and commit through that operation's atomic save adapter.
/// The capture cannot acknowledge/release a reservation or adopt proposed effects.
pub fn freeze_player_operation_baseline(
    saved: &PlayerSaveV6,
    snapshot: &bace_simulation::PlayerReadSnapshot,
    operation: bace_simulation::PlayerSnapshotOperation,
    before_revision: u64,
    now_unix_millis: u64,
) -> Result<PlayerSaveV6, PlayerSaveError> {
    if snapshot.operation() != Some((operation, before_revision))
        || snapshot.character().progression().revision() != before_revision
        || snapshot.binding().actor.0 != saved.player.entity.object_id
        || snapshot.binding().account.0 != saved.player.account_id
    {
        return Err(SaveCodecError::Invalid("valuable player snapshot identity").into());
    }
    let mut next = compose_character(
        saved,
        CharacterSaveView::from(snapshot.character()),
        snapshot.enchantments(),
    )?;
    freeze_supplements(
        saved,
        &mut next,
        PlayerSupplements {
            gag: snapshot.gag(),
            chat_age: snapshot.chat_age(),
            physical_recovery: snapshot.physical_recovery(),
            death: snapshot.death(),
            social: snapshot.social(),
            portal_links: snapshot.portal_links(),
            world: Some(snapshot.world()),
            recovery: snapshot.recovery(),
        },
        now_unix_millis,
        false,
    )?;
    Ok(next)
}
