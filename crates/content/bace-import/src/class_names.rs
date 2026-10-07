use std::{collections::BTreeMap, sync::OnceLock};

/// Matches pinned LifestonedConverter precedence: Adapter WeenieClassID,
/// then Entity WeenieClassName, then sanitized generated fallback.
pub(crate) fn class_name(id: u32, display_name: &str) -> String {
    static NAMES: OnceLock<BTreeMap<u32, &'static str>> = OnceLock::new();
    let names = NAMES.get_or_init(|| {
        include_str!("../data/weenie_class_names.tsv")
            .lines()
            .filter_map(|line| {
                let (id, name) = line.split_once('\t')?;
                Some((id.parse().ok()?, name))
            })
            .collect()
    });
    if let Some(name) = names.get(&id) {
        return (*name).into();
    }
    let clean: String = display_name
        .chars()
        .filter(|c| !"' .()+:_-,\"".contains(*c))
        .collect();
    format!("ace{id}-{}", clean.to_lowercase())
}
