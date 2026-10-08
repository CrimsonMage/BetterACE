//! Projection of validated per-character UI into the login description.
use bace_gameplay_api::CharacterUi;
use bace_wire::PlayerDescription;
pub fn project_ui(value: &CharacterUi, description: &mut PlayerDescription) {
    description.options1 = value.options1;
    description.options2 = value.options2;
    description.spellbook_filters = value.filters;
    description.spell_bars = value.bars.clone();
    description.shortcuts = value
        .shortcuts
        .iter()
        .map(|s| bace_wire::LoginShortcut {
            index: s.index,
            object_id: s.object,
            spell_id: s.spell,
            layer: s.layer,
        })
        .collect();
    description.desired_components = value.components.clone();
    description.gameplay_options = value.gameplay.clone();
}
