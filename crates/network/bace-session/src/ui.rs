//! Decode authenticated UI intent; the owner still checks live binding/entry.
use bace_gameplay_api::{ActionContext, UiRequest};
use bace_wire::{GameActionEnvelope, UiAction, UiInput, WireError};
pub fn decode_ui(
    bytes: &[u8],
    mut context: ActionContext,
    max_bytes: usize,
) -> Result<(ActionContext, UiRequest), WireError> {
    let envelope = GameActionEnvelope::decode(bytes, max_bytes)?;
    context.sequence = envelope.sequence;
    let request = match UiInput::decode(envelope.action, envelope.payload, max_bytes)?.action {
        UiAction::Options(v) => UiRequest::Options {
            options1: v.options1,
            options2: v.options2,
            filters: v.filters,
            shortcuts: v.shortcuts.map(|v| v.into_iter().map(shortcut).collect()),
            bars: v.bars,
            components: v.components,
            gameplay: v.gameplay,
        },
        UiAction::SingleOption { option, enabled } => {
            let (group, mask) =
                crate::ui_option_map::option_mask(option).ok_or(WireError::InvalidEncoding)?;
            UiRequest::SingleOption {
                group,
                mask,
                enabled,
            }
        }
        UiAction::AddShortcut(v) => UiRequest::AddShortcut(shortcut(v)),
        UiAction::RemoveShortcut(v) => UiRequest::RemoveShortcut(v),
        UiAction::AddFavorite {
            spell,
            position,
            bar,
        } => UiRequest::AddFavorite {
            spell,
            position,
            bar,
        },
        UiAction::RemoveFavorite { spell, bar } => UiRequest::RemoveFavorite { spell, bar },
        UiAction::Filters(v) => UiRequest::Filters(v),
        UiAction::Component { template, quantity } => UiRequest::Component { template, quantity },
    };
    Ok((context, request))
}
fn shortcut(v: bace_wire::UiShortcut) -> bace_gameplay_api::UiShortcut {
    bace_gameplay_api::UiShortcut {
        index: v.index,
        object: v.object,
        spell: v.spell,
        layer: v.layer,
    }
}
