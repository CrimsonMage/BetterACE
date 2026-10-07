use crate::staging::{SqlStagingError, StagingManifest};
use bace_content::WeenieTemplate;
use std::collections::BTreeMap;

/// This DTO can currently account for weenie tables only. Reject any populated
/// non-weenie table even if an adapter forgets to list it as unsupported.
pub(crate) fn validate(
    manifest: &StagingManifest,
    weenies: &[WeenieTemplate],
) -> Result<(), SqlStagingError> {
    let mut actual = BTreeMap::new();
    actual.insert("weenie".to_owned(), weenies.len() as u64);
    for weenie in weenies {
        let p = &weenie.properties;
        for (table, count) in [
            ("anim_part", p.animation_parts.len()),
            ("attribute", p.attributes.len()),
            ("attribute_2nd", p.secondary_attributes.len()),
            ("body_part", p.body_parts.len()),
            ("book", usize::from(p.book.is_some())),
            ("book_page_data", p.book_pages.len()),
            ("bool", p.bools.len()),
            ("create_list", p.create_list.len()),
            ("d_i_d", p.data_ids.len()),
            ("emote", p.emotes.len()),
            (
                "emote_action",
                p.emotes.iter().map(|e| e.actions.len()).sum(),
            ),
            ("event_filter", p.event_filter.len()),
            ("float", p.floats.len()),
            ("generator", p.generators.len()),
            ("i_i_d", p.instance_ids.len()),
            ("int", p.ints.len()),
            ("int64", p.int64s.len()),
            ("palette", p.palettes.len()),
            ("position", p.positions.len()),
            ("skill", p.skills.len()),
            ("spell_book", p.spell_book.len()),
            ("string", p.strings.len()),
            ("texture_map", p.texture_maps.len()),
        ] {
            *actual
                .entry(format!("weenie_properties_{table}"))
                .or_insert(0) += count as u64;
        }
    }
    let unsupported: Vec<_> = manifest
        .table_row_counts
        .iter()
        .filter(|(name, count)| **count > 0 && !actual.contains_key(*name))
        .map(|(name, _)| name.clone())
        .collect();
    if !unsupported.is_empty() {
        return Err(SqlStagingError::UnsupportedTables(unsupported));
    }
    if actual
        .iter()
        .any(|(name, count)| manifest.table_row_counts.get(name).copied().unwrap_or(0) != *count)
    {
        return Err(SqlStagingError::Extraction(
            "source/extracted table row counts do not match".into(),
        ));
    }
    Ok(())
}
