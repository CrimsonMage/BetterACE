//! Lossless adapters between character UI state and frozen player schema 3.
use bace_gameplay_api::{CharacterUi, UiShortcut};
use bace_storage_codec::{
    ComponentPreferenceV1, PlayerSaveV6, SaveCodecError, ShortcutSaveV1, SpellFavoriteV1,
};
pub fn restore_ui(saved: &PlayerSaveV6) -> Result<CharacterUi, SaveCodecError> {
    saved.validate()?;
    let metadata = &saved.player.metadata;
    let mut value = CharacterUi {
        options1: metadata.options1,
        options2: metadata.options2,
        filters: saved.ui.spellbook_filters,
        shortcuts: saved
            .ui
            .shortcuts
            .iter()
            .map(|s| UiShortcut {
                index: s.index,
                object: s.object_id,
                spell: s.spell_id,
                layer: s.layer,
            })
            .collect(),
        bars: Default::default(),
        components: saved
            .ui
            .desired_components
            .iter()
            .map(|v| (v.template_id, v.quantity))
            .collect(),
        gameplay: saved.ui.gameplay_options.clone(),
    };
    let mut favorites = metadata.spell_favorites.clone();
    favorites.sort_by_key(|v| (v.bar, v.position));
    for favorite in favorites {
        let bar = value
            .bars
            .get_mut(
                favorite
                    .bar
                    .checked_sub(1)
                    .ok_or(SaveCodecError::Invalid("spell bar"))? as usize,
            )
            .ok_or(SaveCodecError::Invalid("spell bar"))?;
        if favorite.position as usize != bar.len() + 1 {
            return Err(SaveCodecError::Invalid("noncontiguous spell bar"));
        }
        bar.push(favorite.spell_id);
    }
    bace_character::ui::validate(&value).map_err(|_| SaveCodecError::Invalid("saved UI"))?;
    Ok(value)
}
pub fn freeze_ui(
    saved: &PlayerSaveV6,
    value: &CharacterUi,
    revision: u64,
) -> Result<PlayerSaveV6, SaveCodecError> {
    saved.validate()?;
    bace_character::ui::validate(value).map_err(|_| SaveCodecError::Invalid("UI state"))?;
    if revision <= saved.player.entity.mutation_revision {
        return Err(SaveCodecError::Invalid("UI revision must advance"));
    }
    let mut next = saved.clone();
    next.player.entity.mutation_revision = revision;
    next.player.metadata.options1 = value.options1;
    next.player.metadata.options2 = value.options2;
    next.player.metadata.spell_favorites = value
        .bars
        .iter()
        .enumerate()
        .flat_map(|(bar, spells)| {
            spells
                .iter()
                .enumerate()
                .map(move |(position, spell)| SpellFavoriteV1 {
                    spell_id: *spell,
                    bar: bar as u32 + 1,
                    position: position as u32 + 1,
                })
        })
        .collect();
    next.ui.shortcuts = value
        .shortcuts
        .iter()
        .map(|s| ShortcutSaveV1 {
            index: s.index,
            object_id: s.object,
            spell_id: s.spell,
            layer: s.layer,
        })
        .collect();
    next.ui.spellbook_filters = value.filters;
    next.ui.desired_components = value
        .components
        .iter()
        .map(|(id, n)| ComponentPreferenceV1 {
            template_id: *id,
            quantity: *n,
        })
        .collect();
    next.ui.gameplay_options = value.gameplay.clone();
    next.validate()?;
    Ok(next)
}
pub fn project_ui(
    value: &CharacterUi,
    description: &mut bace_wire::PlayerDescription,
) -> Result<(), SaveCodecError> {
    bace_character::ui::validate(value).map_err(|_| SaveCodecError::Invalid("UI projection"))?;
    bace_replication::project_ui(value, description);
    Ok(())
}
