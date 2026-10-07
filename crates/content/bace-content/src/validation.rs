use std::collections::BTreeSet;

use thiserror::Error;

use crate::{Property, WeenieV1};

#[derive(Clone, Copy, Debug)]
pub struct ContentLimits {
    pub max_entries: usize,
    pub max_string_bytes: usize,
}
impl Default for ContentLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_string_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContentError {
    #[error("unsupported native content schema {0}")]
    Schema(u16),
    #[error("weenie ID must be nonzero")]
    Identity,
    #[error("class_name must be nonempty and at most 255 bytes")]
    ClassName,
    #[error("duplicate property ID in {0}")]
    DuplicateProperty(&'static str),
    #[error("content exceeds configured entry/string limits")]
    Limit,
    #[error("non-finite numeric field in content")]
    NonFinite,
    #[error("duplicate weenie ID in candidate batch: {0}")]
    DuplicateWeenie(u32),
    #[error("duplicate class name in catalog: {0}")]
    DuplicateClass(String),
    #[error("catalog publication revision must increase")]
    StaleRevision,
}

fn dictionary<T, K: Ord>(
    values: &[Property<T, K>],
    name: &'static str,
) -> Result<(), ContentError> {
    let mut ids = BTreeSet::new();
    if values.iter().any(|p| !ids.insert(&p.id)) {
        return Err(ContentError::DuplicateProperty(name));
    }
    Ok(())
}

fn finite(values: impl IntoIterator<Item = f64>) -> Result<(), ContentError> {
    if values.into_iter().any(|x| !x.is_finite()) {
        return Err(ContentError::NonFinite);
    }
    Ok(())
}
fn string(value: &str, limits: ContentLimits) -> Result<(), ContentError> {
    if value.len() > limits.max_string_bytes {
        return Err(ContentError::Limit);
    }
    Ok(())
}

impl WeenieV1 {
    pub fn validate(&self, limits: ContentLimits) -> Result<(), ContentError> {
        if self.schema_version != 1 {
            return Err(ContentError::Schema(self.schema_version));
        }
        if self.weenie_id == 0 {
            return Err(ContentError::Identity);
        }
        if self.class_name.trim().is_empty() || self.class_name.len() > 255 {
            return Err(ContentError::ClassName);
        }
        let p = &self.properties;
        let mut entries = 0_usize;
        macro_rules! dictionaries { ($($field:ident),+ $(,)?) => { $(
            entries = entries.saturating_add(p.$field.len());
            dictionary(&p.$field, stringify!($field))?;
        )+ }; }
        dictionaries!(
            bools,
            data_ids,
            floats,
            instance_ids,
            ints,
            int64s,
            strings,
            positions,
            spell_book,
            attributes,
            secondary_attributes,
            body_parts,
            skills
        );
        for count in [
            p.animation_parts.len(),
            p.palettes.len(),
            p.texture_maps.len(),
            p.create_list.len(),
            p.emotes.len(),
            p.event_filter.len(),
            p.generators.len(),
            p.book_pages.len(),
        ] {
            entries = entries.saturating_add(count);
        }
        entries = p
            .emotes
            .iter()
            .fold(entries, |count, e| count.saturating_add(e.actions.len()));
        if let Some(metadata) = &p.authoring_metadata {
            entries = entries.saturating_add(metadata.changelog.len());
            for text in [
                &metadata.modified_by,
                &metadata.user_change_summary,
                &metadata.comments,
            ]
            .into_iter()
            .flatten()
            {
                string(text, limits)?;
            }
            for entry in &metadata.changelog {
                for text in [&entry.created, &entry.author, &entry.comment]
                    .into_iter()
                    .flatten()
                {
                    string(text, limits)?;
                }
            }
        }
        if entries > limits.max_entries {
            return Err(ContentError::Limit);
        }
        if p.event_filter.iter().collect::<BTreeSet<_>>().len() != p.event_filter.len() {
            return Err(ContentError::DuplicateProperty("event_filter"));
        }
        for value in &p.strings {
            string(&value.value, limits)?;
        }
        if let Some(value) = &self.last_modified {
            string(value, limits)?;
        }
        finite(p.floats.iter().map(|v| v.value))?;
        finite(p.spell_book.iter().map(|v| f64::from(v.value)))?;
        for value in &p.positions {
            let v = &value.value;
            finite(
                [
                    v.position_x,
                    v.position_y,
                    v.position_z,
                    v.rotation_w,
                    v.rotation_x,
                    v.rotation_y,
                    v.rotation_z,
                ]
                .map(f64::from),
            )?;
        }
        for value in &p.skills {
            finite([value.value.last_used_time])?;
        }
        for value in &p.body_parts {
            let v = &value.value;
            finite(
                [
                    v.d_var, v.hlf, v.mlf, v.llf, v.hrf, v.mrf, v.lrf, v.hlb, v.mlb, v.llb, v.hrb,
                    v.mrb, v.lrb,
                ]
                .map(f64::from),
            )?;
        }
        for value in &p.create_list {
            finite([f64::from(value.shade)])?;
        }
        for page in &p.book_pages {
            for text in [&page.author_name, &page.author_account, &page.page_text]
                .into_iter()
                .flatten()
            {
                string(text, limits)?;
            }
        }
        for generator in &p.generators {
            finite([f64::from(generator.probability)])?;
            finite(
                [
                    generator.delay,
                    generator.shade,
                    generator.origin_x,
                    generator.origin_y,
                    generator.origin_z,
                    generator.angles_w,
                    generator.angles_x,
                    generator.angles_y,
                    generator.angles_z,
                ]
                .into_iter()
                .flatten()
                .map(f64::from),
            )?;
        }
        for emote in &p.emotes {
            finite([f64::from(emote.probability)])?;
            finite(
                [emote.min_health, emote.max_health]
                    .into_iter()
                    .flatten()
                    .map(f64::from),
            )?;
            if let Some(text) = &emote.quest {
                string(text, limits)?;
            }
            for action in &emote.actions {
                finite([f64::from(action.delay), f64::from(action.extent)])?;
                finite(
                    [action.min_dbl, action.max_dbl, action.percent]
                        .into_iter()
                        .flatten(),
                )?;
                finite(
                    [
                        action.shade,
                        action.origin_x,
                        action.origin_y,
                        action.origin_z,
                        action.angles_w,
                        action.angles_x,
                        action.angles_y,
                        action.angles_z,
                    ]
                    .into_iter()
                    .flatten()
                    .map(f64::from),
                )?;
                for text in [&action.message, &action.test_string].into_iter().flatten() {
                    string(text, limits)?;
                }
            }
        }
        Ok(())
    }

    /// Only mathematical dictionaries/sets are sorted; authored sequence order
    /// remains intact. Validate first so duplicates cannot be hidden by sorting.
    pub fn canonicalize(&mut self) {
        let p = &mut self.properties;
        macro_rules! sort { ($($field:ident),+ $(,)?) => { $(p.$field.sort_by_key(|v| v.id);)+ }; }
        sort!(
            bools,
            data_ids,
            floats,
            instance_ids,
            ints,
            int64s,
            strings,
            positions,
            spell_book,
            attributes,
            secondary_attributes,
            body_parts,
            skills
        );
        p.event_filter.sort_unstable();
    }
}
