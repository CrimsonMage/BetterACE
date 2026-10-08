//! Prepare immutable name checks from verified DAT and accepted content inputs.
//! Asset/catalog admission and durable uniqueness are separate owner gates.
use bace_character::{NameError, NamePolicy};
use bace_dat::TabooTable;

pub fn prepare_name_policy(
    table: &TabooTable,
    creature_names: &[String],
) -> Result<NamePolicy, NameError> {
    let patterns = table
        .entries
        .first()
        .map_or(&[][..], |entry| entry.patterns.as_slice());
    NamePolicy::prepare(patterns, creature_names)
}
