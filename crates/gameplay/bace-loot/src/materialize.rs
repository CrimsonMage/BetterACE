//! Frozen item proposal construction. Known spell insertion's 2.0 value follows
//! pinned ACE BiotaExtensions.GetOrAddKnownSpell (47edade3bd3f6044b676d4eb877c4965c7eda62b).
use crate::{GraphError, LootDrop};
use bace_content::{Property, WeenieV1};
use bace_gameplay_api::GeneratedItemMutation as M;
fn set<T>(values: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(old) = values.iter_mut().find(|p| p.id == id) {
        old.value = value;
    } else {
        values.push(Property { id, value });
        values.sort_by_key(|p| p.id);
    }
}
pub fn materialize_drop(drop: &LootDrop, template: &WeenieV1) -> Result<WeenieV1, GraphError> {
    template
        .validate(Default::default())
        .map_err(|e| GraphError::Invalid(e.to_string()))?;
    if drop.template != template.weenie_id
        || drop.stack == 0
        || drop.stack > i32::MAX as u32
        || drop.mutations.len() > 128
    {
        return Err(GraphError::Invalid("loot item identity/count".into()));
    }
    let mut state = template.clone();
    let mut keys = std::collections::BTreeSet::new();
    for value in &drop.mutations {
        if !keys.insert(crate::mutation::key(value)) {
            return Err(GraphError::Invalid("duplicate item mutation".into()));
        }
        match value {
            M::Int(id, v) => set(&mut state.properties.ints, *id, *v),
            M::Int64(id, v) => set(&mut state.properties.int64s, *id, *v),
            M::Float(id, v) => set(&mut state.properties.floats, *id, *v),
            M::DataId(id, v) => set(&mut state.properties.data_ids, *id, *v),
            M::Bool(id, v) => set(&mut state.properties.bools, *id, *v),
            M::Spell(id) => {
                if *id == 0 || *id > u32::from(u16::MAX) {
                    return Err(GraphError::Invalid("loot spell identity".into()));
                }
                if !state
                    .properties
                    .spell_book
                    .iter()
                    .any(|p| p.id == *id as i32)
                {
                    state.properties.spell_book.push(Property {
                        id: *id as i32,
                        value: 2.0,
                    });
                }
            }
        }
    }
    let max = state
        .properties
        .ints
        .iter()
        .find(|p| p.id == 11)
        .map(|p| p.value);
    if drop.stack > 1 && max.is_none() || max.is_some_and(|max| max <= 0 || drop.stack > max as u32)
    {
        return Err(GraphError::Invalid(
            "loot stack exceeds template capacity".into(),
        ));
    }
    if max.is_some() {
        set(&mut state.properties.ints, 12, drop.stack as i32);
        for (unit, total) in [(13, 5), (15, 19)] {
            if let Some(value) = state
                .properties
                .ints
                .iter()
                .find(|p| p.id == unit)
                .map(|p| p.value)
            {
                let value = value
                    .checked_mul(drop.stack as i32)
                    .ok_or_else(|| GraphError::Invalid("loot stack totals overflow".into()))?;
                set(&mut state.properties.ints, total, value);
            }
        }
    }
    state.properties.spell_book.sort_by_key(|p| p.id);
    state
        .validate(Default::default())
        .map_err(|e| GraphError::Invalid(e.to_string()))?;
    Ok(state)
}
