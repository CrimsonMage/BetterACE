//! Atomic bounded preference edits. Valid spell/component identities come from
//! prepared authoritative inputs, not from the client's spell bar or refill list.
use bace_gameplay_api::{CharacterUi, UiError, UiRequest};
pub fn validate(value: &CharacterUi) -> Result<(), UiError> {
    if value.shortcuts.len() > 18
        || value.components.len() > 4096
        || value.gameplay.len() > 65536
        || value.bars.iter().any(|v| v.len() > 4096)
    {
        return Err(UiError::Capacity);
    }
    let mut seen = std::collections::BTreeSet::new();
    for s in &value.shortcuts {
        if s.index >= 18 || !seen.insert(s.index) {
            return Err(UiError::Invalid);
        }
    }
    seen.clear();
    for &(id, n) in &value.components {
        if id == 0 || n == 0 || n > i32::MAX as u32 || !seen.insert(id) {
            return Err(UiError::Invalid);
        }
    }
    for bar in &value.bars {
        seen.clear();
        if bar.iter().any(|id| *id == 0 || !seen.insert(*id)) {
            return Err(UiError::Invalid);
        }
    }
    Ok(())
}
pub fn apply(
    state: &mut CharacterUi,
    request: UiRequest,
    entered: bool,
    known_spells: &[u32],
    components: &[u32],
) -> Result<bool, UiError> {
    if !entered {
        return Err(UiError::BeforeEntry);
    }
    validate(state)?;
    let mut next = state.clone();
    match request {
        UiRequest::Options {
            options1,
            options2,
            filters,
            shortcuts,
            bars,
            components: desired,
            gameplay,
        } => {
            next.options1 = options1;
            if let Some(v) = options2 {
                next.options2 = v;
            }
            next.filters = filters;
            if let Some(v) = shortcuts {
                next.shortcuts = v;
            }
            if bars.len() > 8 {
                return Err(UiError::Invalid);
            }
            for (slot, bar) in bars.into_iter().enumerate() {
                if bar.iter().any(|id| !known_spells.contains(id)) {
                    return Err(UiError::UnknownSpell);
                }
                next.bars[slot] = bar;
            }
            if let Some(v) = desired {
                next.components.clear();
                for (id, n) in v {
                    component(&mut next, id, n, components)?;
                }
            }
            if let Some(v) = gameplay {
                next.gameplay = v;
            }
        }
        UiRequest::SingleOption {
            group,
            mask,
            enabled,
        } => {
            if !mask.is_power_of_two() {
                return Err(UiError::Invalid);
            }
            let value = match group {
                1 => &mut next.options1,
                2 => &mut next.options2,
                _ => return Err(UiError::Invalid),
            };
            if enabled {
                *value |= mask;
            } else {
                *value &= !mask;
            }
        }
        UiRequest::AddShortcut(s) => {
            next.shortcuts.retain(|v| v.index != s.index);
            next.shortcuts.push(s);
            next.shortcuts.sort_by_key(|v| v.index);
        }
        UiRequest::RemoveShortcut(index) => {
            if index >= 18 {
                return Err(UiError::Invalid);
            }
            next.shortcuts.retain(|s| s.index != index);
        }
        UiRequest::AddFavorite {
            spell,
            bar,
            position,
        } => {
            if !known_spells.contains(&spell) {
                return Err(UiError::UnknownSpell);
            }
            let list = next.bars.get_mut(bar as usize).ok_or(UiError::Invalid)?;
            if position as usize > list.len() {
                return Err(UiError::Invalid);
            }
            if list.contains(&spell) {
                return Ok(false);
            }
            list.insert(position as usize, spell);
        }
        UiRequest::RemoveFavorite { spell, bar } => next
            .bars
            .get_mut(bar as usize)
            .ok_or(UiError::Invalid)?
            .retain(|id| *id != spell),
        UiRequest::Filters(value) => next.filters = value,
        UiRequest::Component { template, quantity } => {
            component(&mut next, template, quantity, components)?
        }
    }
    validate(&next)?;
    let changed = next != *state;
    *state = next;
    Ok(changed)
}
fn component(state: &mut CharacterUi, id: u32, n: i32, valid: &[u32]) -> Result<(), UiError> {
    if id == 0 && n == -1 {
        state.components.clear();
        return Ok(());
    }
    if n < 0 {
        return Err(UiError::Invalid);
    }
    if !valid.contains(&id) {
        return Err(UiError::UnknownComponent);
    }
    state.components.retain(|(key, _)| *key != id);
    if n > 0 {
        state.components.push((id, n as u32));
        state.components.sort_unstable();
    }
    Ok(())
}
