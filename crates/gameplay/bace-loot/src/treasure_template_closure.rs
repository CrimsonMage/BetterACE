//! Cold dependency closure for the original typed WCID leaves, including gems
//! and scrolls. No world-template scan or runtime source parser is required.
use crate::{TreasureError, ace_tables};
use std::collections::BTreeSet;
pub fn pinned_treasure_templates() -> Result<Vec<u32>, TreasureError> {
    let mut ids = BTreeSet::new();
    let leaves = ace_tables::template_leaf_tables()
        .ok_or_else(|| TreasureError::MissingSourceTable("accepted template leaf fields".into()))?;
    for name in leaves {
        let (class, field) = name.split_once('.').ok_or(TreasureError::Bounds)?;
        let missing = || TreasureError::MissingSourceTable(format!("{class}.{field}"));
        let mut add = |value: i64| -> Result<(), TreasureError> {
            let id = u32::try_from(value).map_err(|_| TreasureError::Bounds)?;
            if id == 0 {
                return Err(TreasureError::Bounds);
            }
            ids.insert(id);
            if ids.len() > 16384 {
                return Err(TreasureError::Capacity);
            }
            Ok(())
        };
        if let Some(rows) = ace_tables::lookup(class, field) {
            for &(id, _) in rows {
                add(id)?;
            }
        } else if let Some(rows) = ace_tables::sequence(class, field) {
            for &id in rows {
                add(id)?;
            }
        } else if let Some(rows) = ace_tables::gem(class, field) {
            for &(id, _, _) in rows {
                add(id)?;
            }
        } else {
            return Err(missing());
        }
    }
    Ok(ids.into_iter().collect())
}
